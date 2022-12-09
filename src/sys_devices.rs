// Copyright 2022 System76 <info@system76.com>
// SPDX-License-Identifier: GPL-3.0-only

pub mod i2c {
    use std::path::PathBuf;
    use sysfs_class::RuntimePowerManagement;

    pub struct I2cDevice {
        path: PathBuf,
    }

    impl I2cDevice {
        pub fn set_runtime_pm(&self, pm: RuntimePowerManagement) {
            let _res = std::fs::write(
                self.path.join("device/power/control"),
                match pm {
                    RuntimePowerManagement::Off => "on",
                    RuntimePowerManagement::On => "auto",
                },
            );
        }
    }

    pub fn devices() -> impl Iterator<Item = I2cDevice> {
        std::fs::read_dir("/sys/bus/i2c/devices/")
            .ok()
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .map(|entry| I2cDevice { path: entry.path() })
    }
}

pub mod pci {
    use std::path::PathBuf;
    use sysfs_class::RuntimePowerManagement;

    pub struct PciDevice {
        path: PathBuf,
    }

    impl PciDevice {
        pub fn set_runtime_pm(&self, pm: RuntimePowerManagement) {
            let _res = std::fs::write(
                self.path.join("power/control"),
                match pm {
                    RuntimePowerManagement::Off => "on",
                    RuntimePowerManagement::On => "auto",
                },
            );
        }
    }

    pub fn devices() -> impl Iterator<Item = PciDevice> {
        std::fs::read_dir("/sys/bus/pci/devices/")
            .ok()
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .map(|entry| PciDevice { path: entry.path() })
    }
}

pub mod usb {
    use std::path::PathBuf;
    use sysfs_class::RuntimePowerManagement;

    pub struct UsbDevice {
        path: PathBuf,
    }

    impl UsbDevice {
        pub fn set_runtime_pm(&self, pm: RuntimePowerManagement) {
            let _res = std::fs::write(
                self.path.join("power/control"),
                match pm {
                    RuntimePowerManagement::Off => "on",
                    RuntimePowerManagement::On => "auto",
                },
            );
        }
    }

    pub fn devices() -> impl Iterator<Item = UsbDevice> {
        std::fs::read_dir("/sys/bus/usb/devices/")
            .ok()
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .map(|entry| UsbDevice { path: entry.path() })
    }
}
