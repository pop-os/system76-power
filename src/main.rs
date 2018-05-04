extern crate dbus;
extern crate libc;

use dbus::{Connection, BusType, NameFlag};
use dbus::tree::{Factory, MethodErr};
use std::{env, fs, io, process};
use std::io::Write;
use std::sync::{Arc, Mutex};

use backlight::Backlight;
use kbd_backlight::KeyboardBacklight;
use module::Module;
use pstate::PState;

pub mod backlight;
pub mod kbd_backlight;
mod module;
pub mod pstate;
mod util;
mod state;

use state::{Profile, State};

// Helper function for errors
pub (crate) fn err_str<E: ::std::fmt::Display>(err: E) -> String {
    format!("{}", err)
}

fn performance(state: Option<&Mutex<State>>) -> io::Result<()> {
    {
        let mut pstate = PState::new()?;

        pstate.set_min_perf_pct(50)?;
        pstate.set_max_perf_pct(100)?;
        pstate.set_no_turbo(false)?;
    }

    let mut profile_unset = true;

    if let Some(state) = state {
        let mut state = state.lock().unwrap();

        state.get_active_backlight_mut().store()?;
        if state.performance_backlight.is_set() {
            state.performance_backlight.restore()?;
            profile_unset = false;
        }

        state.profile = Profile::HighPerformance;
    }

    if profile_unset {
        for mut backlight in Backlight::all()? {
            let new = backlight.max_brightness()?;
            backlight.set_brightness(new)?;
        }

        for mut backlight in KeyboardBacklight::all()? {
            let new = backlight.max_brightness()?;
            backlight.set_brightness(new)?;
        }
    }

    Ok(())
}

fn balanced(state: Option<&Mutex<State>>) -> io::Result<()> {
    {
        let mut pstate = PState::new()?;
        pstate.set_min_perf_pct(0)?;
        pstate.set_max_perf_pct(100)?;
        pstate.set_no_turbo(false)?;
    }

    let mut profile_unset = true;

    if let Some(state) = state {
        let mut state = state.lock().unwrap();

        state.get_active_backlight_mut().store()?;
        if state.balanced_backlight.is_set() {
            state.balanced_backlight.restore()?;
            profile_unset = false;
        }

        state.profile = Profile::Balanced;
    }

    if profile_unset {
        for mut backlight in Backlight::all()? {
            let max_brightness = backlight.max_brightness()?;
            let backlight_prev = backlight.brightness()?;
            let new = max_brightness * 40 / 100;
            if new < backlight_prev {
                backlight.set_brightness(new)?;
            }
        }

        for mut backlight in KeyboardBacklight::all()? {
            let max_brightness = backlight.max_brightness()?;
            let kbd_backlight_prev = backlight.brightness()?;
            let new = max_brightness/2;
            if new < kbd_backlight_prev {
                backlight.set_brightness(new)?;
            }
        }
    }

    Ok(())
}

fn battery(state: Option<&Mutex<State>>) -> io::Result<()> {
    {
        let mut pstate = PState::new()?;
        pstate.set_min_perf_pct(0)?;
        pstate.set_max_perf_pct(50)?;
        pstate.set_no_turbo(true)?;
    }

    let mut profile_unset = true;

    if let Some(state) = state {
        let mut state = state.lock().unwrap();

        state.get_active_backlight_mut().store()?;
        if state.battery_backlight.is_set() {
            state.battery_backlight.restore()?;
            profile_unset = false;
        }

        state.profile = Profile::Battery;
    }

    if profile_unset {
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
    }

    Ok(())
}

fn power() -> io::Result<()> {
    {
        let pstate = PState::new()?;
        let min = pstate.min_perf_pct()?;
        let max = pstate.max_perf_pct()?;
        let no_turbo = pstate.no_turbo()?;
        println!("CPU: {}% - {}%, {}", min, max, if no_turbo { "No Turbo" } else { "Turbo" });
    }

    for backlight in Backlight::all()? {
        let brightness = backlight.actual_brightness()?;
        let max_brightness = backlight.max_brightness()?;
        let ratio = (brightness as f64)/(max_brightness as f64);
        let percent = (ratio * 100.0) as u64;
        println!("Backlight {}: {}/{} = {}%", backlight.name(), brightness, max_brightness, percent);
    }

    for backlight in KeyboardBacklight::all()? {
        let brightness = backlight.brightness()?;
        let max_brightness = backlight.max_brightness()?;
        let ratio = (brightness as f64)/(max_brightness as f64);
        let percent = (ratio * 100.0) as u64;
        println!("Keyboard Backlight {}: {}/{} = {}%", backlight.name(), brightness, max_brightness, percent);
    }

    Ok(())
}

fn get_graphics() -> io::Result<&'static str> {
    let modules = Module::all()?;

    if modules.iter().find(|module| module.name == "bbswitch").is_none() {
        let status = process::Command::new("modprobe").arg("bbswitch").status()?;
        if ! status.success() {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                format!("modprobe bbswitch: failed with {}", status)
            ));
        }
    }

    if modules.iter().find(|module| module.name == "nouveau" || module.name == "nvidia").is_some() {
        Ok("nvidia")
    } else {
        Ok("intel")
    }
}

static MODPROBE_NVIDIA: &'static [u8] = br#"# Automatically generated by system76-power
# Disabled until a fix is available: options bbswitch load_state=1
"#;

