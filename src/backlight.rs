use std::fs::File;
use std::io::{self, Read};
use std::path::PathBuf;

use super::parse_file;

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

    pub fn max_brightness(&self) -> io::Result<u64> {
        parse_file(self.path.join("max_brightness"))
    }
}
