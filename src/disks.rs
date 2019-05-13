use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use crate::util::{read_file, write_file};

const AUTOSUSPEND: &str = "device/power/autosuspend_delay_ms";

pub trait DiskPower {
    fn set_apm_level(&self, level: u8) -> io::Result<()>;
    fn set_autosuspend_delay(&self, ms: i32) -> io::Result<()>;
}

pub struct Disks(Vec<Disk>);

impl Default for Disks {
    fn default() -> Disks {
        let mut disks = Vec::new();
        let blocks = match Path::new("/sys/block").read_dir() {
            Ok(blocks) => blocks,
            Err(why) => {
                eprintln!("disks: unable to get block devices: {}", why);
                return Disks(disks);
            }
        };

        for device in blocks.flat_map(Result::ok) {
            if device.path().join("slaves").exists() {
                if let Ok(name) = device.file_name().into_string() {
                    if name.starts_with("loop") || name.starts_with("dm") {
                        continue
                    }

                    disks.push(Disk {
                        path: PathBuf::from(["/dev/", &name].concat()),
                        block: PathBuf::from(["/sys/block/", &name].concat()),
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

    fn set_autosuspend_delay(&self, ms: i32) -> io::Result<()> {
        self.0.iter()
            .filter(|dev| dev.is_rotational)
            .map(|dev| dev.set_autosuspend_delay(ms)).collect()
    }
}

pub struct Disk {
    path: PathBuf,
    block: PathBuf,
    is_rotational: bool,
}

impl DiskPower for Disk {
    fn set_apm_level(&self, level: u8) -> io::Result<()> {
        debug!("Setting APM level on {:?} to {}", &self.path, level);
        Command::new("hdparm")
            .arg("-B")
            .arg(level.to_string())
            .arg(&self.path)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|_| ())
    }

    fn set_autosuspend_delay(&self, ms: i32) -> io::Result<()> {
        debug!("Setting autosuspend delay on {:?} to {}", &self.block, ms);
        write_file(&self.block.join(AUTOSUSPEND), ms.to_string().as_bytes())
    }
}
