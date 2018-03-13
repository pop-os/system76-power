use std::{fs, io};
use std::path::PathBuf;

use util::{parse_file, write_file};

pub struct KeyboardBacklight {
    name: String,
    path: PathBuf,
}

impl KeyboardBacklight {
    pub fn all() -> io::Result<Vec<KeyboardBacklight>> {
        let mut ret = Vec::new();

        for entry_res in fs::read_dir("/sys/class/leds")? {
            let entry = entry_res?;
            if let Ok(name) = entry.file_name().into_string() {
                if name.contains("kbd_backlight") {
                    ret.push(KeyboardBacklight::new(&name)?);
                }
            }
        }

        Ok(ret)
    }

    pub fn new(name: &str) -> io::Result<KeyboardBacklight> {
        let mut path = PathBuf::from("/sys/class/leds");
        path.push(name);

        fs::read_dir(&path)?;

        Ok(KeyboardBacklight {
            name: name.to_string(),
            path: path
        })
    }

    pub fn name(&self) -> &str {
        &self.name
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
}
