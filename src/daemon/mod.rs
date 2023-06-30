// Copyright 2018-2021 System76 <info@system76.com>
//
// SPDX-License-Identifier: GPL-3.0-only

use dbus::{
    arg,
    channel::{MatchingReceiver, Sender},
    message::{MatchRule, Message},
    nonblock::SyncConnection,
};
use dbus_crossroads::{Crossroads, IfaceBuilder, MethodErr};
use dbus_tokio::connection;
use std::{
    fmt::Debug,
    fs,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};
use tokio::{
    signal::unix::{signal, SignalKind},
    time::sleep,
};

use futures::future::FutureExt;

use crate::{
    charge_thresholds::{
        get_charge_profiles, get_charge_thresholds, set_charge_thresholds, ChargeProfile,
    },
    err_str,
    errors::ProfileError,
    fan::FanDaemon,
    graphics::{Graphics, GraphicsMode},
    hid_backlight,
    hotplug::{mux, Detect, HotPlugDetect},
    kernel_parameters::{KernelParameter, NmiWatchdog},
    polkit,
    runtime_pm::runtime_pm_quirks,
    Power, DBUS_IFACE, DBUS_NAME, DBUS_PATH,
};

mod profiles;

use self::profiles::{balanced, battery, performance};

const THRESHOLD_POLICY: &str = "com.system76.powerdaemon.set-charge-thresholds";

static CONTINUE: AtomicBool = AtomicBool::new(true);

fn signal_handling() {
    let mut int = signal(SignalKind::interrupt()).unwrap();
    let mut hup = signal(SignalKind::hangup()).unwrap();
    let mut term = signal(SignalKind::terminate()).unwrap();

    tokio::spawn(async move {
        let sig = futures::select! {
            _ = int.recv().fuse() => "SIGINT",
            _ = hup.recv().fuse() => "SIGHUP",
            _ = term.recv().fuse() => "SIGTERM"
        };

        log::info!("caught signal: {}", sig);
        CONTINUE.store(false, Ordering::SeqCst);
    });
}

// Disabled by default because some systems have quirky ACPI tables that fail to resume from
// suspension.
static PCI_RUNTIME_PM: AtomicBool = AtomicBool::new(false);

// TODO: Whitelist system76 hardware that's known to work with this setting.
pub(crate) fn pci_runtime_pm_support() -> bool { PCI_RUNTIME_PM.load(Ordering::SeqCst) }

struct PowerDaemon {
    initial_set:     bool,
    graphics:        Graphics,
    power_profile:   String,
    profile_errors:  Vec<ProfileError>,
    dbus_connection: Arc<SyncConnection>,
}

impl PowerDaemon {
    fn new(dbus_connection: Arc<SyncConnection>) -> Result<PowerDaemon, String> {
        let graphics = Graphics::new().map_err(err_str)?;
        Ok(PowerDaemon {
            initial_set: false,
            graphics,
            power_profile: String::new(),
            profile_errors: Vec::new(),
            dbus_connection,
        })
    }

    fn apply_profile(
        &mut self,
        func: fn(&mut Vec<ProfileError>, bool),
        name: &str,
    ) -> Result<(), String> {
        if self.power_profile == name {
            log::info!("profile was already set");
            return Ok(());
        }

        func(&mut self.profile_errors, self.initial_set);

        let message =
            Message::new_signal(DBUS_PATH, DBUS_NAME, "PowerProfileSwitch").unwrap().append1(name);

        if let Err(()) = self.dbus_connection.send(message) {
            log::error!("failed to send power profile switch message");
        }

        self.power_profile = name.into();

        if self.profile_errors.is_empty() {
            Ok(())
        } else {
            let mut error_message = String::from("Errors found when setting profile:");
            for error in self.profile_errors.drain(..) {
                error_message = format!("{}\n    - {}", error_message, error);
            }

            Err(error_message)
        }
    }
}

