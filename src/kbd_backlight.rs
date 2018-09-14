use std::{fs, io};
use std::path::{Path, PathBuf};

use backlight::BacklightExt;

pub struct KeyboardBacklight {
    name: String,
    path: PathBuf,
}

impl BacklightExt for KeyboardBacklight {
    fn path(&self) -> &Path { &self.path }
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

        path.read_dir()?;

        Ok(KeyboardBacklight {
            name: name.to_string(),
            path: path
        })
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}
