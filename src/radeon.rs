// Copyright 2018-2021 System76 <info@system76.com>
//
// SPDX-License-Identifier: GPL-3.0-only

use crate::kernel_parameters::{
    DeviceList, KernelParameter, RadeonDpmForcePerformance, RadeonDpmState, RadeonPowerMethod,
    RadeonPowerProfile,
};

pub struct RadeonDevice {
    card:                      u8,
    pub dpm_state:             RadeonDpmState,
    pub dpm_force_performance: RadeonDpmForcePerformance,
    pub power_method:          RadeonPowerMethod,
    pub power_profile:         RadeonPowerProfile,
}

impl RadeonDevice {
    #[must_use]
    pub fn new(card: u8) -> Option<Self> {
        let path = format!("/sys/class/drm/card{}/device", card);
        let device = Self {
            card,
            dpm_state: RadeonDpmState::new(&path),
            dpm_force_performance: RadeonDpmForcePerformance::new(&path),
            power_method: RadeonPowerMethod::new(&path),
            power_profile: RadeonPowerProfile::new(&path),
        };

        // TODO: Better detection of Radeon cards.

        let exists = device.dpm_state.get_path().exists()
            && device.dpm_force_performance.get_path().exists()
            && device.power_method.get_path().exists()
            && device.power_profile.get_path().exists();

        if exists {
            Some(device)
        } else {
            None
        }
    }

    pub fn set_profiles(&self, power_profile: &str, dpm_state: &str, dpm_perf: &str) {
        log::debug!(
            "Setting radeon{} to power profile {}; DPM state {}; DPM perf {}",
            self.card,
            power_profile,
            dpm_state,
            dpm_perf
        );
        self.dpm_state.set(dpm_state.as_bytes());
        self.dpm_force_performance.set(dpm_perf.as_bytes());
        self.power_method.set(b"profile");
        self.power_profile.set(power_profile.as_bytes());
    }
}

impl DeviceList<Self> for RadeonDevice {
    const SUPPORTED: &'static [&'static str] = &[""];

    fn get_devices() -> Box<dyn Iterator<Item = Self>> { Box::new((0u8..10).filter_map(Self::new)) }
}
