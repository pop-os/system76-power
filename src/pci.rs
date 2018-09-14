use std::{fs, io, u8, u16, u32};
use std::path::{Path, PathBuf};
use kernel_parameters::RuntimePowerManagement;

use util::{entries, read_file, write_file};

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
        entries(&self.path.join("devices"), |entry| PciDevice { path: entry.path() })
    }

    pub fn drivers(&self) -> io::Result<Vec<PciDriver>> {
        entries(&self.path.join("drivers"), |entry| PciDriver { path: entry.path() })
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

    pub unsafe fn bind(&self, device: &PciDevice) -> io::Result<()> {
        write_file(self.path.join("bind"), device.name().to_string())
    }

    pub unsafe fn unbind(&self, device: &PciDevice) -> io::Result<()> {
        write_file(self.path.join("unbind"), device.name().to_string())
    }
}

pub struct PciDevice {
    path: PathBuf,
}

macro_rules! pci_device {
    ($file:tt as $out:tt) => {
        pub fn $file(&self) -> io::Result<$out> {
            let v = read_file(self.path.join(stringify!($file)))?;
            $out::from_str_radix(v[2..].trim(), 16).map_err(|err| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("{}", err)
                )
            })
        }
    }
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

    pub fn set_runtime_pm(&self, state: RuntimePowerManagement) -> io::Result<()> {
        write_file(&self.path.join("power/control"), <&'static str>::from(state))
    }

    pci_device!(class as u32);

    pci_device!(device as u16);

    pci_device!(revision as u8);

    pci_device!(vendor as u16);

    pub fn driver(&self) -> io::Result<PciDriver> {
        let path = fs::canonicalize(self.path.join("driver"))?;
        Ok(PciDriver {
            path: path,
        })
    }

    pub unsafe fn remove(&self) -> io::Result<()> {
        write_file(self.path.join("remove"), format!("1"))
    }
}
