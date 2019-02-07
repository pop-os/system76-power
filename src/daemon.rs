use dbus::{Connection, BusType, NameFlag};
use dbus::tree::{Factory, MethodErr};
use std::cell::RefCell;
use std::io;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{ATOMIC_BOOL_INIT, AtomicBool, Ordering};

use {DBUS_NAME, DBUS_PATH, DBUS_IFACE, Power, err_str};
use disks::{Disks, DiskPower};
use fan::FanDaemon;
use graphics::Graphics;
use hotplug::HotPlugDetect;
use kernel_parameters::{DeviceList, Dirty, KernelParameter, LaptopMode, NmiWatchdog};
use pstate::PState;
use radeon::RadeonDevice;
use snd::SoundDevice;
use sysfs_class::{Backlight, Leds, PciDevice, RuntimePM, RuntimePowerManagement, ScsiHost, SysClass};
// use wifi::WifiDevice;

static EXPERIMENTAL: AtomicBool = ATOMIC_BOOL_INIT;

fn experimental_is_enabled() -> bool {
    EXPERIMENTAL.load(Ordering::SeqCst)
}

fn performance() -> io::Result<()> {
    if experimental_is_enabled() {
        let disks = Disks::new();
        disks.set_apm_level(254)?;
        disks.set_autosuspend_delay(-1)?;

        for host in ScsiHost::iter() {
            host?.set_link_power_management_policy(&["med_power_with_dipm", "max_performance"])?;
        }

        SoundDevice::get_devices().for_each(|dev| dev.set_power_save(0, false));
        RadeonDevice::get_devices().for_each(|dev| dev.set_profiles("high", "performance", "auto"));
        for device in PciDevice::all()? {
            device.set_runtime_pm(RuntimePowerManagement::Off)?;
        }

        Dirty::new().set_max_lost_work(15);
        LaptopMode::new().set(b"0");
    }

    if let Ok(pstate) = PState::new() {
        pstate.set_min_perf_pct(50)?;
        pstate.set_max_perf_pct(100)?;
        pstate.set_no_turbo(false)?;
    }

    Ok(())
}

fn balanced() -> io::Result<()> {
    if experimental_is_enabled() {
        let disks = Disks::new();
        disks.set_apm_level(254)?;
        disks.set_autosuspend_delay(-1)?;

        for host in ScsiHost::iter() {
            host?.set_link_power_management_policy(&["med_power_with_dipm", "medium_power"])?;
        }

        SoundDevice::get_devices().for_each(|dev| dev.set_power_save(0, false));
        RadeonDevice::get_devices().for_each(|dev| dev.set_profiles("auto", "performance", "auto"));
        for device in PciDevice::all()? {
            device.set_runtime_pm(RuntimePowerManagement::On)?;
        }

        Dirty::new().set_max_lost_work(15);
        LaptopMode::new().set(b"0");
    }

    if let Ok(pstate) = PState::new() {
        pstate.set_min_perf_pct(0)?;
        pstate.set_max_perf_pct(100)?;
        pstate.set_no_turbo(false)?;
    }

    for mut backlight in Backlight::iter() {
        let backlight = backlight?;
        let max_brightness = backlight.max_brightness()?;
        let current = backlight.brightness()?;
        let new = max_brightness * 40 / 100;
        if new < current {
            backlight.set_brightness(new)?;
        }
    }

    for mut backlight in Leds::keyboard_backlights() {
        let backlight = backlight?;
        let max_brightness = backlight.max_brightness()?;
        let current = backlight.brightness()?;
        let new = max_brightness/2;
        if new < current {
            backlight.set_brightness(new)?;
        }
    }

    Ok(())
}

