use std::{fs, io, u8, u16, u32};
use std::path::PathBuf;

use util::{parse_file, read_file, write_file};

pub struct PciDevice {
    name: String,
    path: PathBuf,
}

impl PciDevice {
    pub fn all() -> io::Result<Vec<PciDevice>> {
        let mut ret = Vec::new();

        for entry_res in fs::read_dir("/sys/bus/pci/devices")? {
            let entry = entry_res?;
            if let Ok(name) = entry.file_name().into_string() {
                ret.push(PciDevice::new(&name)?);
            }
        }

        Ok(ret)
    }

    pub fn new(name: &str) -> io::Result<PciDevice> {
        let mut path = PathBuf::from("/sys/bus/pci/devices");
        path.push(name);

        fs::read_dir(&path)?;

        Ok(PciDevice {
            name: name.to_string(),
            path: path
        })
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn boot_vga(&self) -> io::Result<bool> {
        let v: u8 = parse_file(self.path.join("boot_vga"))?;
        Ok(v > 0)
    }

    pub fn class(&self) -> io::Result<u32> {
        let v = read_file(self.path.join("class"))?;
        u32::from_str_radix(v[2..].trim(), 16).map_err(|err| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("{}", err)
            )
        })
    }

    pub fn device(&self) -> io::Result<u16> {
        let v = read_file(self.path.join("device"))?;
        u16::from_str_radix(v[2..].trim(), 16).map_err(|err| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("{}", err)
            )
        })
    }

    pub fn revision(&self) -> io::Result<u8> {
        let v = read_file(self.path.join("device"))?;
        u8::from_str_radix(v[2..].trim(), 16).map_err(|err| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("{}", err)
            )
        })
    }

    pub fn vendor(&self) -> io::Result<u16> {
        let v = read_file(self.path.join("vendor"))?;
        u16::from_str_radix(v[2..].trim(), 16).map_err(|err| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("{}", err)
            )
        })
    }

    pub fn remove(self) -> io::Result<()> {
        write_file(self.path.join("remove"), format!("1"))
    }
}
