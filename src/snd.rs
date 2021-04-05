use crate::kernel_parameters::*;
use std::path::Path;

pub struct SoundDevice {
    device:                &'static str,
    power_save:            PowerSave,
    power_save_controller: Option<PowerSaveController>,
}

impl SoundDevice {
    pub fn new(device: &'static str) -> Option<SoundDevice> {
        if !Path::new(&["/sys/module/", device].concat()).exists() {
            return None;
        }

        let controller = PowerSaveController::new(device);
        Some(SoundDevice {
            device,
            power_save: PowerSave::new(device),
            power_save_controller: if controller.get_path().exists() {
                Some(controller)
            } else {
                None
            },
        })
    }

    pub fn set_power_save(&self, timeout: u32, enable_controller: bool) {
        log::debug!(
            "{} power controller for {}, with power save timeout value of {}",
            if enable_controller { "Enabling" } else { "Disabling" },
            self.device,
            timeout
        );

        self.power_save.set(timeout.to_string().as_bytes());
        if let Some(ref controller) = self.power_save_controller {
            controller.set(if enable_controller { b"Y" } else { b"N" });
        }
    }
}

impl DeviceList<SoundDevice> for SoundDevice {
    const SUPPORTED: &'static [&'static str] = &["snd_hda_intel", "snd_ac97_codec"];

    fn get_devices() -> Box<dyn Iterator<Item = SoundDevice>> {
        Box::new(Self::SUPPORTED.iter().flat_map(|dev| SoundDevice::new(dev)))
    }
}
