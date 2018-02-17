use std::fs::File;
use std::io::{self, Read};
use std::path::PathBuf;

pub struct Backlight {
    path: PathBuf,
}

impl Backlight {
    pub fn new(name: &str) -> io::Result<Backlight> {
        //TODO: Check for validity
        let mut path = PathBuf::from("/sys/class/backlight");
        path.push(name);
        Ok(Backlight {
            path: path
        })
    }

    fn read_u64(&self, name: &str) -> io::Result<u64> {
        let mut data = String::new();

        {
            let mut path = self.path.clone();
            path.push(name);
            let mut file = File::open(path)?;
            file.read_to_string(&mut data)?;
        }

        data.trim().parse().map_err(|e| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("{}", e)
            )
        })
    }

    pub fn actual_brightness(&self) -> io::Result<u64> {
        self.read_u64("actual_brightness")
    }

    pub fn brightness(&self) -> io::Result<u64> {
        self.read_u64("brightness")
    }

    pub fn max_brightness(&self) -> io::Result<u64> {
        self.read_u64("max_brightness")
    }
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
