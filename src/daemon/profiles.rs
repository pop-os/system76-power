use super::{config::*, pci_runtime_pm_support};
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

pub fn apply_profile(profile: &ProfileConfig, errors: &mut Vec<ProfileError>) {
    // Manage screen and keyboard backlights.
    if let Some(ref backlight) = profile.backlight {
        if let Some(brightness) = backlight.keyboard {
            info!("setting keyboard brightness to {}%", brightness);
            let func = match backlight.method {
                BacklightMethod::None => Leds::set_brightness,
                BacklightMethod::Lower => Leds::set_if_lower_than,
            };

            catch!(errors, iterate_backlights(Leds::iter_keyboards(), func, brightness as u64));
        }

        if let Some(brightness) = backlight.screen {
            info!("setting screen brightness to {}%", brightness);
            let func = match backlight.method {
                BacklightMethod::None => Backlight::set_brightness,
                BacklightMethod::Lower => Backlight::set_if_lower_than,
            };

            catch!(errors, iterate_backlights(Backlight::iter(), func, brightness as u64));
        }
    }

    // Controls disk APM levels and autosuspend delays.
    if let Some(disk) = profile.disk {
        info!(
            "setting global HDD APM: {}; with autosuspend delay: {}s",
            disk.apm_level, disk.autosuspend_delay
        );
        catch!(errors, set_disk_power(disk.apm_level, (disk.autosuspend_delay * 1000) as i32));
    }

    // Enables the laptop mode feature in the kernel, which allows mechanical drives to spin down
    // when inactive.
    if let Some(laptop_mode) = profile.laptop_mode {
        LaptopMode::default().set(laptop_mode.to_string().as_bytes());
    }

    // The dirty kernel parameter controls how often the OS will sync data to disks. The less
    // frequently this occurs, the more power can be saved, yet the higher the risk of sudden
    // power loss causing loss of data. 15s is a resonable number.
    if let Some(max_lost_work) = profile.max_lost_work {
        Dirty::default().set_max_lost_work(max_lost_work);
    }

    // Toggles PCI device runtime power management support -- disabled by default.
    if pci_runtime_pm_support() {
        if let Some(support) = profile.pci_runtime_pm {
            catch!(
                errors,
                pci_device_runtime_pm(if support {
                    RuntimePowerManagement::On
                } else {
                    RuntimePowerManagement::Off
                })
            );
        }
    }

    // Control Intel PState values, if they exist.
    if let Some(pstate) = profile.pstate {
        info!("setting pstate values to {}-{}; turbo: {}", pstate.min, pstate.max, pstate.turbo);
        catch!(errors, pstate_values(pstate.min, pstate.max, !pstate.turbo));
    }

    // Sets radeon power profiles for AMD graphics.
    if let Some(radeon) = profile.radeon {
        RadeonDevice::get_devices().for_each(|dev| {
            dev.set_profiles(
                <&'static str>::from(radeon.profile),
                <&'static str>::from(radeon.dpm_state),
                <&'static str>::from(radeon.dpm_perf),
            )
        });
    }

    // Enables SCSI / SATA link time power management.
    if let Some([first, second]) = profile.scsi_host_link_time_pm_policy {
        catch!(
            errors,
            scsi_host_link_time_pm_policy(&[
                <&'static str>::from(first),
                <&'static str>::from(second),
            ])
        );
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
    strategy: fn(&B, u64) -> io::Result<()>,
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
fn scsi_host_link_time_pm_policy(policies: &[&'static str]) -> Result<(), ScsiHostError> {
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
