use crate::{kernel_parameters::*, modprobe};
use std::path::Path;

pub struct WifiDevice {
    device:      &'static str,
    power_save:  PowerSave,
    power_level: PowerLevel,
}

impl WifiDevice {
    pub fn new(device: &'static str) -> Option<WifiDevice> {
        if !Path::new(&["/sys/module/", device].concat()).exists() {
            return None;
        }

        Some(WifiDevice {
            device,
            power_save: PowerSave::new(device),
            power_level: PowerLevel::new(device),
        })
    }

    pub fn set(&self, power_level: u8) {
        if power_level > 5 {
            error!("invalid wifi power level. levels supported: 1-5");
            return;
        }

        if let (Some(ref save), Some(ref level)) = (self.power_save.get(), self.power_level.get()) {
            if power_level == 0 {
                if save == "Y" {
                    if let Err(why) = modprobe::reload(self.device, &["power_save=N"]) {
                        error!("failed to reload {} module: {}", self.device, why);
                    }
                }
            } else {
                let power_level = power_level.to_string();
                if save != "Y" || (save == "N" && level != &power_level) {
                    let options = &["power_save=Y", &format!("power_level={}", power_level)];
                    if let Err(why) = modprobe::reload(self.device, options) {
                        error!("failed to reload {} module: {}", self.device, why);
                    }
                }
            }
        }
    }
}

impl DeviceList<WifiDevice> for WifiDevice {
    const SUPPORTED: &'static [&'static str] = &["iwlwifi"];

    fn get_devices() -> Box<dyn Iterator<Item = WifiDevice>> {
        Box::new(Self::SUPPORTED.iter().flat_map(|dev| WifiDevice::new(dev)))
    }
}
