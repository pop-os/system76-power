use super::pci_runtime_pm_support;
use crate::{
    disks::{DiskPower, Disks},
    errors::{BacklightError, DiskPowerError, PciDeviceError, ProfileError, ScsiHostError},
    kernel_parameters::{DeviceList, Dirty, KernelParameter, LaptopMode},
    radeon::RadeonDevice,
};
use pstate::{PState, PStateError};
use std::io;
use sysfs_class::{
    Backlight, Brightness, Leds, PciDevice, RuntimePM, RuntimePowerManagement, ScsiHost, SysClass,
};

/// Instead of returning on the first error, we want to collect all errors that occur while
/// setting a profile. Even if one parameter fails to set, we'll still be able to set other
/// parameters successfully.
macro_rules! catch {
    ($errors:ident, $result:expr) => {
        match $result {
            Ok(_) => (),
            Err(why) => $errors.push(why.into()),
        }
    };
}

/// Sets parameters for the balanced profile.
pub fn balanced(errors: &mut Vec<ProfileError>) {
    // The dirty kernel parameter controls how often the OS will sync data to disks. The less
    // frequently this occurs, the more power can be saved, yet the higher the risk of sudden
    // power loss causing loss of data. 15s is a resonable number.
    Dirty::default().set_max_lost_work(15);

    // Enables the laptop mode feature in the kernel, which allows mechanical drives to spin down
    // when inactive.
    LaptopMode::default().set(b"2");

    // Sets radeon power profiles for AMD graphics.
    RadeonDevice::get_devices().for_each(|dev| dev.set_profiles("auto", "performance", "auto"));

    // Controls disk APM levels and autosuspend delays.
    catch!(errors, set_disk_power(127, 60000));

    // Enables SCSI / SATA link time power management.
    catch!(errors, scsi_host_link_time_pm_policy(&["med_power_with_dipm", "medium_power"]));

    // Manage screen backlights.
    catch!(errors, iterate_backlights(Backlight::iter(), &Brightness::set_if_lower_than, 40));

    // Manage keyboard backlights.
    catch!(errors, iterate_backlights(Leds::iter_keyboards(), &Brightness::set_if_lower_than, 50));

    // Parameters which may cause on certain systems.
    if pci_runtime_pm_support() {
        // Enables PCI device runtime power management.
        catch!(errors, pci_device_runtime_pm(RuntimePowerManagement::On));
    }

    // Control Intel PState values, if they exist.
    catch!(errors, pstate_values(0, 100, false));
}

/// Sets parameters for the perfromance profile
pub fn performance(errors: &mut Vec<ProfileError>) {
    Dirty::default().set_max_lost_work(15);
    LaptopMode::default().set(b"0");
    RadeonDevice::get_devices().for_each(|dev| dev.set_profiles("high", "performance", "auto"));
    catch!(errors, set_disk_power(254, 300000));
    catch!(errors, scsi_host_link_time_pm_policy(&["med_power_with_dipm", "max_performance"]));
    catch!(errors, pstate_values(50, 100, false));

    if pci_runtime_pm_support() {
        catch!(errors, pci_device_runtime_pm(RuntimePowerManagement::Off));
    }
}

/// Sets parameters for the battery profile
pub fn battery(errors: &mut Vec<ProfileError>) {
    Dirty::default().set_max_lost_work(15);
    LaptopMode::default().set(b"2");
    RadeonDevice::get_devices().for_each(|dev| dev.set_profiles("low", "battery", "low"));
    catch!(errors, set_disk_power(127, 15000));
    catch!(errors, scsi_host_link_time_pm_policy(&["min_power", "min_power"]));
    catch!(errors, iterate_backlights(Backlight::iter(), &Brightness::set_if_lower_than, 10));
    catch!(errors, iterate_backlights(Leds::iter_keyboards(), &Brightness::set_brightness, 0));
    catch!(errors, pstate_values(0, 50, true));

    if pci_runtime_pm_support() {
        catch!(errors, pci_device_runtime_pm(RuntimePowerManagement::On));
    }
}

/// Controls the Intel PState values.
fn pstate_values(min: u8, max: u8, no_turbo: bool) -> Result<(), PStateError> {
    if let Ok(pstate) = PState::new() {
        pstate.set_min_perf_pct(min)?;
        pstate.set_max_perf_pct(max)?;
        pstate.set_no_turbo(no_turbo)?;
    }

    Ok(())
}

/// Iterates across all backlights in the supplied iterator, executing the given strategy function
/// on each discovered backlight source.
fn iterate_backlights<B: Brightness>(
    iterator: impl Iterator<Item = io::Result<B>>,
    strategy: &dyn Fn(&B, u64) -> io::Result<()>,
    value: u64,
) -> Result<(), BacklightError> {
    for backlight in iterator {
        match backlight {
            Ok(ref backlight) => set_backlight(strategy, backlight, value)?,
            Err(why) => {
                warn!("failed to iterate keyboard backlight: {}", why);
            }
        }
    }

    Ok(())
}

/// Iterates on all available PCI devices, disabling or enabling runtime power mangement.
fn pci_device_runtime_pm(pm: RuntimePowerManagement) -> Result<(), PciDeviceError> {
    for device in PciDevice::iter() {
        match device {
            Ok(device) => device
                .set_runtime_pm(pm)
                .map_err(|why| PciDeviceError::SetRuntimePM(device.id().to_owned(), why))?,
            Err(why) => {
                warn!("failed to iterate PCI device: {}", why);
            }
        }
    }

    Ok(())
}

/// Iterates on all available SCSI/SATA hosts, setting the first link time power mangement policy
/// that succeeeds.
fn scsi_host_link_time_pm_policy(policies: &'static [&'static str]) -> Result<(), ScsiHostError> {
    for device in ScsiHost::iter() {
        match device {
            Ok(device) => {
                device.set_link_power_management_policy(policies).map_err(|why| {
                    ScsiHostError::LinkTimePolicy(policies[0], device.id().to_owned(), why)
                })?;
            }
            Err(why) => {
                warn!("failed to iterate SCSI Host device: {}", why);
            }
        }
    }

    Ok(())
}

/// Generically sets a backlight value to the backlight, using the provided strategy function.
fn set_backlight<B: Brightness>(
    strategy: impl Fn(&B, u64) -> io::Result<()>,
    backlight: &B,
    value: u64,
) -> Result<(), BacklightError> {
    strategy(backlight, value)
        .map_err(|why| BacklightError::Set(backlight.id().to_owned(), why))?;
    Ok(())
}

/// Controls disk APM levels and autosuspend delays. Only applicable for mechanical drives.
fn set_disk_power(apm_level: u8, autosuspend_delay: i32) -> Result<(), DiskPowerError> {
    let disks = Disks::default();
    disks.set_apm_level(apm_level)?;
    disks.set_autosuspend_delay(autosuspend_delay)?;
    Ok(())
}
