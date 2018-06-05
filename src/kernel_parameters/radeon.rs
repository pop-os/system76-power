#![allow(unused)]

use super::*;

pub struct RadeonDevice {
    path: String,
    pub dpm_state: RadeonDpmState,
    pub dpm_force_performance: RadeonDpmForcePerformance,
    pub power_method: RadeonPowerMethod,
    pub power_profile: RadeonPowerProfile,

}

impl RadeonDevice {
    pub fn new(card: u32) -> RadeonDevice {
        let path = format!("/sys/class/drm/card{}", card);
        RadeonDevice {
            dpm_state: RadeonDpmState::new(&path),
            dpm_force_performance: RadeonDpmForcePerformance::new(&path),
            power_method: RadeonPowerMethod::new(&path),
            power_profile: RadeonPowerProfile::new(&path),
            path
        }
    }
}
