// Copyright 2018-2021 System76 <info@system76.com>
//
// SPDX-License-Identifier: GPL-3.0-only

use crate::kernel_parameters::{
    DeviceList, KernelParameter, RadeonDpmForcePerformance, RadeonDpmState, RadeonPowerMethod,
    RadeonPowerProfile,
};

pub struct RadeonDevice {
    card: u8,
    pub dpm_state: RadeonDpmState,
    pub dpm_force_performance: RadeonDpmForcePerformance,
    pub power_method: RadeonPowerMethod,
    pub power_profile: RadeonPowerProfile,
}

impl RadeonDevice {
    #[must_use]
    pub fn new(card: u8) -> Option<Self> {
        let path = format!("/sys/class/drm/card{}/device", card);
        log::debug!("Checking for AMD GPU at: {}", path);

        let device = Self {
            card,
            dpm_state: RadeonDpmState::new(&path),
            dpm_force_performance: RadeonDpmForcePerformance::new(&path),
            power_method: RadeonPowerMethod::new(&path),
            power_profile: RadeonPowerProfile::new(&path),
        };

        // Modern AMD GPUs (Vega, RDNA, etc.) only require DPM state and force performance level.
        // Legacy fields (power_method, power_profile) are optional for backward compatibility
        // with older Radeon GPUs but are not present on modern AMD integrated/discrete GPUs.
        let dpm_state_exists = device.dpm_state.get_path().exists();
        let dpm_force_exists = device.dpm_force_performance.get_path().exists();
        let power_method_exists = device.power_method.get_path().exists();
        let power_profile_exists = device.power_profile.get_path().exists();

        log::debug!("  card{} sysfs paths:", card);
        log::debug!("    power_dpm_state: {}", if dpm_state_exists { "EXISTS" } else { "MISSING" });
        log::debug!(
            "    power_dpm_force_performance_level: {}",
            if dpm_force_exists { "EXISTS" } else { "MISSING" }
        );
        log::debug!(
            "    power_method: {}",
            if power_method_exists { "EXISTS (legacy)" } else { "MISSING (modern GPU)" }
        );
        log::debug!(
            "    power_profile: {}",
            if power_profile_exists { "EXISTS (legacy)" } else { "MISSING (modern GPU)" }
        );

        let has_essential_controls = dpm_state_exists && dpm_force_exists;

        if has_essential_controls {
            log::info!("AMD GPU card{} detected and will be managed (modern AMD GPU)", card);
            Some(device)
        } else {
            log::debug!("card{} does not have essential DPM controls - skipping", card);
            None
        }
    }

    pub fn set_profiles(&self, power_profile: &str, dpm_state: &str, dpm_perf: &str) {
        log::info!("Setting AMD GPU card{} power management:", self.card);
        log::info!("  Target profile: {}", power_profile);
        log::info!("  DPM state: {}", dpm_state);
        log::info!("  DPM performance level: {}", dpm_perf);

        // Set DPM controls (required, present on all modern AMD GPUs)
        log::debug!("  Writing DPM state to: {}", self.dpm_state.get_path().display());
        self.dpm_state.set(dpm_state.as_bytes());

        log::debug!(
            "  Writing DPM force performance level to: {}",
            self.dpm_force_performance.get_path().display()
        );
        self.dpm_force_performance.set(dpm_perf.as_bytes());

        // Set legacy power controls (optional, only on older Radeon GPUs)
        // The set() method will log a warning if these paths don't exist, but won't crash
        if self.power_method.get_path().exists() {
            log::debug!("  Writing power method to: {}", self.power_method.get_path().display());
            self.power_method.set(b"profile");
        } else {
            log::debug!("  Skipping power_method (not present on modern AMD GPUs)");
        }

        if self.power_profile.get_path().exists() {
            log::debug!("  Writing power profile to: {}", self.power_profile.get_path().display());
            self.power_profile.set(power_profile.as_bytes());
        } else {
            log::debug!("  Skipping power_profile (not present on modern AMD GPUs)");
        }

        log::info!("  AMD GPU card{} configuration completed", self.card);
    }
}

impl DeviceList<Self> for RadeonDevice {
    const SUPPORTED: &'static [&'static str] = &[""];

    fn get_devices() -> Box<dyn Iterator<Item = Self>> {
        Box::new((0u8..10).filter_map(Self::new))
    }
}
