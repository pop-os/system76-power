use dbus::tree::{Factory, MethodErr};
use dbus::{BusType, Connection, NameFlag};
use std::borrow::Cow;
use std::cell::RefCell;
use std::io;
use std::path::Path;
use std::process::Command;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering, ATOMIC_BOOL_INIT};
use std::sync::Arc;

use config::{Config, ConfigPState, Profile, ProfileParameters};
use disks::{DiskPower, Disks};
use fan::FanDaemon;
use graphics::Graphics;
use hotplug::HotPlugDetect;
use kernel_parameters::{DeviceList, Dirty, KernelParameter, LaptopMode, NmiWatchdog};
use pstate::PState;
use radeon::RadeonDevice;
use snd::SoundDevice;
use sysfs_class::{
    Backlight, Leds, PciDevice, RuntimePM, RuntimePowerManagement, ScsiHost, SysClass,
};
use {err_str, Power, DBUS_IFACE, DBUS_NAME, DBUS_PATH};
// use wifi::WifiDevice;

static EXPERIMENTAL: AtomicBool = ATOMIC_BOOL_INIT;

fn experimental_is_enabled() -> bool {
    EXPERIMENTAL.load(Ordering::SeqCst)
}

/// Executes an external script that is defined for a given profile.
fn execute_script(script: &Path) {
    match Command::new(script).status() {
        Ok(status) => if !status.success() {
            warn!("balance script failed with status: {:?}", status);
        },
        Err(why) => {
            warn!("balance script failed to execute: {}", why);
        }
    }
}

/// Record every error that occurs, so that they may later be combined into an error of errors for the caller.
fn try<F: FnMut() -> io::Result<()>>(errors: &mut Vec<io::Error>, msg: &str, mut func: F) {
    if let Err(why) = func() {
        errors.push(io::Error::new(
            io::ErrorKind::Other,
            format!("{}: {}", msg, why),
        ));
    }
}

fn apply_profile(
    profile: &mut Cow<'static, str>,
    errors: &mut Vec<io::Error>,
    config: &Profile,
    params: &ProfileParameters,
) {
    if experimental_is_enabled() {
        let disks = Disks::new();
        try(errors, "failed to set disk apm level", || {
            disks.set_apm_level(params.disk_apm)
        });
        try(errors, "failed to set disk autosuspend delay", || {
            disks.set_autosuspend_delay(params.disk_autosuspend_delay)
        });
        try(errors, "failed to set SCSI power management policy", || {
            for host in ScsiHost::iter() {
                host?.set_link_power_management_policy(params.scsi_profiles)?;
            }

            Ok(())
        });

        SoundDevice::get_devices().for_each(|dev| {
            dev.set_power_save(params.sound_power_save.0, params.sound_power_save.1)
        });

        if let Some(ref radeon) = config.radeon {
            RadeonDevice::get_devices().for_each(|dev| {
                dev.set_profiles(
                    &radeon.profile,
                    &radeon.dpm_state,
                    &radeon.dpm_perf,
                )
            });
        }

        if let Some(ref pci) = config.pci {
            let pm = if pci.runtime_pm {
                RuntimePowerManagement::On
            } else {
                RuntimePowerManagement::Off
            };

            try(
                errors,
                "failed to change PCI device runtime power management",
                || {
                    for device in PciDevice::all()? {
                        device.set_runtime_pm(pm)?;
                    }

                    Ok(())
                },
            );
        }
    }

    Dirty::new().set_max_lost_work(config.max_lost_work);
    LaptopMode::new().set(config.laptop_mode.to_string().as_bytes());

    if let Ok(pstate) = PState::new() {
        try(errors, "failed to set Intel PState settings", || {
            pstate.set_values(
                config
                    .pstate
                    .clone()
                    .unwrap_or_else(|| params.pstate_defaults.clone())
                    .into(),
            )
        });
    }

    if let Some(default_brightness) = params.backlight_screen {
        try(errors, "failed to set screen backlight", || {
            for mut backlight in Backlight::iter() {
                let backlight = backlight?;

                let new = config
                    .backlight
                    .as_ref()
                    .map_or(default_brightness, |b| b.screen);

                let max_brightness = backlight.max_brightness()?;
                let current = backlight.brightness()?;
                let new = max_brightness * u64::from(new) / 100;

                if new < current {
                    backlight.set_brightness(new)?;
                }
            }

            Ok(())
        });
    }

    if let Some(default_brightness) = params.backlight_keyboard {
        try(errors, "failed to set keyboard backlight", || {
            for mut backlight in Leds::keyboard_backlights() {
                let backlight = backlight?;

                let new = config
                    .backlight
                    .as_ref()
                    .map_or(default_brightness, |b| b.keyboard);

                let max_brightness = backlight.max_brightness()?;
                let current = backlight.brightness()?;
                let new = max_brightness * u64::from(new) / 100;

                if new < current {
                    backlight.set_brightness(new)?;
                }
            }

            Ok(())
        });
    }

    if let Some(ref script) = config.script {
        execute_script(script);
    }

    *profile = params.profile.clone();
}

