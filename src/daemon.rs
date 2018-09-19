use dbus::{Connection, BusType, NameFlag};
use dbus::tree::{Factory, MethodErr};
use std::cell::RefCell;
use std::io;
use std::path::Path;
use std::process::Command;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{ATOMIC_BOOL_INIT, AtomicBool, Ordering};

use {DBUS_NAME, DBUS_PATH, DBUS_IFACE, Power, err_str};
use backlight::{Backlight, BacklightExt};
use config::{Config, ConfigProfile, Profile, ProfileParameters};
use disks::{Disks, DiskPower};
use graphics::Graphics;
use hotplug::HotPlugDetect;
use kbd_backlight::KeyboardBacklight;
use kernel_parameters::{DeviceList, Dirty, KernelParameter, LaptopMode, NmiWatchdog, RuntimePowerManagement};
use pci::PciBus;
use pstate::PState;
use radeon::RadeonDevice;
use scsi::{ScsiHosts, ScsiPower};
use snd::SoundDevice;
// use wifi::WifiDevice;

static EXPERIMENTAL: AtomicBool = ATOMIC_BOOL_INIT;

fn experimental_is_enabled() -> bool {
    EXPERIMENTAL.load(Ordering::SeqCst)
}

/// Executes an external script that is defined for a given profile.
fn execute_script(script: &Path) {
    match Command::new(script).status() {
        Ok(status) => if ! status.success() {
            warn!("balance script failed with status: {:?}", status);
        }
        Err(why) => {
            warn!("balance script failed to execute: {}", why);
        }
    }
}

/// Record every error that occurs, so that they may later be combined into an error of errors for the caller.
fn try<F: Fn() -> io::Result<()>>(errors: &mut Vec<io::Error>, msg: &str, func: F) {
    if let Err(why) = func() {
        errors.push(io::Error::new(
            io::ErrorKind::Other,
            format!("{}: {}", msg, why)
        ));
    }
}

fn apply_profile(
    profile: &mut Profile,
    errors: &mut Vec<io::Error>,
    config: &ConfigProfile,
    params: &ProfileParameters
) {
    if experimental_is_enabled() {
        let disks = Disks::new();
        try(errors, "failed to set disk apm level", || disks.set_apm_level(params.disk_apm));
        try(errors, "failed to set disk autosuspend delay", || disks.set_autosuspend_delay(params.disk_autosuspend_delay));
        try(errors, "failed to set SCSI power management policy", || {
            ScsiHosts::new().set_power_management_policy(params.scsi_profiles)
        });

        SoundDevice::get_devices().for_each(|dev| dev.set_power_save(
            params.sound_power_save.0,
            params.sound_power_save.1
        ));

        RadeonDevice::get_devices().for_each(|dev| dev.set_profiles(
            params.radeon_profile,
            params.radeon_dpm_state,
            params.radeon_dpm_perf
        ));

        try(errors, "failed to change PCI device runtime power management", || {
            for device in PciBus::new()?.devices()? {
                device.set_runtime_pm(params.pci_runtime_pm)?;
            }

            Ok(())
        });
        

        Dirty::new().set_max_lost_work(params.max_lost_work);
        LaptopMode::new().set(params.laptop_mode);
    }

    try(errors, "failed to set Intel PState settings", || {
        PState::new()?.set_config(config.pstate.as_ref(), params.pstate_defaults)
    });

    if let Some(default_brightness) = params.backlight_screen {
        try(errors, "failed to set screen backlight", || {
            for mut backlight in Backlight::all()? {
                backlight.set_if_lower(
                    config.backlight.as_ref().map_or(default_brightness, |b| b.screen) as u64
                )?;
            }

            Ok(())
        });
    }

    if let Some(default_brightness) = params.backlight_keyboard {
        try(errors, "failed to set keyboard backlight", || {
            for mut backlight in KeyboardBacklight::all()? {
                backlight.set_if_lower(
                    config.backlight.as_ref().map_or(default_brightness, |b| b.keyboard) as u64
                )?;
            }

            Ok(())
        });
    }

    if let Some(ref script) = config.script {
        execute_script(script);
    }

    *profile = params.profile;
}

struct PowerDaemon {
    graphics: Graphics,
    config: Config,
    errors: Vec<io::Error>,
}

impl PowerDaemon {
    fn new() -> Result<PowerDaemon, String> {
        let graphics = Graphics::new().map_err(err_str)?;
        let config = Config::new();
        let errors = Vec::new();
        Ok(PowerDaemon { graphics, config, errors })
    }

