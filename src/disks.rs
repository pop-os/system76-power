use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use util::read_file;

pub trait DiskPower {
    fn set_apm_level(&self, level: u8) -> io::Result<()>;
}

pub struct Disks(Vec<Disk>);

impl Disks {
    pub fn new() -> Disks {
        let mut disks = Vec::new();
        let blocks = match Path::new("/sys/block").read_dir() {
            Ok(blocks) => blocks,
            Err(why) => {
                eprintln!("disks: unable to get block devices: {}", why);
                return Disks(disks);
            }
        };

        for device in blocks.flat_map(|dev| dev.ok()) {
            if device.path().join("slaves").exists() {
                if let Ok(name) = device.file_name().into_string() {
                    if name.starts_with("loop") || name.starts_with("dm") {
                        continue
                    }

                    let path = PathBuf::from(["/dev/", &name].concat());
                    disks.push(Disk {
                        path,
                        is_rotational: {
                            read_file(device.path().join("queue/rotational")).ok()
                                .map_or(false, |string| string.trim() == "1")
                        }
                    });
                }
            }
        }

        Disks(disks)
    }
}

impl DiskPower for Disks {
    fn set_apm_level(&self, level: u8) -> io::Result<()> {
        self.0.iter()
            .filter(|dev| dev.is_rotational)
            .map(|dev| dev.set_apm_level(level)).collect()
    }
}

pub struct Disk {
    path: PathBuf,
    is_rotational: bool,
}

impl DiskPower for Disk {
    fn set_apm_level(&self, level: u8) -> io::Result<()> {
        eprintln!("Setting APM level on {:?} to {}", &self.path, level);
        Command::new("hdparm")
            .arg("-B")
            .arg(level.to_string())
            .arg(&self.path)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|_| ())
    }
}
