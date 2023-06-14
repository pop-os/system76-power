// Copyright 2018-2021 System76 <info@system76.com>
//
// SPDX-License-Identifier: GPL-3.0-only

use crate::errors::DiskPowerError;
use std::{
    fs::{read_to_string, write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

const AUTOSUSPEND: &str = "device/power/autosuspend_delay_ms";

pub trait DiskPower {
    fn set_apm_level(&self, level: u8) -> Result<(), DiskPowerError>;
    fn set_autosuspend_delay(&self, ms: i32) -> Result<(), DiskPowerError>;
}

pub struct Disks(Vec<Disk>);

impl Default for Disks {
    fn default() -> Disks {
        let mut disks = Vec::new();
        let blocks = match Path::new("/sys/block").read_dir() {
            Ok(blocks) => blocks,
            Err(why) => {
                log::warn!("unable to get block devices: {}", why);
                return Disks(disks);
            }
        };

        for device in blocks.filter_map(Result::ok) {
            if device.path().join("slaves").exists() {
                if let Ok(name) = device.file_name().into_string() {
                    if name.starts_with("loop") || name.starts_with("dm") || name.starts_with("md")
                    {
                        continue;
                    }

                    disks.push(Disk {
                        path:          PathBuf::from(["/dev/", &name].concat()),
                        block:         PathBuf::from(["/sys/block/", &name].concat()),
                        is_rotational: {
                            read_to_string(device.path().join("queue/rotational"))
                                .ok()
                                .map_or(false, |string| string.trim() == "1")
                        },
                    });
                }
            }
        }

        Disks(disks)
    }
}

impl DiskPower for Disks {
    fn set_apm_level(&self, level: u8) -> Result<(), DiskPowerError> {
        self.0.iter().filter(|dev| dev.is_rotational).try_for_each(|dev| dev.set_apm_level(level))
    }

    fn set_autosuspend_delay(&self, ms: i32) -> Result<(), DiskPowerError> {
        self.0
            .iter()
            .filter(|dev| dev.is_rotational)
            .try_for_each(|dev| dev.set_autosuspend_delay(ms))
    }
}

pub struct Disk {
    path:          PathBuf,
    block:         PathBuf,
    is_rotational: bool,
}

impl DiskPower for Disk {
    fn set_apm_level(&self, level: u8) -> Result<(), DiskPowerError> {
        log::debug!("Setting APM level on {:?} to {}", &self.path, level);
        Command::new("hdparm")
            .arg("-B")
            .arg(level.to_string())
            .arg(&self.path)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map_err(|why| DiskPowerError::ApmLevel(self.path.clone(), level, why))
            .map(|_| ())
    }

    fn set_autosuspend_delay(&self, ms: i32) -> Result<(), DiskPowerError> {
        log::debug!("Setting autosuspend delay on {:?} to {}", &self.block, ms);
        write(self.block.join(AUTOSUSPEND), ms.to_string().as_bytes())
            .map_err(|why| DiskPowerError::AutosuspendDelay(self.block.clone(), ms, why))
    }
}