impl Power for PowerDaemon {
    fn battery(&mut self) -> Result<(), String> {
        self.apply_profile(battery, "Battery").map_err(err_str)
    }

    fn balanced(&mut self) -> Result<(), String> {
        self.apply_profile(balanced, "Balanced").map_err(err_str)
    }

    fn performance(&mut self) -> Result<(), String> {
        self.apply_profile(performance, "Performance").map_err(err_str)
    }

    fn get_external_displays_require_dgpu(&mut self) -> Result<bool, String> {
        self.graphics.get_external_displays_require_dgpu().map_err(err_str)
    }

    fn get_default_graphics(&mut self) -> Result<String, String> {
        match self.graphics.get_default_graphics().map_err(err_str)? {
            GraphicsMode::Integrated => Ok("integrated".to_string()),
            GraphicsMode::Compute => Ok("compute".to_string()),
            GraphicsMode::Hybrid => Ok("hybrid".to_string()),
            GraphicsMode::Discrete => Ok("nvidia".to_string()),
        }
    }

    fn get_graphics(&mut self) -> Result<String, String> {
        match self.graphics.get_vendor().map_err(err_str)? {
            GraphicsMode::Integrated => Ok("integrated".to_string()),
            GraphicsMode::Compute => Ok("compute".to_string()),
            GraphicsMode::Hybrid => Ok("hybrid".to_string()),
            GraphicsMode::Discrete => Ok("nvidia".to_string()),
        }
    }

    fn get_profile(&mut self) -> Result<String, String> { Ok(self.power_profile.clone()) }

    fn get_switchable(&mut self) -> Result<bool, String> { Ok(self.graphics.can_switch()) }

    fn set_graphics(&mut self, vendor: &str) -> Result<(), String> {
        let vendor = match vendor {
            "nvidia" => GraphicsMode::Discrete,
            "hybrid" => GraphicsMode::Hybrid,
            "compute" => GraphicsMode::Compute,
            _ => GraphicsMode::Integrated,
        };

        self.graphics.set_vendor(vendor).map_err(err_str)
    }

    fn get_graphics_power(&mut self) -> Result<bool, String> {
        self.graphics.get_power().map_err(err_str)
    }

    fn set_graphics_power(&mut self, power: bool) -> Result<(), String> {
        self.graphics.set_power(power).map_err(err_str)
    }

    fn auto_graphics_power(&mut self) -> Result<(), String> {
        self.graphics.auto_power().map_err(err_str)
    }

    fn get_charge_thresholds(&mut self) -> Result<(u8, u8), String> { get_charge_thresholds() }

    fn set_charge_thresholds(&mut self, thresholds: (u8, u8)) -> Result<(), String> {
        // NOTE: This method is not actually called by daemon
        set_charge_thresholds(thresholds)
    }

    fn get_charge_profiles(&mut self) -> Result<Vec<ChargeProfile>, String> {
        Ok(get_charge_profiles())
    }
}

