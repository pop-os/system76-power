use dbus::{Connection, BusType, NameFlag};
use dbus::tree::{Factory, MethodErr};
use std::cell::RefCell;
use std::io;
use std::rc::Rc;
use std::sync::Arc;

use {DBUS_NAME, DBUS_PATH, DBUS_IFACE, Power, err_str};
use backlight::Backlight;
use disks::{Disks, DiskPower};
use graphics::Graphics;
use hotplug::HotPlugDetect;
use kbd_backlight::KeyboardBacklight;
use pstate::PState;

fn performance() -> io::Result<()> {
    let disks = Disks::new();
    disks.set_apm_level(254)?;
    disks.set_autosuspend_delay(-1)?;

    {
        let mut pstate = PState::new()?;
        pstate.set_min_perf_pct(50)?;
        pstate.set_max_perf_pct(100)?;
        pstate.set_no_turbo(false)?;
    }

    Ok(())
}

fn balanced() -> io::Result<()> {
    let disks = Disks::new();
    disks.set_apm_level(254)?;
    disks.set_autosuspend_delay(-1)?;

    {
        let mut pstate = PState::new()?;
        pstate.set_min_perf_pct(0)?;
        pstate.set_max_perf_pct(100)?;
        pstate.set_no_turbo(false)?;
    }

    for mut backlight in Backlight::all()? {
        let max_brightness = backlight.max_brightness()?;
        let current = backlight.brightness()?;
        let new = max_brightness * 40 / 100;
        if new < current {
            backlight.set_brightness(new)?;
        }
    }

    for mut backlight in KeyboardBacklight::all()? {
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
    let disks = Disks::new();
    disks.set_apm_level(128)?;
    disks.set_autosuspend_delay(15000)?;

    {
        let mut pstate = PState::new()?;
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

    for mut backlight in KeyboardBacklight::all()? {
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

pub fn daemon() -> Result<(), String> {
    eprintln!("Starting daemon");
    let daemon = Rc::new(RefCell::new(PowerDaemon::new()?));

    eprintln!("Setting automatic graphics power");
    match daemon.borrow_mut().auto_graphics_power() {
        Ok(()) => (),
        Err(err) => {
            eprintln!("Failed to set automatic graphics power: {}", err);
        }
    }

    eprintln!("Connecting to dbus system bus");
    let c = Connection::get_private(BusType::System).map_err(err_str)?;

    eprintln!("Registering dbus name {}", DBUS_NAME);
    c.register_name(DBUS_NAME, NameFlag::ReplaceExisting as u32).map_err(err_str)?;

    let f = Factory::new_fn::<()>();

    let signal = Arc::new(f.signal("HotPlugDetect", ()).sarg::<u64,_>("port"));

    eprintln!("Adding dbus path {} with interface {}", DBUS_PATH, DBUS_IFACE);
    let tree = f.tree(()).add(f.object_path(DBUS_PATH, ()).introspectable().add(
        f.interface(DBUS_IFACE, ())
        .add_m({
            let daemon = daemon.clone();
            f.method("Performance", (), move |m| {
                eprintln!("Performance");
                match daemon.borrow_mut().performance() {
                    Ok(()) => {
                        let mret = m.msg.method_return();
                        Ok(vec![mret])
                    },
                    Err(err) => {
                        eprintln!("{}", err);
                        Err(MethodErr::failed(&err))
                    }
                }
            })
        })
        .add_m({
            let daemon = daemon.clone();
            f.method("Balanced", (), move |m| {
                eprintln!("Balanced");
                match daemon.borrow_mut().balanced() {
                    Ok(()) => {
                        let mret = m.msg.method_return();
                        Ok(vec![mret])
                    },
                    Err(err) => {
                        eprintln!("{}", err);
                        Err(MethodErr::failed(&err))
                    }
                }
            })
        })
        .add_m({
            let daemon = daemon.clone();
            f.method("Battery", (), move |m| {
                eprintln!("Battery");
                match daemon.borrow_mut().battery() {
                    Ok(()) => {
                        let mret = m.msg.method_return();
                        Ok(vec![mret])
                    },
                    Err(err) => {
                        eprintln!("{}", err);
                        Err(MethodErr::failed(&err))
                    }
                }
            })
        })
        .add_m({
            let daemon = daemon.clone();
            f.method("GetGraphics", (), move |m| {
                eprintln!("GetGraphics");
                match daemon.borrow_mut().get_graphics() {
                    Ok(vendor) => {
                        let mret = m.msg.method_return().append1(vendor);
                        Ok(vec![mret])
                    },
                    Err(err) => {
                        eprintln!("{}", err);
                        Err(MethodErr::failed(&err))
                    }
                }
            })
            .outarg::<&str,_>("vendor")
        })
        .add_m({
            let daemon = daemon.clone();
            f.method("SetGraphics", (), move |m| {
                let vendor = m.msg.read1()?;
                eprintln!("SetGraphics({})", vendor);
                match daemon.borrow_mut().set_graphics(vendor) {
                    Ok(()) => {
                        let mret = m.msg.method_return();
                        Ok(vec![mret])
                    },
                    Err(err) => {
                        eprintln!("{}", err);
                        Err(MethodErr::failed(&err))
                    }
                }
            })
            .inarg::<&str,_>("vendor")
        })
        .add_m({
            let daemon = daemon.clone();
            f.method("GetGraphicsPower", (), move |m| {
                eprintln!("GetGraphicsPower");
                match daemon.borrow_mut().get_graphics_power() {
                    Ok(power) => {
                        let mret = m.msg.method_return().append1(power);
                        Ok(vec![mret])
                    },
                    Err(err) => {
                        eprintln!("{}", err);
                        Err(MethodErr::failed(&err))
                    }
                }
            })
            .outarg::<bool,_>("power")
        })
        .add_m({
            let daemon = daemon.clone();
            f.method("SetGraphicsPower", (), move |m| {
                let power = m.msg.read1()?;
                eprintln!("SetGraphicsPower({})", power);
                match daemon.borrow_mut().set_graphics_power(power) {
                    Ok(()) => {
                        let mret = m.msg.method_return();
                        Ok(vec![mret])
                    },
                    Err(err) => {
                        eprintln!("{}", err);
                        Err(MethodErr::failed(&err))
                    }
                }
            })
            .inarg::<bool,_>("power")
        })
        .add_m({
            let daemon = daemon.clone();
            f.method("AutoGraphicsPower", (), move |m| {
                eprintln!("AutoGraphicsPower");
                match daemon.borrow_mut().auto_graphics_power() {
                    Ok(()) => {
                        let mret = m.msg.method_return();
                        Ok(vec![mret])
                    },
                    Err(err) => {
                        eprintln!("{}", err);
                        Err(MethodErr::failed(&err))
                    }
                }
            })
        })
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

    eprintln!("Handling dbus requests");
    loop {
        c.incoming(1000).next();

        let hpd = hpd();
        for i in 0..hpd.len() {
            if hpd[i] != last[i] {
                if hpd[i] {
                    eprintln!("HotPlugDetect {}", i);
                    c.send(
                        signal.msg(&DBUS_PATH.into(), &DBUS_NAME.into()).append1(i as u64)
                    ).map_err(|()| format!("failed to send message"))?;
                }
            }
        }

        last = hpd;
    }
}
