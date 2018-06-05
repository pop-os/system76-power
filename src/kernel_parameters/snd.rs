#![allow(unused)]
use std::path::Path;
use super::*;

pub const HDA_INTEL: &str = "snd_hda_intel";
pub const AC97: &str = "snd_ac97_codec";

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
}
