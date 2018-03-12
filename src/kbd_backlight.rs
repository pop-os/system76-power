use std::io;
use std::path::PathBuf;

use util::{parse_file, parse_file_radix, write_file};

pub struct KeyboardBacklight {
    path: PathBuf,
}

impl KeyboardBacklight {
    pub fn new() -> io::Result<KeyboardBacklight> {
        //TODO: Check for validity
        Ok(KeyboardBacklight {
            path: PathBuf::from(
                "/sys/devices/platform/system76/leds/system76::kbd_backlight"
            )
        })
    }

    pub fn brightness(&self) -> io::Result<u64> {
        parse_file(self.path.join("brightness"))
    }
    
    pub fn set_brightness(&mut self, value: u64) -> io::Result<()> {
        write_file(self.path.join("brightness"), format!("{}", value))
    }

    pub fn max_brightness(&self) -> io::Result<u64> {
        parse_file(self.path.join("max_brightness"))
    }
    
    pub fn color_left(&self) -> io::Result<u64> {
        parse_file_radix(self.path.join("color_left"), 16)
    }
    
    pub fn set_color_left(&mut self, color: u64) -> io::Result<()> {
        write_file(self.path.join("color_left"), format!("{:06X}", color))
    }
    
    pub fn color_center(&self) -> io::Result<u64> {
        parse_file_radix(self.path.join("color_center"), 16)
    }
    
    pub fn set_color_center(&mut self, color: u64) -> io::Result<()> {
        write_file(self.path.join("color_center"), format!("{:06X}", color))
    }
    
    pub fn color_right(&self) -> io::Result<u64> {
        parse_file_radix(self.path.join("color_right"), 16)
    }
    
    pub fn set_color_right(&mut self, color: u64) -> io::Result<()> {
        write_file(self.path.join("color_right"), format!("{:06X}", color))
    }
    
    pub fn color_extra(&self) -> io::Result<u64> {
        parse_file_radix(self.path.join("color_extra"), 16)
    }
    
    pub fn set_color_extra(&mut self, color: u64) -> io::Result<()> {
        write_file(self.path.join("color_extra"), format!("{:06X}", color))
    }
}