struct PowerDaemon {
    graphics: Graphics,
    config: Config,
    errors: Vec<io::Error>,
    overwrite_config: bool,
}

impl PowerDaemon {
    fn new() -> Result<PowerDaemon, String> {
        let graphics = Graphics::new().map_err(err_str)?;
        let mut overwrite_config = true;
        let config = match Config::new() {
            Ok(config) => config,
            Err(why) => {
                error!(
                    "failed to read config file (defaults will be used, instead): {}",
                    why
                );
                overwrite_config = false;
                Config::default()
            }
        };

        debug!("using this config: {:#?}", config);

        let errors = Vec::new();
        Ok(PowerDaemon {
            graphics,
            config,
            errors,
            overwrite_config,
        })
    }

    fn set_profile_and_then<F: FnMut(&mut Self)>(&mut self, mut func: F) -> Result<(), String> {
        func(self);

        if self.overwrite_config {
            if let Err(why) = self.config.write() {
                error!("errored when writing config: {}", why);
            }
        }

        if self.errors.is_empty() {
            return Ok(());
        }

        let mut message = String::from("error(s) occurred when setting profile:\n");
        for error in self.errors.drain(..) {
            message.push_str(format!("    {}", error).as_str());
        }

        Err(message)
    }
}

impl Power for PowerDaemon {
    fn custom(&mut self, profile: &str) -> Result<(), String> {
        let profile_ = self.config.profiles.custom.get(profile)
            .cloned()
            .ok_or_else(|| format!("{} is not a known profile", profile))?;

        let profile_parameters = ProfileParameters {
            profile: Cow::Owned(profile.to_owned()),
            disk_apm: 254,
            disk_autosuspend_delay: -1,
            scsi_profiles: &["med_power_with_dipm", "medium_power"],
            sound_power_save: (0, false),
            pstate_defaults: ConfigPState {
                min: 0,
                max: 100,
                turbo: true,
            },
            backlight_screen: Some(80),
            backlight_keyboard: Some(50),
        };

        self.set_profile_and_then(|d| {
            apply_profile(
                &mut d.config.defaults.last_profile,
                &mut d.errors,
                &profile_,
                &profile_parameters,
            )
        })
    }

    fn performance(&mut self) -> Result<(), String> {
        static PARAMETERS: ProfileParameters = ProfileParameters {
            profile: Cow::Borrowed("performance"),
            disk_apm: 254,
            disk_autosuspend_delay: -1,
            scsi_profiles: &["med_power_with_dipm", "max_performance"],
            sound_power_save: (0, false),
            pstate_defaults: ConfigPState {
                min: 50,
                max: 100,
                turbo: true,
            },
            backlight_screen: None,
            backlight_keyboard: None,
        };

        self.set_profile_and_then(|d| {
            apply_profile(
                &mut d.config.defaults.last_profile,
                &mut d.errors,
                &d.config.profiles.performance,
                &PARAMETERS,
            )
        })
    }

    fn balanced(&mut self) -> Result<(), String> {
        static PARAMETERS: ProfileParameters = ProfileParameters {
            profile: Cow::Borrowed("battery"),
            disk_apm: 254,
            disk_autosuspend_delay: -1,
            scsi_profiles: &["med_power_with_dipm", "medium_power"],
            sound_power_save: (0, false),
            pstate_defaults: ConfigPState {
                min: 0,
                max: 100,
                turbo: true,
            },
            backlight_screen: Some(80),
            backlight_keyboard: Some(50),
        };

        self.set_profile_and_then(|d| {
            apply_profile(
                &mut d.config.defaults.last_profile,
                &mut d.errors,
                &d.config.profiles.balanced,
                &PARAMETERS,
            )
        })
    }

    fn battery(&mut self) -> Result<(), String> {
        static PARAMETERS: ProfileParameters = ProfileParameters {
            profile: Cow::Borrowed("battery"),
            disk_apm: 128,
            disk_autosuspend_delay: 15000,
            scsi_profiles: &["min_power", "min_power"],
            sound_power_save: (1, true),
            pstate_defaults: ConfigPState {
                min: 0,
                max: 50,
                turbo: false,
            },
            backlight_screen: Some(10),
            backlight_keyboard: Some(0),
        };

        self.set_profile_and_then(|d| {
            apply_profile(
                &mut d.config.defaults.last_profile,
                &mut d.errors,
                &d.config.profiles.battery,
                &PARAMETERS,
            )
        })
    }

    fn get_graphics(&mut self) -> Result<String, String> {
        self.graphics.get_vendor().map_err(err_str)
    }

    fn get_switchable(&mut self) -> Result<bool, String> {
        Ok(self.graphics.can_switch())
    }

