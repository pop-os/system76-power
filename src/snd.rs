use std::path::Path;
use kernel_parameters::*;

pub const SND_DEVICES: &[&str] = &["snd_hda_intel", "snd_ac97_codec"];

pub struct SoundDevice {
    power_save: SndPowerSave,
    power_save_controller: Option<SndPowerSaveController>
}

impl SoundDevice {
    pub fn new(device: &str) -> Option<SoundDevice> {
        if !Path::new(device).exists() {
            return None;
        }

        let controller = SndPowerSaveController::new(device);
        Some(SoundDevice {
            power_save: SndPowerSave::new(device),
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
