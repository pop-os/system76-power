use std::path::Path;
use kernel_parameters::*;

pub struct WifiDevice {
    // Enable power management
    power_save: PowerSave,
    power_level: PowerLevel,
}

impl WifiDevice {
    pub fn new(device: &str) -> Option<WifiDevice> {
        if !Path::new(device).exists() {
            return None;
        }

        Some(
            WifiDevice {
                power_save: PowerSave::new(device),
                power_level: PowerLevel::new(device),
            }
        )
    }

    pub fn set(&self, power_level: u8) {
        if power_level > 5 {
            eprintln!("invalid wifi power level. levels supported: 1-5");
            return;
        }

        if power_level == 0 {
            self.power_save.set(b"N");
        } else {
            self.power_save.set(b"Y");
            self.power_level.set(power_level.to_string().as_bytes());
        }
    }
}

impl DeviceList<WifiDevice> for WifiDevice {
    const SUPPORTED: &'static [&'static str] = &["iwlwifi"];

    fn get_devices() -> Box<Iterator<Item = WifiDevice>> {
        Box::new(Self::SUPPORTED.into_iter().flat_map(|dev| WifiDevice::new(dev)))
    }
}
