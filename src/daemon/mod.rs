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
    time::Duration,
};

use futures::future::FutureExt;
use futures_lite::StreamExt;
use tokio::time::sleep;

use crate::{
    charge_thresholds::{
        get_charge_profiles, get_charge_thresholds, set_charge_thresholds, ChargeProfile,
    },
    cpufreq, err_str,
    errors::ProfileError,
    fan::FanDaemon,
    graphics::{Graphics, GraphicsMode},
    hid_backlight, hotplug,
    kernel_parameters::{KernelParameter, NmiWatchdog},
    polkit, Power, DBUS_IFACE, DBUS_NAME, DBUS_PATH,
};

mod interrupt;
use self::interrupt::CONTINUE;

mod profiles;
use self::profiles::{balanced, battery, performance};

const THRESHOLD_POLICY: &str = "com.system76.powerdaemon.set-charge-thresholds";

// Disabled by default because some systems have quirky ACPI tables that fail to resume from
// suspension.
static PCI_RUNTIME_PM: AtomicBool = AtomicBool::new(false);

// TODO: Whitelist system76 hardware that's known to work with this setting.
pub(crate) fn pci_runtime_pm_support() -> bool { PCI_RUNTIME_PM.load(Ordering::SeqCst) }

struct PowerDaemon {
    initial_set:     bool,
    on_battery:      bool,
    graphics:        Graphics,
    power_profile:   String,
    dbus_connection: Arc<SyncConnection>,
}

impl PowerDaemon {
    fn new(dbus_connection: Arc<SyncConnection>) -> Result<PowerDaemon, String> {
        let graphics = Graphics::new().map_err(err_str)?;
        Ok(PowerDaemon {
            initial_set: false,
            on_battery: false,
            graphics,
            power_profile: String::new(),
            dbus_connection,
        })
    }

    fn apply_profile(
        &mut self,
        func: fn(&mut Vec<ProfileError>, bool, bool),
        name: &str,
    ) -> Result<(), String> {
        if self.power_profile == name {
            log::info!("profile was already set");
            return Ok(());
        }

        let mut profile_errors = Vec::new();

        func(&mut profile_errors, self.on_battery, self.initial_set);

        let message =
            Message::new_signal(DBUS_PATH, DBUS_NAME, "PowerProfileSwitch").unwrap().append1(name);

        if let Err(()) = self.dbus_connection.send(message) {
            log::error!("failed to send power profile switch message");
        }

        self.power_profile = name.into();

        if profile_errors.is_empty() {
            Ok(())
        } else {
            let mut error_message = String::from("Errors found when setting profile:");
            for error in profile_errors.drain(..) {
                error_message = format!("{}\n    - {}", error_message, error);
            }

            Err(error_message)
        }
    }

    /// Called when the status changes between AC and battery.
    ///
    /// We want to disable CPU frequency boosting if the system is on
    /// battery power when the Battery profile is in use.
    fn on_battery_changed(&mut self, on_battery: bool) {
        self.on_battery = on_battery;

        if self.power_profile == "Battery" {
            // intel_pstate has its own mechanism for managing boost.
            if let Ok(pstate) = intel_pstate::PState::new() {
                let _res = pstate.set_no_turbo(on_battery);

                return;
            }

            if on_battery {
                cpufreq::boost::disable();
            } else {
                cpufreq::boost::enable();
            }
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
    interrupt::handle();
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

    let mut on_battery_stream = on_battery_stream(&mut daemon).await;

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

    {
        let cr = cr.clone();
        c.start_receive(
            MatchRule::new_method_call(),
            Box::new(move |msg, c| {
                cr.lock().unwrap().handle_message(msg, c).unwrap();
                true
            }),
        );
    }

    // Spawn the HID backlight daemon.
    let _hid_backlight_task = tokio::spawn(hid_backlight::daemon());

    // Initialize the hotplug signal emitter.
    let mut hotplug_emitter = hotplug::Emitter::new(nvidia_device_id);

    // Initialize the fan management daemon.
    let mut fan_daemon = FanDaemon::new(nvidia_exists);

    while CONTINUE.load(Ordering::SeqCst) {
        sleep(Duration::from_millis(1000)).await;

        // Notify the daemon on battery status changes
        if let Some(stream) = on_battery_stream.as_mut() {
            if let Some(Some(property_changed)) = stream.next().now_or_never() {
                if let Ok(on_battery) = property_changed.get().await {
                    if let Some(daemon) =
                        cr.lock().unwrap().data_mut::<PowerDaemon>(&dbus::Path::from(DBUS_PATH))
                    {
                        daemon.on_battery_changed(on_battery);
                    }
                }
            }
        }

        fan_daemon.step();

        for id in hotplug_emitter.emit_on_detect() {
            log::info!("HotPlugDetect {}", id);
            let result = c.send(
                Message::new_signal(DBUS_PATH, DBUS_NAME, "HotPlugDetect")
                    .unwrap()
                    .append1(id as u64),
            );

            if result.is_err() {
                log::error!("failed to send HotPlugDetect signal");
            }
        }

        hotplug_emitter.mux_step();
    }

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

/// Create a stream for listening to battery AC events.
async fn on_battery_stream(
    daemon: &mut PowerDaemon,
) -> Option<zbus::PropertyStream<'static, bool>> {
    if let Ok(connection) = zbus::Connection::system().await {
        if let Ok(proxy) = upower_dbus::UPowerProxy::new(&connection).await {
            if let Ok(on_battery) = proxy.on_battery().await {
                daemon.on_battery_changed(on_battery);
                return Some(proxy.receive_on_battery_changed().await);
            }
        }
    }

    None
}
