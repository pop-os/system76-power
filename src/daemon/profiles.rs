// Copyright 2018-2021 System76 <info@system76.com>
//
// SPDX-License-Identifier: GPL-3.0-only

use super::pci_runtime_pm_support;
use crate::{
    errors::{BacklightError, ModelError, PciDeviceError, ProfileError, ScsiHostError},
    kernel_parameters::{DeviceList, Dirty, KernelParameter, LaptopMode},
    radeon::RadeonDevice,
    Profile,
};
use intel_pstate::{PState, PStateError, PStateValues};
use std::{
    fs,
    io::{self, Read, Seek, SeekFrom, Write},
    path::Path,
    process::Command,
};
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
pub fn balanced(errors: &mut Vec<ProfileError>, _on_battery: bool, set_brightness: bool) {
    // Use the ACPI Platform Profile if the hardware is supported by the kernel.
    if crate::acpi_platform::supported() {
        crate::acpi_platform::balanced();
    }

    // The dirty kernel parameter controls how often the OS will sync data to disks. The less
    // frequently this occurs, the more power can be saved, yet the higher the risk of sudden
    // power loss causing loss of data. 15s is a reasonable number.
    Dirty::default().set_max_lost_work(15);

    // Enables the laptop mode feature in the kernel, which allows mechanical drives to spin down
    // when inactive.
    LaptopMode::default().set(b"2");

    // Sets radeon power profiles for AMD graphics.
    RadeonDevice::get_devices().for_each(|dev| dev.set_profiles("auto", "performance", "auto"));

    // Enables SCSI / SATA link time power management.
    catch!(errors, scsi_host_link_time_pm_policy(&["med_power_with_dipm", "medium_power"]));

    if set_brightness {
        // Manage screen backlights.
        catch!(errors, iterate_backlights(Backlight::iter(), &Brightness::set_if_lower_than, 40));

        // Manage keyboard backlights.
        catch!(
            errors,
            iterate_backlights(Leds::iter_keyboards(), &Brightness::set_if_lower_than, 50)
        );
    }

    // Parameters which may cause on certain systems.
    if pci_runtime_pm_support() {
        // Enables PCI device runtime power management.
        catch!(errors, pci_device_runtime_pm(RuntimePowerManagement::On));
    }

    // Set to balanced profile.
    crate::cpufreq::set(Profile::Balanced, 100);

    // Control Intel PState values, if they exist.
    catch!(
        errors,
        pstate_values(
            PStateValues::default()
                .hwp_dynamic_boost(true)
                .min_perf_pct(0)
                .max_perf_pct(100)
                .no_turbo(false)
        )
    );

    if let Some(model_profiles) = ModelProfiles::new() {
        catch!(errors, model_profiles.balanced.set());
    }
}

/// Sets parameters for the performance profile
pub fn performance(errors: &mut Vec<ProfileError>, _on_battery: bool, _set_brightness: bool) {
    // Use the ACPI Platform Profile if the hardware is supported by the kernel.
    if crate::acpi_platform::supported() {
        crate::acpi_platform::performance();
    }

    Dirty::default().set_max_lost_work(15);
    LaptopMode::default().set(b"0");
    RadeonDevice::get_devices().for_each(|dev| dev.set_profiles("high", "performance", "auto"));
    catch!(errors, scsi_host_link_time_pm_policy(&["med_power_with_dipm", "max_performance"]));
    crate::cpufreq::set(Profile::Performance, 100);
    catch!(
        errors,
        pstate_values(
            PStateValues::default()
                .hwp_dynamic_boost(true)
                .min_perf_pct(0)
                .max_perf_pct(100)
                .no_turbo(false)
        )
    );

    if pci_runtime_pm_support() {
        catch!(errors, pci_device_runtime_pm(RuntimePowerManagement::Off));
    }

    if let Some(model_profiles) = ModelProfiles::new() {
        catch!(errors, model_profiles.performance.set());
    }
}

/// Sets parameters for the battery profile
pub fn battery(errors: &mut Vec<ProfileError>, on_battery: bool, set_brightness: bool) {
    // Use the ACPI Platform Profile if the hardware is supported by the kernel.
    if crate::acpi_platform::supported() {
        crate::acpi_platform::battery();
    }

    Dirty::default().set_max_lost_work(15);
    LaptopMode::default().set(b"2");
    RadeonDevice::get_devices().for_each(|dev| dev.set_profiles("low", "battery", "low"));
    catch!(errors, scsi_host_link_time_pm_policy(&["min_power", "min_power"]));
    crate::cpufreq::set(Profile::Battery, 50);

    catch!(
        errors,
        pstate_values(
            PStateValues::default().min_perf_pct(0).max_perf_pct(50).no_turbo(on_battery)
        )
    );

    if set_brightness {
        catch!(errors, iterate_backlights(Backlight::iter(), &Brightness::set_if_lower_than, 10));
        catch!(errors, iterate_backlights(Leds::iter_keyboards(), &Brightness::set_brightness, 0));
    }

    if pci_runtime_pm_support() {
        catch!(errors, pci_device_runtime_pm(RuntimePowerManagement::On));
    }

    if let Some(model_profiles) = ModelProfiles::new() {
        catch!(errors, model_profiles.battery.set());
    }
}