    fn set_graphics(&mut self, vendor: &str) -> Result<(), String> {
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
}

pub fn daemon(experimental: bool) -> Result<(), String> {
    let daemon = Rc::new(RefCell::new(PowerDaemon::new()?));
    let experimental = experimental || daemon.borrow().config.defaults.experimental;
    info!(
        "Starting daemon{}",
        if experimental {
            " with experimental enabled"
        } else {
            ""
        }
    );
    EXPERIMENTAL.store(experimental, Ordering::SeqCst);

    info!("Disabling NMI Watchdog (for kernel debugging only)");
    NmiWatchdog::new().set(b"0");

    info!("Setting automatic graphics power");
    if let Err(err) = daemon.borrow_mut().auto_graphics_power() {
        error!("Failed to set automatic graphics power: {}", err);
    }

    let res = {
        let mut daemon = daemon.borrow_mut();
        let last_profile = daemon.config.defaults.last_profile.clone();
        info!(
            "Initializing with previously-set profile: {}",
            last_profile
        );
        match last_profile.as_ref() {
            "battery" => daemon.battery(),
            "balanced" => daemon.balanced(),
            "performance" => daemon.performance(),
            profile => daemon.custom(profile)
        }
    };

    if let Err(why) = res {
        eprintln!("failed to set initial profile: {}", why);
    }

    info!("Connecting to dbus system bus");
    let c = Connection::get_private(BusType::System).map_err(err_str)?;

    info!("Registering dbus name {}", DBUS_NAME);
    c.register_name(DBUS_NAME, NameFlag::ReplaceExisting as u32)
        .map_err(err_str)?;

    let f = Factory::new_fn::<()>();

    // Defines whether the value returned by the method should be appended.
    macro_rules! append {
        (true, $m:ident, $value:ident) => {
            $m.msg.method_return().append1($value)
        };
        (false, $m:ident, $value:ident) => {
            $m.msg.method_return()
        };
    }

    // Programs the message that should be printed.
    macro_rules! get_value {
        (true, $name:expr, $daemon:ident, $m:ident, $method:tt) => {{
            let value = $m.msg.read1()?;
            info!("DBUS Received {}({}) method", $name, value);
            $daemon.borrow_mut().$method(value)
        }};

        (false, $name:expr, $daemon:ident, $m:ident, $method:tt) => {{
            info!("DBUS Received {} method", $name);
            $daemon.borrow_mut().$method()
        }};
    }

    // Creates a new dbus method from an existing method in the daemon.
    macro_rules! method {
        ($method:tt, $name:expr, $append:tt, $print:tt) => {{
            let daemon = daemon.clone();
            f.method($name, (), move |m| {
                let result = get_value!($print, $name, daemon, m, $method);
                match result {
                    Ok(_value) => {
                        let mret = append!($append, m, _value);
                        Ok(vec![mret])
                    }
                    Err(err) => {
                        error!("{}", err);
                        Err(MethodErr::failed(&err))
                    }
                }
            })
        }};
    }

    let signal = Arc::new(f.signal("HotPlugDetect", ()).sarg::<u64, _>("port"));

    info!(
        "Adding dbus path {} with interface {}",
        DBUS_PATH, DBUS_IFACE
    );
    let tree = f.tree(()).add(
        f.object_path(DBUS_PATH, ()).introspectable().add(
            f.interface(DBUS_IFACE, ())
                .add_m(method!(custom, "Custom", false, true).inarg::<&str, _>("profile"))
                .add_m(method!(performance, "Performance", false, false))
                .add_m(method!(balanced, "Balanced", false, false))
                .add_m(method!(battery, "Battery", false, false))
                .add_m(method!(get_graphics, "GetGraphics", true, false).outarg::<&str, _>("vendor"))
                .add_m(method!(set_graphics, "SetGraphics", false, true).inarg::<&str, _>("vendor"))
                .add_m(
                    method!(get_switchable, "GetSwitchable", true, false)
                        .outarg::<bool, _>("switchable"),
                )
                .add_m(
                    method!(get_graphics_power, "GetGraphicsPower", true, false)
                        .outarg::<bool, _>("power"),
                )
                .add_m(
                    method!(set_graphics_power, "SetGraphicsPower", false, true)
                        .inarg::<bool, _>("power"),
                )
                .add_m(method!(
                    auto_graphics_power,
                    "AutoGraphicsPower",
                    false,
                    false
                ))
                .add_s(signal.clone()),
        ),
    );

    tree.set_registered(&c, true).map_err(err_str)?;

    c.add_handler(tree);

    let fan_daemon_res = FanDaemon::new(daemon.borrow().config.fan_curves.get_active());

    if let Err(ref err) = fan_daemon_res {
        error!("fan daemon: {}", err);
    }

    let hpd_res = unsafe { HotPlugDetect::new() };

    let hpd = || -> [bool; 3] {
        if let Ok(ref hpd) = hpd_res {
            unsafe { hpd.detect() }
        } else {
            [false; 3]
        }
    };

    let mut last = hpd();

    info!("Handling dbus requests");
    loop {
        c.incoming(1000).next();

        if let Ok(ref fan_daemon) = fan_daemon_res {
            fan_daemon.step();
        }

        let hpd = hpd();
        for i in 0..hpd.len() {
            if hpd[i] != last[i] && hpd[i] {
                info!("HotPlugDetect {}", i);
                c.send(
                    signal
                        .msg(&DBUS_PATH.into(), &DBUS_NAME.into())
                        .append1(i as u64),
                ).map_err(|()| "failed to send message".to_string())?;
            }
        }

        last = hpd;
    }
}