#[tokio::main(flavor = "current_thread")]
#[allow(clippy::too_many_lines)]
pub async fn daemon() -> Result<(), String> {
    signal_handling();
    let pci_runtime_pm = std::env::var("S76_POWER_PCI_RUNTIME_PM").ok().map_or(false, |v| v == "1");

    log::info!(
        "Starting daemon{}",
        if pci_runtime_pm { " with pci runtime pm support enabled" } else { "" }
    );
    PCI_RUNTIME_PM.store(pci_runtime_pm, Ordering::SeqCst);

    log::info!("Connecting to dbus system bus");
    let (resource, c) = connection::new_system_sync().map_err(err_str)?;

    tokio::spawn(async {
        let err = resource.await;
        panic!("Lost connection to D-Bus: {}", err);
    });

    let mut daemon = PowerDaemon::new(c.clone())?;
    let nvidia_exists = !daemon.graphics.nvidia.is_empty();

    log::info!("Disabling NMI Watchdog (for kernel debugging only)");
    NmiWatchdog::default().set(b"0");

    // Get the NVIDIA device ID before potentially removing it.
    let nvidia_device_id = if nvidia_exists {
        fs::read_to_string("/sys/bus/pci/devices/0000:01:00.0/device").ok()
    } else {
        None
    };

    log::info!("Setting automatic graphics power");
    match daemon.auto_graphics_power() {
        Ok(()) => (),
        Err(err) => {
            log::warn!("Failed to set automatic graphics power: {}", err);
        }
    }

    match runtime_pm_quirks() {
        Ok(()) => (),
        Err(err) => {
            log::warn!("Failed to set runtime power management quirks: {}", err);
        }
    }

    log::info!("Initializing with the balanced profile");
    if let Err(why) = daemon.balanced() {
        log::warn!("Failed to set initial profile: {}", why);
    }

    daemon.initial_set = true;

    log::info!("Registering dbus name {}", DBUS_NAME);
    c.request_name(DBUS_NAME, false, true, false).await.map_err(err_str)?;

    log::info!("Adding dbus path {} with interface {}", DBUS_PATH, DBUS_IFACE);
    let mut cr = Crossroads::new();
    cr.set_async_support(Some((
        c.clone(),
        Box::new(|x| {
            tokio::spawn(x);
        }),
    )));
    let iface_token = cr.register(DBUS_IFACE, |b| {
        sync_action_method(b, "Performance", PowerDaemon::performance);
        sync_action_method(b, "Balanced", PowerDaemon::balanced);
        sync_action_method(b, "Battery", PowerDaemon::battery);
        sync_get_method(
            b,
            "GetExternalDisplaysRequireDGPU",
            "required",
            PowerDaemon::get_external_displays_require_dgpu,
        );
        sync_get_method(b, "GetDefaultGraphics", "vendor", PowerDaemon::get_default_graphics);
        sync_get_method(b, "GetGraphics", "vendor", PowerDaemon::get_graphics);
        sync_set_method(b, "SetGraphics", "vendor", |d, s: String| d.set_graphics(&s));
        sync_get_method(b, "GetProfile", "profile", PowerDaemon::get_profile);
        sync_get_method(b, "GetSwitchable", "switchable", PowerDaemon::get_switchable);
        sync_get_method(b, "GetGraphicsPower", "power", PowerDaemon::get_graphics_power);
        sync_set_method(b, "SetGraphicsPower", "power", PowerDaemon::set_graphics_power);
        sync_get_method(b, "GetChargeThresholds", "thresholds", PowerDaemon::get_charge_thresholds);
        let c_clone = c.clone();
        b.method_with_cr_async(
            "SetChargeThresholds",
            ("thresholds",),
            (),
            move |mut ctx, _cr, (thresholds,): ((u8, u8),)| {
                let sender = ctx.message().sender().unwrap().into_static();
                let c = c_clone.clone();
                let res = async move {
                    let pid = polkit::get_connection_unix_process_id(&c, sender)
                        .await
                        .map_err(err_str)?;
                    let permitted = if pid == 0 {
                        true
                    } else {
                        polkit::check_authorization(&c, pid, 0, THRESHOLD_POLICY)
                            .await
                            .map_err(err_str)?
                    };
                    if permitted {
                        set_charge_thresholds(thresholds)?;
                        Ok(())
                    } else {
                        Err("Operation not permitted by Polkit".to_string())
                    }
                };
                async move { ctx.reply(res.await.map_err(|e| MethodErr::failed(&e))) }
            },
        );
        sync_get_method(b, "GetChargeProfiles", "profiles", PowerDaemon::get_charge_profiles);
        b.signal::<(u64,), _>("HotPlugDetect", ("port",));
        b.signal::<(&str,), _>("PowerProfileSwitch", ("profile",));
    });
    cr.insert(DBUS_PATH, &[iface_token], daemon);

    let cr = Arc::new(std::sync::Mutex::new(cr));
    c.start_receive(
        MatchRule::new_method_call(),
        Box::new(move |msg, c| {
            cr.lock().unwrap().handle_message(msg, c).unwrap();
            true
        }),
    );

    // Spawn hid backlight daemon
    let _hid_backlight = thread::spawn(hid_backlight::daemon);

    let mut fan_daemon = FanDaemon::new(nvidia_exists);

    let mut hpd_res = unsafe { HotPlugDetect::new(nvidia_device_id) };

    let mux_res = unsafe { mux::DisplayPortMux::new() };

    let mut hpd = || -> [bool; 4] {
        if let Ok(ref mut hpd) = hpd_res {
            unsafe { hpd.detect() }
        } else {
            [false; 4]
        }
    };

    let mut last = hpd();

    log::info!("Handling dbus requests");
    while CONTINUE.load(Ordering::SeqCst) {
        sleep(Duration::from_millis(1000)).await;

        fan_daemon.step();

        let hpd = hpd();
        for i in 0..hpd.len() {
            if hpd[i] != last[i] && hpd[i] {
                log::info!("HotPlugDetect {}", i);
                c.send(
                    Message::new_signal(DBUS_PATH, DBUS_NAME, "HotPlugDetect")
                        .unwrap()
                        .append1(i as u64),
                )
                .map_err(|()| "failed to send message".to_string())?;
            }
        }

        last = hpd;

        if let Ok(ref mux) = mux_res {
            unsafe {
                mux.step();
            }
        }
    }

    log::info!("daemon exited from loop");
    Ok(())
}