    fn set_profile_and_then(&mut self, func: fn(&mut Self)) -> Result<(), String> {
        func(self);
        if let Err(why) = self.config.write() {
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
    fn performance(&mut self) -> Result<(), String> {
        static PARAMETERS: ProfileParameters = ProfileParameters {
            profile: Profile::Performance,
            disk_apm: 254,
            disk_autosuspend_delay: -1,
            scsi_profiles: &["med_power_with_dipm", "max_performance"],
            sound_power_save: (0, false),
            radeon_profile: "high",
            radeon_dpm_state: "performance",
            radeon_dpm_perf: "auto",
            pci_runtime_pm: RuntimePowerManagement::Off,
            max_lost_work: 15,
            laptop_mode: b"0",
            pstate_defaults: (50, 100, true),
            backlight_screen: None,
            backlight_keyboard: None,
        };

        self.set_profile_and_then(|d| {
            apply_profile(
                &mut d.config.defaults.last_profile,
                &mut d.errors,
                &d.config.profiles.performance,
                &PARAMETERS
            )
        })
    }

    fn balanced(&mut self) -> Result<(), String> {
        static PARAMETERS: ProfileParameters = ProfileParameters {
            profile: Profile::Balanced,
            disk_apm: 254,
            disk_autosuspend_delay: -1,
            scsi_profiles: &["med_power_with_dipm", "medium_power"],
            sound_power_save: (0, false),
            radeon_profile: "auto",
            radeon_dpm_state: "performance",
            radeon_dpm_perf: "auto",
            pci_runtime_pm: RuntimePowerManagement::Off,
            max_lost_work: 15,
            laptop_mode: b"0",
            pstate_defaults: (0, 100, true),
            backlight_screen: Some(80),
            backlight_keyboard: Some(50),
        };

        self.set_profile_and_then(|d| {
            apply_profile(
                &mut d.config.defaults.last_profile,
                &mut d.errors,
                &d.config.profiles.balanced,
                &PARAMETERS
            )
        })
    }

    fn battery(&mut self) -> Result<(), String> {
        static PARAMETERS: ProfileParameters = ProfileParameters {
            profile: Profile::Battery,
            disk_apm: 128,
            disk_autosuspend_delay: 15000,
            scsi_profiles: &["min_power", "min_power"],
            sound_power_save: (1, true),
            radeon_profile: "low",
            radeon_dpm_state: "battery",
            radeon_dpm_perf: "low",
            pci_runtime_pm: RuntimePowerManagement::On,
            max_lost_work: 15,
            laptop_mode: b"2",
            pstate_defaults: (0, 50, false),
            backlight_screen: Some(10),
            backlight_keyboard: Some(0),
        };

        self.set_profile_and_then(|d| {
            apply_profile(
                &mut d.config.defaults.last_profile,
                &mut d.errors,
                &d.config.profiles.battery,
                &PARAMETERS
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
    info!("Starting daemon{}", if experimental { " with experimental enabled" } else { "" });
    EXPERIMENTAL.store(experimental, Ordering::SeqCst);

    info!("Disabling NMI Watchdog (for kernel debugging only)");
    NmiWatchdog::new().set(b"0");

    info!("Setting automatic graphics power");
    match daemon.borrow_mut().auto_graphics_power() {
        Ok(()) => (),
        Err(err) => {
            error!("Failed to set automatic graphics power: {}", err);
        }
    }

    let res = {
        let mut daemon = daemon.borrow_mut();
        let last_profile = daemon.config.defaults.last_profile;
        info!("Initializing with previously-set profile: {}", <&'static str>::from(last_profile));
        match last_profile {
            Profile::Battery => daemon.battery(),
            Profile::Balanced => daemon.balanced(),
            Profile::Performance => daemon.performance()
        }
    };

    if let Err(why) = res {
        eprintln!("failed to set initial profile: {}", why);
    }

    info!("Connecting to dbus system bus");
    let c = Connection::get_private(BusType::System).map_err(err_str)?;

    info!("Registering dbus name {}", DBUS_NAME);
    c.register_name(DBUS_NAME, NameFlag::ReplaceExisting as u32).map_err(err_str)?;

    let f = Factory::new_fn::<()>();

    // Defines whether the value returned by the method should be appended.
    macro_rules! append {
        (true, $m:ident, $value:ident) => { $m.msg.method_return().append1($value) };
        (false, $m:ident, $value:ident) => { $m.msg.method_return() };
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
                    },
                    Err(err) => {
                        error!("{}", err);
                        Err(MethodErr::failed(&err))
                    }
                }
            })
        }};
    }

    let signal = Arc::new(f.signal("HotPlugDetect", ()).sarg::<u64,_>("port"));

    info!("Adding dbus path {} with interface {}", DBUS_PATH, DBUS_IFACE);
    let tree = f.tree(()).add(f.object_path(DBUS_PATH, ()).introspectable().add(
        f.interface(DBUS_IFACE, ())
            .add_m(method!(performance, "Performance", false, false))
            .add_m(method!(balanced, "Balanced", false, false))
            .add_m(method!(battery, "Battery", false, false))
            .add_m(method!(get_graphics, "GetGraphics", true, false).outarg::<&str,_>("vendor"))
            .add_m(method!(set_graphics, "SetGraphics", false, true).inarg::<&str,_>("vendor"))
            .add_m(method!(get_switchable, "GetSwitchable", true, false).outarg::<bool, _>("switchable"))
            .add_m(method!(get_graphics_power, "GetGraphicsPower", true, false).outarg::<bool,_>("power"))
            .add_m(method!(set_graphics_power, "SetGraphicsPower", false, true).inarg::<bool,_>("power"))
            .add_m(method!(auto_graphics_power, "AutoGraphicsPower", false, false))
            .add_s(signal.clone())
    ));

    tree.set_registered(&c, true).map_err(err_str)?;

    c.add_handler(tree);

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

        let hpd = hpd();
        for i in 0..hpd.len() {
            if hpd[i] != last[i] {
                if hpd[i] {
                    info!("HotPlugDetect {}", i);
                    c.send(
                        signal.msg(&DBUS_PATH.into(), &DBUS_NAME.into()).append1(i as u64)
                    ).map_err(|()| format!("failed to send message"))?;
                }
            }
        }

        last = hpd;
    }
}
