use std::fmt::Display;
use std::fs::File;
use std::io::{self, Read};
use std::path::Path;
use std::str::FromStr;

use backlight::Backlight;

mod backlight;

pub fn parse_file<F: FromStr, P: AsRef<Path>>(path: P) -> io::Result<F>
    where F::Err: Display
{
    let mut data = String::new();

    {
        let mut file = File::open(path.as_ref())?;
        file.read_to_string(&mut data)?;
    }

    data.trim().parse().map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{}", e)
        )
    })
}

fn power() -> io::Result<()> {
    let backlight = Backlight::new("intel_backlight")?;
    let brightness = backlight.actual_brightness()?;
    let max_brightness = backlight.max_brightness()?;
    let ratio = (brightness as f64)/(max_brightness as f64);
    let power = 0.7 + 3.0 * ratio;
    let percent = (ratio * 100.0) as u64;
    println!("{}/{} = {}% ~{:.2} W", brightness, max_brightness, percent, power);

    Ok(())
}

fn main() {
    power().unwrap();
}
