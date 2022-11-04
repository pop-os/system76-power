// Copyright 2018-2021 System76 <info@system76.com>
//
// SPDX-License-Identifier: GPL-3.0-only

#![allow(unused)]
pub use sysfs_class::RuntimePowerManagement;

use std::{
    fs::{read_to_string, write},
    path::{Path, PathBuf},
    str,
};

/// Base trait that implements kernel parameter get/set capabilities.
pub trait KernelParameter {
    const NAME: &'static str;

    fn get_path(&self) -> &Path;

    fn get(&self) -> Option<String> {
        let path = self.get_path();
        if path.exists() {
            match read_to_string(path) {
                Ok(mut value) => {
                    value.pop();
                    return Some(value);
                }
                Err(why) => log::error!("{}: failed to get value: {}", path.display(), why),
            }
        } else {
            log::warn!("{} does not exist", path.display());
        }

        None
    }

    fn set(&self, value: &[u8]) {
        let path = self.get_path();
        if path.exists() {
            log::debug!(
                "Modifying kernel parameter at {:?} to {}",
                path,
                str::from_utf8(value).unwrap_or("[INVALID UTF8]")
            );

            if let Err(why) = write(path, value) {
                log::error!("{}: failed to set value: {}", path.display(), why);
            }
        } else {
            log::warn!("{} does not exist", path.display());
        }
    }
}

pub trait DeviceList<T> {
    const SUPPORTED: &'static [&'static str];

    fn get_devices() -> Box<dyn Iterator<Item = T>>;
}

// Macros to help with constructing kernel parameter structures.

macro_rules! static_parameters {
    ($($struct:tt { $name:tt : $path:expr }),+) => (
        $(
            pub struct $struct;

            impl Default for $struct { fn default() -> Self { $struct } }

            impl KernelParameter for $struct {
                const NAME: &'static str = stringify!($name);

                fn get_path(&self) -> &Path {
                    Path::new($path)
                }
            }
        )+
    );
}

macro_rules! dynamic_parameters {
    ($($struct:tt { $name:tt : $format:expr }),+) => (
        $(
            pub struct $struct {
                path: PathBuf
            }

            impl $struct {
                #[must_use]
                pub fn new(unique: &str) -> $struct {
                    $struct {
                        path: PathBuf::from(format!($format, unique))
                    }
                }
            }

            impl KernelParameter for $struct {
                const NAME: &'static str = stringify!($name);

                fn get_path(&self) -> &Path { &self.path }
            }
        )+
    );
}

// Kernel parameters which implement the base trait.

static_parameters! {
    LaptopMode { laptop_mode: "/proc/sys/vm/laptop_mode" },
    DirtyExpire { dirty_expire: "/proc/sys/vm/dirty_expire_centisecs" },
    DirtyWriteback { dirty_writeback: "/proc/sys/vm/dirty_writeback_centisecs" },
    NmiWatchdog { nmi_watchdog : "/proc/sys/kernel/nmi_watchdog" },
    PcieAspm { pcie_aspm: "/sys/module/pcie_aspm/parameters/policy" }
}

dynamic_parameters! {
    DiskIoSched { disk_io_scheduler: "/sys/block/{}/queue/scheduler" },
    PhcControls { phc_controls: "/sys/devices/system/cpu/cpu{}/cpufreq/phc_controls" },
    RadeonDpmState { radeon_dpm_state: "{}/power_dpm_state" },
    RadeonDpmForcePerformance {
        radeon_dpm_force_performance_level: "{}/power_dpm_force_performance_level"
    },
    RadeonPowerMethod { radeon_power_method: "{}/power_method" },
    RadeonPowerProfile { radeon_power_profile: "{}/power_profile" },
    PowerSave { power_save: "/sys/module/{}/parameters/power_save" },
    PowerLevel { power_level: "/sys/module/{}/parameters/power_level" },
    PowerSaveController {
        power_save_controller: "/sys/module/{}/parameters/power_save_controller"
    }
}

#[derive(Default)]
pub struct Dirty {
    expire:    DirtyExpire,
    writeback: DirtyWriteback,
}

impl Dirty {
    pub fn set_max_lost_work(&self, secs: u32) {
        let centisecs = (u64::from(secs) * 100).to_string();
        let centisecs = centisecs.as_bytes();
        self.expire.set(centisecs);
        self.writeback.set(centisecs);
    }
}
