use std::{fs, io, u8, u16, u32};
use std::path::{Path, PathBuf};

use util::{read_file, write_file};

pub struct PciBus {
    path: PathBuf
}

impl PciBus {
    pub fn new() -> io::Result<PciBus> {
        let path = PathBuf::from("/sys/bus/pci");
        if path.is_dir() {
            Ok(PciBus { path })
        } else {
            Err(io::Error::new(io::ErrorKind::NotFound, "pci directory not found"))
        }
    }

    pub fn devices(&self) -> io::Result<Vec<PciDevice>> {
        let mut ret = Vec::new();

        for entry_res in fs::read_dir(self.path.join("devices"))? {
            let entry = entry_res?;
            ret.push(PciDevice {
                path: entry.path()
            });
        }

        Ok(ret)
    }

    pub fn drivers(&self) -> io::Result<Vec<PciDriver>> {
        let mut ret = Vec::new();

        for entry_res in fs::read_dir(self.path.join("drivers"))? {
            let entry = entry_res?;
            ret.push(PciDriver {
                path: entry.path()
            });
        }

        Ok(ret)
    }

    pub fn rescan(&self) -> io::Result<()> {
        write_file(self.path.join("rescan"), format!("1"))
    }
}

pub struct PciDriver {
    path: PathBuf,
}

impl PciDriver {
    pub fn name(&self) -> &str {
        self.path
            .file_name().expect("invalid path")
            .to_str().expect("invalid UTF-8")
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

pub struct PciDevice {
    path: PathBuf,
}

impl PciDevice {
    pub fn name(&self) -> &str {
        self.path
            .file_name().expect("invalid path")
            .to_str().expect("invalid UTF-8")
    }

    pub fn path(&self) -> &Path {
        &self.path
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

    pub fn driver(&self) -> io::Result<PciDriver> {
        let path = fs::canonicalize(self.path.join("driver"))?;
        Ok(PciDriver {
            path: path,
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

    pub unsafe fn remove(self) -> io::Result<()> {
        write_file(self.path.join("remove"), format!("1"))
    }
}
