use std::{fs::write, io, path::PathBuf};

pub struct PciBus {
    path: PathBuf,
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

    pub fn rescan(&self) -> io::Result<()> { write(self.path.join("rescan"), "1") }
}
