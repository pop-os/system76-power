extern crate log;
extern crate system76_power;

use log::LevelFilter;
use std::{io, process, thread, time};
use system76_power::fan::FanDaemon;
use system76_power::logging;

fn inner() -> io::Result<()> {
    let daemon = FanDaemon::new()?;

    loop {
        if let Some(temp) = daemon.get_temp() {
            if let Some(duty) = daemon.get_duty(temp) {
                println!(
                    "{}°C ({}): {}% ({})",
                    (temp as f32) / 1000.0,
                    temp,
                    (duty as u32 * 100) / 255,
                    duty,
                );
            } else {
                println!(
                    "{}°C ({}): Fan curve does not specify duty",
                    (temp as f32) / 1000.0,
                    temp,
                );
            }
        } else {
            println!("Failed to read temperature");
        }
        thread::sleep(time::Duration::new(1, 0));
    }
}

fn main() {
    if let Err(why) = logging::setup_logging(LevelFilter::Debug) {
        eprintln!("failed to set up logging: {}", why);
        process::exit(1);
    }

    if let Err(err) = inner() {
        eprintln!("{:?}", err);
        process::exit(1);
    }
}
