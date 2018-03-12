use std::io;
use std::path::PathBuf;

use util::{parse_file, write_file};

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

    pub fn bl_power(&self) -> io::Result<u64> {
        parse_file(self.path.join("actual_brightness"))
    }

    pub fn actual_brightness(&self) -> io::Result<u64> {
        parse_file(self.path.join("actual_brightness"))
    }

    pub fn brightness(&self) -> io::Result<u64> {
        parse_file(self.path.join("brightness"))
    }
    
    pub fn set_brightness(&self, value: u64) -> io::Result<()> {
        write_file(self.path.join("brightness"), format!("{}", value))
    }

    pub fn max_brightness(&self) -> io::Result<u64> {
        parse_file(self.path.join("max_brightness"))
    }
}
