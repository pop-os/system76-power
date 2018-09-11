#![allow(unused)]
use std::path::{Path, PathBuf};
use std::str;
use util::{read_file, write_file};

/// Base trait that implements kernel parameter get/set capabilities.
pub trait KernelParameter {
    const NAME: &'static str;

    fn get_path(&self) -> &Path;

    fn get(&self) -> Option<String> {
        let path = self.get_path();
        if path.exists() {
            match read_file(path) {
                Ok(mut value) => {
                    value.pop();
                    return Some(value);
                },
                Err(why) => {
                    error!("{}: failed to get value: {}", path.display(), why)
                }
            }
        } else {
            warn!("{} does not exist", path.display());
        }

        None
    }

    fn set(&self, value: &[u8]) {
        let path = self.get_path();
        if path.exists() {
            debug!("Modifying kernel parameter at {:?} to {}", path, match str::from_utf8(value) {
                Ok(string) => string,
                Err(_) => "[INVALID UTF8]",
            });

            if let Err(why) = write_file(path, value) {
                error!("{}: failed to set value: {}", path.display(), why)
            }
        } else {
            warn!("{} does not exist", path.display());
        }
    }
}

pub trait DeviceList<T> {
    const SUPPORTED: &'static [&'static str];

    fn get_devices() -> Box<Iterator<Item = T>>;
}

// Macros to help with constructing kernel parameter structures.

macro_rules! static_parameters {
    ($($struct:tt { $name:tt : $path:expr }),+) => (
        $(
            pub struct $struct;

            impl $struct { pub fn new() -> $struct { $struct } }

            impl KernelParameter for $struct {
                const NAME: &'static str = stringify!($name);

                fn get_path<'a>(&'a self) -> &'a Path {
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
                pub fn new(unique: &str) -> $struct {
                    $struct {
                        path: PathBuf::from(format!($format, unique))
                    }
                }
            }

            impl KernelParameter for $struct {
                const NAME: &'static str = stringify!($name);

                fn get_path<'a>(&'a self) -> &'a Path { &self.path }
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

/// Control whether a device uses, or does not use, runtime power management.
pub enum RuntimePowerManagement {
    On,
    Off,
}

impl From<RuntimePowerManagement> for &'static str {
    fn from(pm: RuntimePowerManagement) -> &'static str {
        match pm {
            RuntimePowerManagement::On => "auto",
            RuntimePowerManagement::Off => "on",
        }
    }
}

pub struct Dirty {
    expire: DirtyExpire,
    writeback: DirtyWriteback,
}

impl Dirty {
    pub fn new() -> Dirty {
        Dirty {
            expire: DirtyExpire::new(),
            writeback: DirtyWriteback::new(),
        }
    }

    pub fn set_max_lost_work(&self, secs: u32) {
        let centisecs = (secs as u64 * 100).to_string();
        let centisecs = centisecs.as_bytes();
        self.expire.set(centisecs);
        self.writeback.set(centisecs);
    }
}
