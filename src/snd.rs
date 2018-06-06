use std::path::Path;
use kernel_parameters::*;

pub struct SoundDevice {
    power_save: PowerSave,
    power_save_controller: Option<SndPowerSaveController>
}

impl SoundDevice {
    pub fn new(device: &str) -> Option<SoundDevice> {
        if !Path::new(&["/sys/module/", device].concat()).exists() {
            return None;
        }

        let controller = SndPowerSaveController::new(device);
        Some(SoundDevice {
            power_save: PowerSave::new(device),
            power_save_controller: if controller.get_path().exists() {
                Some(controller)
            } else {
                None
            }
        })
    }

    pub fn set_power_save(&self, timeout: u32, enable_controller: bool) {
        self.power_save.set(timeout.to_string().as_bytes());
        if let Some(ref controller) = self.power_save_controller {
            controller.set(if enable_controller { b"Y" } else { b"N" });
        }
    }
}

impl DeviceList<SoundDevice> for SoundDevice {
    const SUPPORTED: &'static [&'static str] = &["snd_hda_intel", "snd_ac97_codec"];

    fn get_devices() -> Box<Iterator<Item = SoundDevice>> {
        Box::new(Self::SUPPORTED.into_iter().flat_map(|dev| SoundDevice::new(dev)))
    }
}
