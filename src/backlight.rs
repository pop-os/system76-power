use std::{fs, io};
use std::path::PathBuf;

use util::{parse_file, write_file};

pub struct Backlight {
    name: String,
    path: PathBuf,
}

impl Backlight {
    pub fn all() -> io::Result<Vec<Backlight>> {
        let mut ret = Vec::new();

        for entry_res in fs::read_dir("/sys/class/backlight")? {
            let entry = entry_res?;
            if let Ok(name) = entry.file_name().into_string() {
                ret.push(Backlight::new(&name)?);
            }
        }

        Ok(ret)
    }

    pub fn new(name: &str) -> io::Result<Backlight> {
        let mut path = PathBuf::from("/sys/class/backlight");
        path.push(name);

        fs::read_dir(&path)?;

        Ok(Backlight {
            name: name.to_string(),
            path: path
        })
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn bl_power(&self) -> io::Result<u64> {
        parse_file(self.path.join("bl_power"))
    }

    pub fn actual_brightness(&self) -> io::Result<u64> {
        parse_file(self.path.join("actual_brightness"))
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
