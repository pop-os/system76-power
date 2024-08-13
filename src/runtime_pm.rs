use std::{fs, io};
use sysfs_class::{PciDevice, RuntimePM, RuntimePowerManagement, SysClass};

pub fn runtime_pm_quirks(vendor: &str, model: &str) -> io::Result<()> {
    match (vendor.trim(), model.trim()) {
        ("System76", "bonw15") => {
            for dev in PciDevice::all()? {
                match (dev.vendor()?, dev.device()?) {
                    (0x8086, 0x1138) => {
                        log::info!(
                            "Disabling runtime power management on Thunderbolt XHCI device at {:?}",
                            dev.path()
                        );
                        dev.set_runtime_pm(RuntimePowerManagement::Off)?;
                    }
                    _ => (),
                }
            }
        }
        ("System76", "bonw15-b") => {
            for dev in PciDevice::all()? {
                match (dev.vendor()?, dev.device()?) {
                    (0x8086, 0x5782) => {
                        log::info!(
                            "Disabling runtime power management on Thunderbolt XHCI device at {:?}",
                            dev.path()
                        );
                        dev.set_runtime_pm(RuntimePowerManagement::Off)?;
                    }
                    _ => (),
                }
            }
        }
        _ => (),
    }

    Ok(())
}

pub fn thunderbolt_hotplug_wakeup(vendor: &str, model: &str) -> io::Result<()> {
    match (vendor.trim(), model.trim()) {
        ("System76", "bonw15-b") => {
            fs::read("/sys/kernel/debug/thunderbolt/0-0/regs")?;
        }
        (..) => {}
    }

    Ok(())
}
