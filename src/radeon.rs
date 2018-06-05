use std::path::Path;
use kernel_parameters::*;

pub struct RadeonDevice {
    path: String,
    pub dpm_state: RadeonDpmState,
    pub dpm_force_performance: RadeonDpmForcePerformance,
    pub power_method: RadeonPowerMethod,
    pub power_profile: RadeonPowerProfile,
}

impl RadeonDevice {
    pub fn new(card: u8) -> Option<RadeonDevice> {
        let path = format!("/sys/class/drm/card{}/device", card);
        let device = RadeonDevice {
            dpm_state: RadeonDpmState::new(&path),
            dpm_force_performance: RadeonDpmForcePerformance::new(&path),
            power_method: RadeonPowerMethod::new(&path),
            power_profile: RadeonPowerProfile::new(&path),
            path
        };

        // TODO: Better detection of Radeon cards.

        let exists = device.dpm_state.get_path().exists()
            && device.dpm_force_performance.get_path().exists()
            && device.power_method.get_path().exists()
            && device.power_profile.get_path().exists();

        if exists { Some(device) } else { None }
    }

    pub fn set_profiles(&self, power_profile: &str, dpm_state: &str, dpm_perf: &str) {
        self.dpm_state.set(dpm_state.as_bytes());
        self.dpm_force_performance.set(dpm_perf.as_bytes());
        self.power_method.set(b"profile");
        self.power_profile.set(power_profile.as_bytes());
    }

    // TODO: impl Iterator<Item = RadeonDevice>

    pub fn get_devices() -> Vec<RadeonDevice> {
        (0u8..10).flat_map(RadeonDevice::new).collect()
    }
}
