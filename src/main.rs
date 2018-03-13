use std::{env, io, process};

use backlight::Backlight;
use kbd_backlight::KeyboardBacklight;
use pstate::PState;

pub mod backlight;
pub mod kbd_backlight;
pub mod pstate;
mod util;

fn performance() -> io::Result<()> {
    {
        let mut pstate = PState::new()?;
        pstate.set_min_perf_pct(50)?;
        pstate.set_max_perf_pct(100)?;
        pstate.set_no_turbo(false)?;
    }

    Ok(())
}

fn balanced() -> io::Result<()> {
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

fn main() {
    let mut args = env::args().skip(1);

    if let Some(arg) = args.next() {
        match arg.as_str() {
            "performance" => {
                println!("setting performance mode");
                performance().unwrap();
            },
            "balanced" => {
                println!("setting balanced mode");
                balanced().unwrap();
            },
            "battery" => {
                println!("setting battery mode");
                battery().unwrap();
            },
            _ => {
                eprintln!("system76-power: unknown sub-command {}", arg);
                process::exit(1);
            }
        }
    } else {
        power().unwrap();
    }
}