static MODPROBE_INTEL: &'static [u8] = br#"# Automatically generated by system76-power
# Disabled until a fix is available: options bbswitch load_state=0
blacklist nouveau
blacklist nvidia
blacklist nvidia-drm
blacklist nvidia-modeset
alias nouveau off
alias nvidia off
alias nvidia-drm off
alias nvidia-modeset off
"#;

static MODULES_LOAD: &'static [u8] = br#"# Automatically generated by system76-power
bbswitch
"#;

fn set_graphics(vendor: &str) -> io::Result<()> {
    let modules = Module::all()?;

    if modules.iter().find(|module| module.name == "bbswitch").is_none() {
        let status = process::Command::new("modprobe").arg("bbswitch").status()?;
        if ! status.success() {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                format!("modprobe bbswitch: failed with {}", status)
            ));
        }
    }

    {
        let path = "/etc/modprobe.d/system76-power.conf";
        let mut file = fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(path)?;

        if vendor == "nvidia" {
            file.write_all(MODPROBE_NVIDIA)?;
        } else {
            file.write_all(MODPROBE_INTEL)?;
        }

        file.sync_all()?;
    }

    {
        let path = "/etc/modules-load.d/system76-power.conf";
        let mut file = fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(path)?;

        file.write_all(MODULES_LOAD)?;

        file.sync_all()?;
    }

    if vendor == "nvidia" {
        let status = process::Command::new("systemctl").arg("enable").arg("nvidia-fallback.service").status()?;
        if ! status.success() {
            // Error is ignored in case this service is removed
            eprintln!("systemctl: failed with {}", status);
        }
    } else {
        let status = process::Command::new("systemctl").arg("disable").arg("nvidia-fallback.service").status()?;
        if ! status.success() {
            // Error is ignored in case this service is removed
            eprintln!("systemctl: failed with {}", status);
        }
    }

    let status = process::Command::new("update-initramfs").arg("-u").status()?;
    if ! status.success() {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            format!("update-initramfs: failed with {}", status)
        ));
    }

    Ok(())
}

fn daemon() -> Result<(), String> {
    if unsafe { libc::geteuid() } != 0 {
        return Err(format!("must be run as root"));
    }

    let state = Arc::new(Mutex::new(State::default()));
    balanced(None).map_err(err_str)?;

    let c = Connection::get_private(BusType::System).map_err(err_str)?;
    c.register_name("com.system76.PowerDaemon", NameFlag::ReplaceExisting as u32).map_err(err_str)?;

    let f = Factory::new_fn::<()>();

    let tree = f.tree(()).add(f.object_path("/com/system76/PowerDaemon", ()).introspectable().add(
        f.interface("com.system76.PowerDaemon", ())
        .add_m({
            let state = state.clone();
            f.method("Performance", (), move |m| {
                eprintln!("Performance");
                match performance(Some(&state)) {
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
            let state = state.clone();
            f.method("Balanced", (), move |m| {
                eprintln!("Balanced");
                match balanced(Some(&state)) {
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
        .add_m(
            f.method("Battery", (), move |m| {
                eprintln!("Battery");
                match battery(Some(&state)) {
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
        )
        .add_m(
            f.method("GetGraphics", (), move |m| {
                eprintln!("GetGraphics");
                match get_graphics() {
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
        )
        .add_m(
            f.method("SetGraphics", (), move |m| {
                let vendor = m.msg.read1()?;
                eprintln!("SetGraphics({})", vendor);
                match set_graphics(vendor) {
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
        )
    ));

    tree.set_registered(&c, true).map_err(err_str)?;

    c.add_handler(tree);

    loop {
        c.incoming(1000).next();
    }
}

fn usage() {
    eprintln!("system76-power [sub-command] [args...]");
    eprintln!("  daemon - run in daemon mode");
    eprintln!("  performance - set profile to performance");
    eprintln!("  balanced - set profile to balanced");
    eprintln!("  battery - set profile to battery");
    eprintln!("  graphics - query graphics mode");
    eprintln!("  graphics intel - set graphics mode to intel");
    eprintln!("  graphics nvidia - set graphics mode to nvidia");
}

fn main() {
    let mut args = env::args().skip(1);

    if let Some(arg) = args.next() {
        match arg.as_str() {
            "daemon" => {
                println!("starting daemon");
                daemon().unwrap();
            },
            "performance" => {
                println!("setting performance mode");
                performance(None).unwrap();
            },
            "balanced" => {
                println!("setting balanced mode");
                balanced(None).unwrap();
            },
            "battery" => {
                println!("setting battery mode");
                battery(None).unwrap();
            },
            "graphics" => if let Some(arg) = args.next() {
                match arg.as_str() {
                    "intel" => {
                        println!("setting intel graphics");
                        set_graphics("intel").unwrap();
                    },
                    "nvidia" => {
                        println!("setting nvidia graphics");
                        set_graphics("nvidia").unwrap();
                    },
                    _ => {
                        eprintln!("system76-power: unknown graphics vendor {}", arg);
                        usage();
                        process::exit(1);
                    }
                }
            } else {
                let graphics = get_graphics().unwrap();
                println!("{}", graphics);
            },
            _ => {
                eprintln!("system76-power: unknown sub-command {}", arg);
                usage();
                process::exit(1);
            }
        }
    } else {
        power().unwrap();
    }
}
