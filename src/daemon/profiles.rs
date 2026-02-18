// Copyright 2018-2021 System76 <info@system76.com>
//
// SPDX-License-Identifier: GPL-3.0-only

use super::pci_runtime_pm_support;
use crate::errors::RyzenAdjError;
use crate::{
    errors::{BacklightError, ModelError, PciDeviceError, ProfileError, ScsiHostError},
    kernel_parameters::{DeviceList, Dirty, KernelParameter, LaptopMode, PcieAspm},
    radeon::RadeonDevice,
    sys_devices, Profile,
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
pub fn balanced(errors: &mut Vec<ProfileError>, set_brightness: bool) {
    log::info!("=== Applying BALANCED profile ===");

    // Use the ACPI Platform Profile if the hardware is supported by the kernel.
    if crate::acpi_platform::supported() {
        log::info!("Setting ACPI platform profile to balanced");
        crate::acpi_platform::balanced();
    }

    // The dirty kernel parameter controls how often the OS will sync data to disks. The less
    // frequently this occurs, the more power can be saved, yet the higher the risk of sudden
    // power loss causing loss of data. 15s is a reasonable number.
    log::debug!("Setting dirty writeback to 15 seconds");
    Dirty::default().set_max_lost_work(15);

    // Enables the laptop mode feature in the kernel, which allows mechanical drives to spin down
    // when inactive.
    log::debug!("Setting laptop mode to 2");
    LaptopMode.set(b"2");

    // Sets radeon power profiles for AMD graphics.
    log::info!("Configuring AMD GPU devices for balanced profile");
    let gpu_count = RadeonDevice::get_devices().count();
    log::info!("Found {} AMD GPU device(s)", gpu_count);
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
    log::info!("Configuring CPU for balanced profile (100% frequency cap)");
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

    // Default PCIe ASPM for balanced mode
    PcieAspm.set(b"default");

    // Enable I2C runtime PM for moderate power saving
    for device in sys_devices::i2c::devices() {
        device.set_runtime_pm(RuntimePowerManagement::On);
    }

    if let Some(model_profiles) = ModelProfiles::new() {
        catch!(errors, model_profiles.balanced.set());
    }

    catch!(errors, set_ryzen_limits(25_000, 35_000, 20_000, 85));

    log::info!("=== BALANCED profile applied successfully ===");
}

/// Sets parameters for the performance profile
pub fn performance(errors: &mut Vec<ProfileError>, _set_brightness: bool) {
    log::info!("=== Applying PERFORMANCE profile ===");

    // Use the ACPI Platform Profile if the hardware is supported by the kernel.
    if crate::acpi_platform::supported() {
        log::info!("Setting ACPI platform profile to performance");
        crate::acpi_platform::performance();
    }

    // Faster dirty writeback for performance mode
    log::debug!("Setting dirty writeback to 10 seconds");
    Dirty::default().set_max_lost_work(10);
    LaptopMode.set(b"0");

    log::info!("Configuring AMD GPU devices for performance profile");
    let gpu_count = RadeonDevice::get_devices().count();
    log::info!("Found {} AMD GPU device(s)", gpu_count);
    RadeonDevice::get_devices().for_each(|dev| dev.set_profiles("high", "performance", "auto"));

    catch!(errors, scsi_host_link_time_pm_policy(&["med_power_with_dipm", "max_performance"]));

    log::info!("Configuring CPU for performance profile (100% frequency cap)");
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

    // Default PCIe ASPM for performance (safe, minimal power management)
    PcieAspm.set(b"default");

    // Disable I2C runtime PM for lowest latency
    for device in sys_devices::i2c::devices() {
        device.set_runtime_pm(RuntimePowerManagement::Off);
    }

    if let Some(model_profiles) = ModelProfiles::new() {
        catch!(errors, model_profiles.performance.set());
    }

    catch!(errors, set_ryzen_max_performance());

    log::info!("=== PERFORMANCE profile applied successfully ===");
}

/// Sets parameters for the battery profile
pub fn battery(errors: &mut Vec<ProfileError>, set_brightness: bool) {
    log::info!("=== Applying BATTERY profile ===");

    // Use the ACPI Platform Profile if the hardware is supported by the kernel.
    if crate::acpi_platform::supported() {
        log::info!("Setting ACPI platform profile to battery");
        crate::acpi_platform::battery();
    }

    // Increase dirty writeback interval for better battery life
    log::debug!("Setting dirty writeback to 30 seconds");
    Dirty::default().set_max_lost_work(30);
    LaptopMode.set(b"2");

    log::info!("Configuring AMD GPU devices for battery profile");
    let gpu_count = RadeonDevice::get_devices().count();
    log::info!("Found {} AMD GPU device(s)", gpu_count);
    RadeonDevice::get_devices().for_each(|dev| dev.set_profiles("low", "battery", "low"));

    catch!(errors, scsi_host_link_time_pm_policy(&["min_power", "min_power"]));

    // Set CPU frequency cap to 60% (~2.7GHz for Ryzen, allows good performance without boost)
    log::info!("Configuring CPU for battery profile (60% frequency cap)");
    crate::cpufreq::set(Profile::Battery, 60);

    catch!(
        errors,
        pstate_values(PStateValues::default().min_perf_pct(0).max_perf_pct(25).no_turbo(true))
    );

    if set_brightness {
        catch!(errors, iterate_backlights(Backlight::iter(), &Brightness::set_if_lower_than, 10));
        catch!(errors, iterate_backlights(Leds::iter_keyboards(), &Brightness::set_brightness, 0));
    }

    if pci_runtime_pm_support() {
        catch!(errors, pci_device_runtime_pm(RuntimePowerManagement::On));
    }

    // Enable aggressive PCIe ASPM for battery savings
    PcieAspm.set(b"powersupersave");

    // Enable I2C device runtime power management (touchpad, sensors)
    for device in sys_devices::i2c::devices() {
        device.set_runtime_pm(RuntimePowerManagement::On);
    }

    if let Some(model_profiles) = ModelProfiles::new() {
        catch!(errors, model_profiles.battery.set());
    }

    catch!(errors, set_ryzen_limits(12_000, 18_000, 10_000, 60));

    log::info!("=== BATTERY profile applied successfully ===");
}

fn set_ryzen_limits(
    stapm_limit: u32,
    fast_limit: u32,
    slow_limit: u32,
    tctl_temp: u32,
) -> Result<(), RyzenAdjError> {
    let stapm_limit_str = stapm_limit.to_string();
    let fast_limit_str = fast_limit.to_string();
    let slow_limit_str = slow_limit.to_string();
    let tctl_temp_str = tctl_temp.to_string();

    let output = Command::new("ryzenadj")
        .arg("--stapm-limit=".to_owned() + &stapm_limit_str)
        .arg("--fast-limit=".to_owned() + &fast_limit_str)
        .arg("--slow-limit=".to_owned() + &slow_limit_str)
        .arg("--tctl-temp=".to_owned() + &tctl_temp_str)
        .output()
        .map_err(RyzenAdjError::CmdError)?;

    if output.status.success() {
        log::info!(
            "Successfully set Ryzen limits: STAPM={}mW, Fast={}mW, Slow={}mW, Tctl={}°C",
            stapm_limit,
            fast_limit,
            slow_limit,
            tctl_temp
        );
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        log::warn!("Error setting Ryzen limits: {}", stderr);
        Err(RyzenAdjError::CmdError(io::Error::new(
            io::ErrorKind::Other,
            "ryzenadj command failed",
        )))
    }
}

fn set_ryzen_max_performance() -> Result<(), RyzenAdjError> {
    let output = Command::new("ryzenadj")
        .arg("--max-performance".to_owned())
        .output()
        .map_err(RyzenAdjError::CmdError)?;

    if output.status.success() {
        log::info!("Successfully set Ryzen to max performance.");
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        eprintln!("Error setting Ryzen to max performance: {}", stderr);
        Err(RyzenAdjError::CmdError(io::Error::new(
            io::ErrorKind::Other,
            "ryzenadj command failed",
        )))
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
    pl1: Option<u8>,
    pl2: Option<u8>,
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
    pub balanced: ModelProfile,
    pub performance: ModelProfile,
    pub battery: ModelProfile,
}

impl ModelProfiles {
    pub fn new() -> Option<Self> {
        let model_line =
            fs::read_to_string("/sys/class/dmi/id/product_version").unwrap_or_default();
        match model_line.trim() {
            "galp5" => Some(Self {
                balanced: ModelProfile {
                    pl1: Some(28),
                    pl2: None,            // galp5 doesn't like setting pl2
                    tcc_offset: Some(12), // 88 C
                },
                performance: ModelProfile {
                    pl1: Some(40),
                    pl2: None,           // galp5 doesn't like setting pl2
                    tcc_offset: Some(7), // 93 C
                },
                battery: ModelProfile {
                    pl1: Some(12),
                    pl2: None,            // galp5 doesn't like setting pl2
                    tcc_offset: Some(32), // 68 C
                },
            }),
            "lemp9" => Some(Self {
                balanced: ModelProfile {
                    pl1: Some(20),
                    pl2: Some(40),        // Upped from 30
                    tcc_offset: Some(12), // 88 C
                },
                performance: ModelProfile {
                    pl1: Some(30),
                    pl2: Some(50),
                    tcc_offset: Some(2), // 98 C
                },
                battery: ModelProfile {
                    pl1: Some(10),
                    pl2: Some(30),
                    tcc_offset: Some(32), // 68 C
                },
            }),
            _ => None,
        }
    }
}
