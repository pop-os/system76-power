use std::{fs, io};
use sysfs_class::{PciDevice, RuntimePM, RuntimePowerManagement, SysClass};

pub fn runtime_pm_quirks() -> io::Result<()> {
    let vendor = fs::read_to_string("/sys/class/dmi/id/sys_vendor")?;
    let model = fs::read_to_string("/sys/class/dmi/id/product_version")?;

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
        _ => (),
    }

    Ok(())
}
