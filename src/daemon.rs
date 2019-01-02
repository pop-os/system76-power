use dbus::tree::{Factory, MethodErr};
use dbus::{BusType, Connection, NameFlag};
use itertools::Itertools;
use std::borrow::Cow;
use std::cell::RefCell;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering, ATOMIC_BOOL_INIT};
use std::sync::Arc;

use config::{ActiveState, Config, ConfigPState, Profile, ProfileParameters};
use disks::{DiskPower, Disks};
use fan::{FanCurve, FanDaemon};
use graphics::Graphics;
use hotplug::HotPlugDetect;
use kernel_parameters::{DeviceList, Dirty, KernelParameter, LaptopMode, NmiWatchdog};
use pstate::PState;
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
        Ok(status) => if status.success() {
            info!("script at {:?} successfully executed", script);
        } else {
            warn!("script at {:?} failed with status: {:?}", script, status);
        },
        Err(why) => {
            warn!("script at {:?} failed to execute: {}", script, why);
        }
    }
}

/// Record every error that occurs, so that they may later be combined into an error of errors for the caller.
fn try<F: FnMut() -> io::Result<()>>(errors: &mut Vec<io::Error>, msg: &str, mut func: F) {
    if let Err(why) = func() {
        errors.push(io::Error::new(
            why.kind(),
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

    if let Some(ref graphics_profile) = config.graphics {
        match Graphics::new() {
            Ok(graphics) => try(errors, "failed to set graphics profile", || {
                graphics.set_power_profile(match graphics_profile.as_ref() {
                    "battery" => 0,
                    "balanced" => 1,
                    "performance" => 2,
                    profile => {
                        warn!("unknown graphics power profile: '{}'. Using 'balanced' instead.", profile);
                        1
                    }
                })
            }),
            Err(why) => errors.push(why)
        }
    }

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

    // A possibly-defined script to execute for this profile. For the "balannced" profile, this
    // would be at `/etc/system76-power/scripts/balanced`.
    let possible_script = PathBuf::from(
        [::config::CONFIG_PARENT, "scripts/", params.profile.as_ref()].concat()
    );

    if possible_script.exists() {
        execute_script(&possible_script)
    }

    *profile = params.profile.clone();
}

struct PowerDaemon {
    fan_daemon: Rc<RefCell<io::Result<FanDaemon>>>,
    graphics: Graphics,
    config: Config,
    active_state: ActiveState,
    errors: Vec<io::Error>,
}

impl PowerDaemon {
    fn new() -> Result<PowerDaemon, String> {
        let graphics = Graphics::new().map_err(err_str)?;

        let config = match Config::new() {
            Ok(config) => config,
            Err(why) => {
                error!(
                    "failed to read config file \
                    (defaults will be used, instead): {}",
                    why
                );
                Config::default()
            }
        };

        debug!("using this config: {:#?}", config);

        let mut active_state = match ActiveState::new() {
            Ok(config) => config,
            Err(why) => {
                error!(
                    "failed to read active profile config file \
                    (defaults will be used, instead): {}",
                    why
                );
                ActiveState::default()
            }
        };

        let active_curve = match config.fan_curves.get(&active_state.fan_curve).cloned() {
            Some(curve) => curve,
            None => {
                error!(
                    "fan curve profile, {}, was not found. Using the \
                    standard profile instead.",
                    &active_state.fan_curve
                );

                active_state.fan_curve = "default".into();

                FanCurve::standard()
            }
        };

        let fan_daemon = FanDaemon::new(active_curve);

        if let Err(why) = fan_daemon.as_ref() {
            warn!("fan daemon initialization failed: {}", why);
        }

        let errors = Vec::new();
        Ok(PowerDaemon {
            fan_daemon: Rc::new(RefCell::new(fan_daemon)),
            graphics,
            config,
            active_state,
            errors,
        })
    }

    fn set_profile_and_then<F: FnMut(&mut Self)>(&mut self, mut func: F) -> Result<(), String> {
        func(self);

        if let Err(why) = self.active_state.write() {
            error!("errored when writing config: {}", why);
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
    fn set_profile(&mut self, profile: &str) -> Result<(), String> {
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
                &mut d.active_state.power_profile,
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
                &mut d.active_state.power_profile,
                &mut d.errors,
                &d.config.profiles.performance,
                &PARAMETERS,
            )
        })
    }

    fn balanced(&mut self) -> Result<(), String> {
        static PARAMETERS: ProfileParameters = ProfileParameters {
            profile: Cow::Borrowed("balanced"),
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
                &mut d.active_state.power_profile,
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
                &mut d.active_state.power_profile,
                &mut d.errors,
                &d.config.profiles.battery,
                &PARAMETERS,
            )
        })
    }

    fn set_fan_curve(&mut self, profile: &str) -> Result<(), String> {
        match self.config.fan_curves.get(profile) {
            Some(profile) => {
                match *self.fan_daemon.borrow_mut() {
                    Ok(ref mut fan_daemon) => fan_daemon.set_curve(profile.clone()),
                    Err(_) => return Err("fan curve cannot be set because the fan daemon \
                        failed to initialize".into())
                }
            },
            None => return Err("fan curve profile not found".into())
        }

        self.active_state.fan_curve = Cow::Owned(profile.to_owned());
        Ok(())
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

    fn get_profile(&self) -> Result<String, String> {
        Ok(self.active_state.power_profile.to_string())
    }

    fn get_profiles(&self) -> Result<String, String> {
        Ok(self.config.profiles.get_profiles().join(" "))
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
        let last_profile = daemon.active_state.power_profile.clone();
        info!(
            "Initializing with previously-set profile: {}",
            last_profile
        );
        match last_profile.as_ref() {
            "battery" => daemon.battery(),
            "balanced" => daemon.balanced(),
            "performance" => daemon.performance(),
            profile => daemon.set_profile(profile)
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
        (has_out, $m:ident, $value:ident) => {
            $m.msg.method_return().append1($value)
        };

        (no_out, $m:ident, $value:ident) => {
            $m.msg.method_return()
        };
    }

    // Programs the message that should be printed.
    macro_rules! get_value {
        (has_in, $name:expr, $daemon:ident, $m:ident, $method:tt) => {{
            let value = $m.msg.read1()?;
            info!("DBUS Received {}({}) method", $name, value);
            $daemon.borrow_mut().$method(value)
        }};

        (no_in, $name:expr, $daemon:ident, $m:ident, $method:tt) => {{
            info!("DBUS Received {} method", $name);
            $daemon.borrow_mut().$method()
        }};
    }

    let signal_hotplug = Arc::new(f.signal("HotPlugDetect", ()).sarg::<u64, _>("port"));

    info!(
        "Adding dbus path {} with interface {}",
        DBUS_PATH, DBUS_IFACE
    );

    macro_rules! dbus_impl {
        (
            $(
                fn $method:tt<$name:tt, $append:tt, $hasvalue:tt>(
                    $( $inarg_name:tt : $inarg_type:ty ),*
                ) $( -> $($outarg_name:tt: $outarg_type:ty ),* )*;
            )*

            $(
                signal $signal:ident;
            )*
        ) => {{
            let interface = f.interface(DBUS_IFACE, ())
                $(
                    .add_m({
                        let daemon = daemon.clone();
                        f.method($name, (), move |m| {
                            let result = get_value!($hasvalue, $name, daemon, m, $method);
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
                        $(.inarg::<$inarg_type, _>(stringify!($inarg_name)))*
                        $($(.outarg::<$outarg_type, _>(stringify!($outarg_name)))*)*
                    })
                )*
                $(
                    .add_s($signal.clone())
                )*;

            f.tree(()).add(f.object_path(DBUS_PATH, ()).introspectable().add(interface))
        }}
    }

    let tree = dbus_impl! {
        // Power Profiles
        fn battery<"Battery", no_out, no_in>();
        fn balanced<"Balanced", no_out, no_in>();
        fn performance<"Performance", no_out, no_in>();

        fn get_profiles<"GetProfiles", has_out, no_in>() -> profiles: &str;
        fn get_profile<"GetProfile", has_out, no_in>() -> profile: &str;
        fn set_profile<"SetProfile", no_out, has_in>(profile: &str);

        // Fans
        fn set_fan_curve<"SetFanCurve", no_out, has_in>(profile: &str);

        // Graphics
        fn get_graphics<"GetGraphics", has_out, no_in>() -> vendor: &str;
        fn set_graphics<"SetGraphics", no_out, has_in>(vendor: &str);
        fn get_graphics_power<"GetGraphicsPower", has_out, no_in>() -> power: bool;
        fn set_graphics_power<"SetGraphicsPower", no_out, has_in>(power: bool);
        fn auto_graphics_power<"AutoGraphicsPower", no_out, no_in>();

        // Switchable graphics detection
        fn get_switchable<"GetSwitchable", has_out, no_in>() -> switchable: bool;

        signal signal_hotplug;
    };

    tree.set_registered(&c, true).map_err(err_str)?;

    c.add_handler(tree);

    let fan_daemon = daemon.borrow().fan_daemon.clone();

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

        if let Ok(ref fan_daemon) = *fan_daemon.borrow() {
            fan_daemon.step();
        }

        let hpd = hpd();
        for i in 0..hpd.len() {
            if hpd[i] != last[i] && hpd[i] {
                info!("HotPlugDetect {}", i);
                c.send(
                    signal_hotplug
                        .msg(&DBUS_PATH.into(), &DBUS_NAME.into())
                        .append1(i as u64),
                ).map_err(|()| "failed to send message".to_string())?;
            }
        }

        last = hpd;
    }
}