fn sync_method<IA, OA, F>(
    b: &mut IfaceBuilder<PowerDaemon>,
    name: &'static str,
    input_args: IA::strs,
    output_args: OA::strs,
    f: F,
) where
    IA: arg::ArgAll + arg::ReadAll + Debug,
    OA: arg::ArgAll + arg::AppendAll,
    F: Fn(&mut PowerDaemon, IA) -> Result<OA, String> + Send + 'static,
{
    b.method_with_cr(name, input_args, output_args, move |ctx, cr, args| {
        log::info!("DBUS Received {}{:?} method", name, args);
        match cr.data_mut(ctx.path()) {
            Some(daemon) => match f(daemon, args) {
                Ok(ret) => Ok(ret),
                Err(err) => Err(MethodErr::failed(&err)),
            },
            None => Err(MethodErr::no_path(ctx.path())),
        }
    });
}

/// `DBus` wrapper for a method taking no argument and returning no values
fn sync_action_method<F>(b: &mut IfaceBuilder<PowerDaemon>, name: &'static str, f: F)
where
    F: Fn(&mut PowerDaemon) -> Result<(), String> + Send + 'static,
{
    sync_method(b, name, (), (), move |d, _: ()| f(d));
}

/// `DBus` wrapper for method taking no arguments and returning one value
fn sync_get_method<T, F>(
    b: &mut IfaceBuilder<PowerDaemon>,
    name: &'static str,
    output_arg: &'static str,
    f: F,
) where
    T: arg::Arg + arg::Append + Debug,
    F: Fn(&mut PowerDaemon) -> Result<T, String> + Send + 'static,
{
    sync_method(b, name, (), (output_arg,), move |d, _: ()| f(d).map(|x| (x,)));
}

/// `DBus` wrapper for method taking one argument and returning no values
fn sync_set_method<T, F>(
    b: &mut IfaceBuilder<PowerDaemon>,
    name: &'static str,
    input_arg: &'static str,
    f: F,
) where
    T: arg::Arg + for<'z> arg::Get<'z> + Debug,
    F: Fn(&mut PowerDaemon, T) -> Result<(), String> + Send + 'static,
{
    sync_method(b, name, (input_arg,), (), move |d, (arg,)| f(d, arg));
}