/// Controls the Intel [`PState`] values.
fn pstate_values(values: PStateValues) -> Result<(), PStateError> {
    if let Ok(pstate) = PState::new() {
        pstate.set_values(values)?;
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
                log::warn!("failed to iterate keyboard backlight: {}", why);
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
                .map_err(|why| PciDeviceError::SetRuntimePm(device.id().to_owned(), why))?,
            Err(why) => {
                log::warn!("failed to iterate PCI device: {}", why);
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
                log::warn!("failed to iterate SCSI Host device: {}", why);
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

pub struct ModelProfile {
    pl1:        Option<u8>,
    pl2:        Option<u8>,
    tcc_offset: Option<u8>,
}

impl ModelProfile {
    // TODO pub fn get() -> Result<Self, ModelError> {}

    pub fn set(&self) -> Result<(), ModelError> {
        // Thermald sets pl1 and pl2 on its own, conflicting with system76-power
        let _status = Command::new("systemctl")
            .arg("stop")
            .arg("thermald.service")
            .status()
            .map_err(ModelError::Thermald)?;
        // TODO: check status, allow thermald to be missing

        // Set PL1
        if let Some(pl1) = self.pl1 {
            fs::write(
                "/sys/class/powercap/intel-rapl:0/constraint_0_power_limit_uw",
                format!("{}", u64::from(pl1) * 1_000_000),
            )
            .map_err(ModelError::Pl1)?;
        }

        // Set PL2
        if let Some(pl2) = self.pl2 {
            fs::write(
                "/sys/class/powercap/intel-rapl:0/constraint_1_power_limit_uw",
                format!("{}", u64::from(pl2) * 1_000_000),
            )
            .map_err(ModelError::Pl2)?;
        }

        // Set TCC
        if let Some(tcc_offset) = self.tcc_offset {
            let path = Path::new("/dev/cpu/0/msr");
            if !path.is_file() {
                let status =
                    Command::new("modprobe").arg("msr").status().map_err(ModelError::ModprobeIo)?;
                if !status.success() {
                    return Err(ModelError::ModprobeExitStatus(status));
                }
            }

            let mut file = fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(path)
                .map_err(ModelError::MsrOpen)?;
            file.seek(SeekFrom::Start(0x1A2)).map_err(ModelError::MsrSeek)?;
            let mut data = [0; 8];
            file.read_exact(&mut data).map_err(ModelError::MsrRead)?;
            data[3] = tcc_offset;
            file.write_all(&data).map_err(ModelError::MsrWrite)?;
        }

        Ok(())
    }
}

pub struct ModelProfiles {
    pub balanced:    ModelProfile,
    pub performance: ModelProfile,
    pub battery:     ModelProfile,
}

impl ModelProfiles {
    pub fn new() -> Option<Self> {
        let model_line =
            fs::read_to_string("/sys/class/dmi/id/product_version").unwrap_or_default();
        match model_line.trim() {
            "galp5" => Some(ModelProfiles {
                balanced:    ModelProfile {
                    pl1:        Some(28),
                    pl2:        None,     // galp5 doesn't like setting pl2
                    tcc_offset: Some(12), // 88 C
                },
                performance: ModelProfile {
                    pl1:        Some(40),
                    pl2:        None,    // galp5 doesn't like setting pl2
                    tcc_offset: Some(7), // 93 C
                },
                battery:     ModelProfile {
                    pl1:        Some(12),
                    pl2:        None,     // galp5 doesn't like setting pl2
                    tcc_offset: Some(32), // 68 C
                },
            }),
            "lemp9" => Some(ModelProfiles {
                balanced:    ModelProfile {
                    pl1:        Some(20),
                    pl2:        Some(40), // Upped from 30
                    tcc_offset: Some(12), // 88 C
                },
                performance: ModelProfile {
                    pl1:        Some(30),
                    pl2:        Some(50),
                    tcc_offset: Some(2), // 98 C
                },
                battery:     ModelProfile {
                    pl1:        Some(10),
                    pl2:        Some(30),
                    tcc_offset: Some(32), // 68 C
                },
            }),
            _ => None,
        }
    }
}
