// Copyright 2018-2021 System76 <info@system76.com>
//
// SPDX-License-Identifier: GPL-3.0-only

use crate::{
    kernel_parameters::{DeviceList, KernelParameter, PowerLevel, PowerSave},
    modprobe,
};
use std::path::Path;

pub struct WifiDevice {
    device:      &'static str,
    power_save:  PowerSave,
    power_level: PowerLevel,
}

impl WifiDevice {
    #[must_use]
    pub fn new(device: &'static str) -> Option<Self> {
        if !Path::new(&["/sys/module/", device].concat()).exists() {
            return None;
        }

        Some(Self {
            device,
            power_save: PowerSave::new(device),
            power_level: PowerLevel::new(device),
        })
    }

    pub fn set(&self, power_level: u8) {
        if power_level > 5 {
            log::error!("invalid wifi power level. levels supported: 1-5");
            return;
        }

        if let (Some(ref save), Some(ref level)) = (self.power_save.get(), self.power_level.get()) {
            if power_level == 0 {
                if save == "Y" {
                    if let Err(why) = modprobe::reload(self.device, &["power_save=N"]) {
                        log::error!("failed to reload {} module: {}", self.device, why);
                    }
                }
            } else {
                let power_level = power_level.to_string();
                if save != "Y" || (save == "N" && level != &power_level) {
                    let options = &["power_save=Y", &format!("power_level={}", power_level)];
                    if let Err(why) = modprobe::reload(self.device, options) {
                        log::error!("failed to reload {} module: {}", self.device, why);
                    }
                }
            }
        }
    }
}

impl DeviceList<Self> for WifiDevice {
    const SUPPORTED: &'static [&'static str] = &["iwlwifi"];

    fn get_devices() -> Box<dyn Iterator<Item = Self>> {
        Box::new(Self::SUPPORTED.iter().filter_map(|dev| Self::new(dev)))
    }
}
