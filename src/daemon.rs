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
use config::{Config, ConfigProfile, Profile};
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

/// Sets the performance profile.
fn performance(config: &ConfigProfile, profile: &mut Profile) -> io::Result<()> {
    if experimental_is_enabled() {
        let disks = Disks::new();
        disks.set_apm_level(254)?;
        disks.set_autosuspend_delay(-1)?;

        ScsiHosts::new().set_power_management_policy(&["med_power_with_dipm", "max_performance"])?;
        SoundDevice::get_devices().for_each(|dev| dev.set_power_save(0, false));
        RadeonDevice::get_devices().for_each(|dev| dev.set_profiles("high", "performance", "auto"));
        for device in PciBus::new()?.devices()? {
            device.set_runtime_pm(RuntimePowerManagement::Off)?;
        }

        Dirty::new().set_max_lost_work(15);
        LaptopMode::new().set(b"0");
    }

    PState::new()?.set_config(config.pstate.as_ref(), (50, 100, true))?;

    if let Some(ref script) = config.script {
        execute_script(script);
    }

    *profile = Profile::Performance;

    Ok(())
}

fn balanced(config: &ConfigProfile, profile: &mut Profile) -> io::Result<()> {
    if experimental_is_enabled() {
        let disks = Disks::new();
        disks.set_apm_level(254)?;
        disks.set_autosuspend_delay(-1)?;

        ScsiHosts::new().set_power_management_policy(&["med_power_with_dipm", "medium_power"])?;
        SoundDevice::get_devices().for_each(|dev| dev.set_power_save(0, false));
        RadeonDevice::get_devices().for_each(|dev| dev.set_profiles("auto", "performance", "auto"));
        for device in PciBus::new()?.devices()? {
            device.set_runtime_pm(RuntimePowerManagement::On)?;
        }
        Dirty::new().set_max_lost_work(15);
        LaptopMode::new().set(b"0");
    }

    PState::new()?.set_config(config.pstate.as_ref(), (0, 100, true))?;

    for mut backlight in Backlight::all()? {
        backlight.set_if_lower(
            config.backlight.as_ref().map_or(40, |b| b.screen) as u64
        )?;
    }

    for mut backlight in KeyboardBacklight::all()? {
        backlight.set_if_lower(
            config.backlight.as_ref().map_or(50, |b| b.keyboard) as u64
        )?;
    }

    if let Some(ref script) = config.script {
        execute_script(script);
    }

    *profile = Profile::Balanced;

    Ok(())
}

fn battery(config: &ConfigProfile, profile: &mut Profile) -> io::Result<()> {
    if experimental_is_enabled() {
        let disks = Disks::new();
        disks.set_apm_level(128)?;
        disks.set_autosuspend_delay(15000)?;

        ScsiHosts::new().set_power_management_policy(&["min_power", "min_power"])?;
        SoundDevice::get_devices().for_each(|dev| dev.set_power_save(1, true));
        RadeonDevice::get_devices().for_each(|dev| dev.set_profiles("low", "battery", "low"));
        for device in PciBus::new()?.devices()? {
            device.set_runtime_pm(RuntimePowerManagement::On)?;
        }

        Dirty::new().set_max_lost_work(15);
        LaptopMode::new().set(b"2");
    }

    PState::new()?.set_config(config.pstate.as_ref(), (0, 50, false))?;

    for mut backlight in Backlight::all()? {
        backlight.set_if_lower(
            config.backlight.as_ref().map_or(10, |b| b.screen) as u64
        )?;
    }

    for mut backlight in KeyboardBacklight::all()? {
        backlight.set_if_lower(
            config.backlight.as_ref().map_or(0, |b| b.keyboard) as u64
        )?;
    }

    if let Some(ref script) = config.script {
        execute_script(script);
    }

    *profile = Profile::Battery;

    Ok(())
}

struct PowerDaemon {
    graphics: Graphics,
    config: Config,
}

impl PowerDaemon {
    fn new() -> Result<PowerDaemon, String> {
        let graphics = Graphics::new().map_err(err_str)?;
        let config = Config::new();
        Ok(PowerDaemon { graphics, config })
    }

    fn set_profile_and_then(&mut self, func: fn(&mut Self) -> Result<(), String>) -> Result<(), String> {
        let res = func(self);
        if let Err(why) = self.config.write() {
            error!("errored when writing config: {}", why);
        }
        res
    }
}

impl Power for PowerDaemon {
    fn performance(&mut self) -> Result<(), String> {
        self.set_profile_and_then(|d| {
            performance(&d.config.profiles.performance, &mut d.config.defaults.last_profile).map_err(err_str)
        })
    }

    fn balanced(&mut self) -> Result<(), String> {
        self.set_profile_and_then(|d| {
            balanced(&mut d.config.profiles.balanced, &mut d.config.defaults.last_profile).map_err(err_str)
        })
    }

    fn battery(&mut self) -> Result<(), String> {
        self.set_profile_and_then(|d| {
            battery(&mut d.config.profiles.battery, &mut d.config.defaults.last_profile).map_err(err_str)
        })
    }

    fn get_graphics(&mut self) -> Result<String, String> {
        self.graphics.get_vendor().map_err(err_str)
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
        let profiles = &daemon.config.profiles.clone();
        let last_profile = &mut daemon.config.defaults.last_profile;
        info!("Initializing with previously-set profile: {}", <&'static str>::from(*last_profile));
        match *last_profile {
            Profile::Battery => battery(&profiles.battery, last_profile),
            Profile::Balanced => balanced(&profiles.balanced, last_profile),
            Profile::Performance => performance(&profiles.performance, last_profile)
        }
    };

    res.map_err(|why| format!("failed to set initial profile: {}", why))?;

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