fn battery() -> io::Result<()> {
    if experimental_is_enabled() {
        let disks = Disks::new();
        disks.set_apm_level(128)?;
        disks.set_autosuspend_delay(15000)?;

        for host in ScsiHost::iter() {
            host?.set_link_power_management_policy(&["min_power", "min_power"])?;
        }

        SoundDevice::get_devices().for_each(|dev| dev.set_power_save(1, true));
        RadeonDevice::get_devices().for_each(|dev| dev.set_profiles("low", "battery", "low"));
        for device in PciDevice::all()? {
            device.set_runtime_pm(RuntimePowerManagement::On)?;
        }

        Dirty::new().set_max_lost_work(15);
        LaptopMode::new().set(b"2");
    }

    if let Ok(pstate) = PState::new() {
        pstate.set_min_perf_pct(0)?;
        pstate.set_max_perf_pct(50)?;
        pstate.set_no_turbo(true)?;
    }

    for mut backlight in Backlight::all()? {
        let max_brightness = backlight.max_brightness()?;
        let current = backlight.brightness()?;
        let new = max_brightness * 10 / 100;
        if new < current {
            backlight.set_brightness(new)?;
        }
    }

    for mut backlight in Leds::keyboard_backlights() {
        let backlight = backlight?;
        backlight.set_brightness(0)?;
    }

    Ok(())
}

struct PowerDaemon {
    graphics: Graphics
}

impl PowerDaemon {
    fn new() -> Result<PowerDaemon, String> {
        let graphics = Graphics::new().map_err(err_str)?;
        Ok(PowerDaemon { graphics })
    }
}

impl Power for PowerDaemon {
    fn performance(&mut self) -> Result<(), String> {
        performance().map_err(err_str)
    }

    fn balanced(&mut self) -> Result<(), String> {
        balanced().map_err(err_str)
    }

    fn battery(&mut self) -> Result<(), String> {
        battery().map_err(err_str)
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
    info!("Starting daemon{}", if experimental { " with experimental enabled" } else { "" });
    EXPERIMENTAL.store(experimental, Ordering::SeqCst);
    let daemon = Rc::new(RefCell::new(PowerDaemon::new()?));

    info!("Disabling NMI Watchdog (for kernel debugging only)");
    NmiWatchdog::new().set(b"0");

    info!("Setting automatic graphics power");
    match daemon.borrow_mut().auto_graphics_power() {
        Ok(()) => (),
        Err(err) => {
            error!("Failed to set automatic graphics power: {}", err);
        }
    }

    info!("Initializing with the balanced profile");
    balanced().map_err(|why| format!("failed to set initial profile: {}", why))?;

    info!("Connecting to dbus system bus");
    let c = Connection::get_private(BusType::System).map_err(err_str)?;

    info!("Registering dbus name {}", DBUS_NAME);
    c.register_name(DBUS_NAME, NameFlag::ReplaceExisting as u32).map_err(err_str)?;

    info!(
        "Adding dbus path {} with interface {}",
        DBUS_PATH, DBUS_IFACE
    );

    let f = Factory::new_fn::<()>();

    let signal_hotplug = Arc::new(f.signal("HotPlugDetect", ()).sarg::<u64, _>("port"));

    let tree = dbus_impl!(daemon, f {
        // Power Profiles
        fn battery<Battery, no_out, no_in>();
        fn balanced<Balanced, no_out, no_in>();
        fn performance<Performance, no_out, no_in>();

        // Graphics
        fn get_graphics<GetGraphics, has_out, no_in>() -> vendor: &str;
        fn set_graphics<SetGraphics, no_out, has_in>(vendor: &str);
        fn get_graphics_power<GetGraphicsPower, has_out, no_in>() -> power: bool;
        fn set_graphics_power<SetGraphicsPower, no_out, has_in>(power: bool);
        fn auto_graphics_power<AutoGraphicsPower, no_out, no_in>();

        // Switchable graphics detection
        fn get_switchable<GetSwitchable, has_out, no_in>() -> switchable: bool;

        signal signal_hotplug;
    });

    tree.set_registered(&c, true).map_err(err_str)?;

    c.add_handler(tree);

    let fan_daemon_res = FanDaemon::new();

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
                    signal_hotplug.msg(&DBUS_PATH.into(), &DBUS_NAME.into()).append1(i as u64)
                ).map_err(|()| "failed to send message".to_string())?;
            }
        }

        last = hpd;
    }
}
