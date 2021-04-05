use log::LevelFilter;
use std::{process, thread, time};
use system76_power::{
    fan::{FanDaemon, FanDaemonError},
    logging,
};

fn inner() -> Result<(), FanDaemonError> {
    let daemon = FanDaemon::new(false);

    loop {
        if let Some(temp) = daemon.get_temp() {
            if let Some(duty) = daemon.get_duty(temp) {
                println!(
                    "{}°C ({}): {}% ({})",
                    (temp as f32) / 1000.0,
                    temp,
                    (u32::from(duty) * 100) / 255,
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
    if let Err(why) = logging::setup(LevelFilter::Debug) {
        eprintln!("failed to set up logging: {}", why);
        process::exit(1);
    }

    if let Err(err) = inner() {
        eprintln!("{:?}", err);
        process::exit(1);
    }
}
