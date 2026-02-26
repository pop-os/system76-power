// Copyright 2018-2021 System76 <info@system76.com>
//
// SPDX-License-Identifier: GPL-3.0-only

mod config;
mod device;
mod error;
mod mode;
mod nvidia;
mod runtime;
mod systemd;

pub(crate) use config::update_initramfs_cmd;
pub use device::GraphicsDevice;
pub use error::GraphicsDeviceError;
pub use mode::GraphicsMode;

use crate::{module::Module, pci::PciBus};
use std::{fs, io};
use sysfs_class::{PciDevice, SysClass};

use config::EXTERNAL_DISPLAY_REQUIRES_NVIDIA;

pub struct Graphics {
    pub bus: PciBus,
    pub amd: Vec<GraphicsDevice>,
    pub intel: Vec<GraphicsDevice>,
    pub nvidia: Vec<GraphicsDevice>,
    pub other: Vec<GraphicsDevice>,
}

impl Graphics {
    pub fn new() -> io::Result<Self> {
        let bus = PciBus::new()?;

        log::info!("Rescanning PCI bus");
        bus.rescan()?;

        let devs = PciDevice::all()?;

        let functions = |parent: &PciDevice| -> Vec<PciDevice> {
            let mut functions = Vec::new();
            if let Some(parent_slot) = parent.id().split('.').next() {
                for func in &devs {
                    if let Some(func_slot) = func.id().split('.').next() {
                        if func_slot == parent_slot {
                            log::info!("{}: Function for {}", func.id(), parent.id());
                            functions.push(func.clone());
                        }
                    }
                }
            }
            functions
        };

        let mut amd = Vec::new();
        let mut intel = Vec::new();
        let mut nvidia = Vec::new();
        let mut other = Vec::new();
        for dev in &devs {
            let c = dev.class()?;
            if (c >> 16) & 0xFF == 0x03 {
                match dev.vendor()? {
                    0x1002 => {
                        log::info!("{}: AMD graphics", dev.id());
                        amd.push(GraphicsDevice::new(
                            dev.id().to_owned(),
                            dev.device()?,
                            functions(dev),
                        ));
                    }
                    0x10DE => {
                        log::info!("{}: NVIDIA graphics", dev.id());
                        nvidia.push(GraphicsDevice::new(
                            dev.id().to_owned(),
                            dev.device()?,
                            functions(dev),
                        ));
                    }
                    0x8086 => {
                        log::info!("{}: Intel graphics", dev.id());
                        intel.push(GraphicsDevice::new(
                            dev.id().to_owned(),
                            dev.device()?,
                            functions(dev),
                        ));
                    }
                    vendor => {
                        log::info!("{}: Other({:X}) graphics", dev.id(), vendor);
                        other.push(GraphicsDevice::new(
                            dev.id().to_owned(),
                            dev.device()?,
                            functions(dev),
                        ));
                    }
                }
            }
        }

        Ok(Self { bus, amd, intel, nvidia, other })
    }

    pub fn is_desktop(&self) -> bool {
        let chassis = fs::read_to_string("/sys/class/dmi/id/chassis_type")
            .map_err(GraphicsDeviceError::SysFs)
            .unwrap_or_default();
        chassis.trim() == "3"
    }

    #[must_use]
    pub fn can_switch(&self) -> bool {
        !self.is_desktop()
            && (!self.nvidia.is_empty() && (!self.intel.is_empty() || !self.amd.is_empty()))
    }

    pub fn get_external_displays_require_dgpu(&self) -> Result<bool, GraphicsDeviceError> {
        self.switchable_or_fail()?;
        let model = fs::read_to_string("/sys/class/dmi/id/product_version")
            .map_err(GraphicsDeviceError::SysFs)?;
        Ok(EXTERNAL_DISPLAY_REQUIRES_NVIDIA.contains(&model.trim()))
    }

    pub fn get_default_graphics(&self) -> Result<GraphicsMode, GraphicsDeviceError> {
        // Models that should default to discrete graphics only
        const DEFAULT_DISCRETE: &[&str] = &["bonw16"];

        self.switchable_or_fail()?;

        let vendor = fs::read_to_string("/sys/class/dmi/id/sys_vendor")
            .map_err(GraphicsDeviceError::SysFs)
            .map(|s| s.trim().to_string())?;

        let product = fs::read_to_string("/sys/class/dmi/id/product_version")
            .map_err(GraphicsDeviceError::SysFs)
            .map(|s| s.trim().to_string())?;

        let runtimepm = match nvidia::gpu_supports_runtimepm(&self.nvidia) {
            Ok(ok) => ok,
            Err(err) => {
                log::warn!("could not determine GPU runtimepm support: {}", err);
                false
            }
        };

        // Only default to hybrid on System76 models
        if vendor != "System76" || DEFAULT_DISCRETE.contains(&product.as_str()) {
            Ok(GraphicsMode::Discrete)
        } else if runtimepm {
            Ok(GraphicsMode::Hybrid)
        } else {
            Ok(GraphicsMode::Integrated)
        }
    }

    pub fn get_vendor(&self) -> Result<GraphicsMode, GraphicsDeviceError> {
        let modules = Module::all().map_err(GraphicsDeviceError::ModulesFetch)?;
        let vendor =
            if modules.iter().any(|module| module.name == "nouveau" || module.name == "nvidia") {
                let mode = match config::get_prime_discrete() {
                    Ok(m) => m,
                    Err(_) => "nvidia".to_string(),
                };

                if mode == "on-demand" {
                    GraphicsMode::Hybrid
                } else if mode == "off" {
                    GraphicsMode::Compute
                } else {
                    GraphicsMode::Discrete
                }
            } else {
                GraphicsMode::Integrated
            };
        Ok(vendor)
    }

    /// Set the graphics mode (boot-time path).
    ///
    /// Writes all configuration files and rebuilds the initramfs. A reboot is
    /// required for the change to take effect. The existing behaviour is fully
    /// preserved.
    pub fn set_vendor(&self, vendor: GraphicsMode) -> Result<(), GraphicsDeviceError> {
        self.switchable_or_fail()?;
        config::write_vendor_config(vendor)?;

        log::info!("Updating initramfs");
        let (cmd, arg) = config::update_initramfs_cmd();
        let status = std::process::Command::new(cmd)
            .arg(arg)
            .status()
            .map_err(|why| GraphicsDeviceError::Command { cmd, why })?;

        if !status.success() {
            return Err(GraphicsDeviceError::UpdateInitramfs(status));
        }

        Ok(())
    }

    /// Switch graphics mode at runtime without a reboot.
    ///
    /// Sequence:
    ///   1.  Guard: external-display models that require the dGPU are rejected.
    ///   2.  Stop the active display manager (gdm/sddm/lightdm).
    ///   3.  Stop NVIDIA daemon services.
    ///   4.  Unbind vtconsole framebuffer and EFI framebuffer (non-fatal).
    ///   5.  Kill lingering `/dev/nvidia*` file-descriptor holders via `fuser -k`.
    ///   6.  Unload NVIDIA kernel modules in reverse dependency order.
    ///   7.  Unbind PCI functions from their kernel drivers (sysfs).
    ///   8.  Remove PCI devices from the bus.
    ///   9.  Write all configuration files (same as `set_vendor`, no initramfs).
    ///  10.  (GPU-active modes only) Rescan PCI bus, load modules, start services.
    ///  11.  Set sysfs power/control (5 s deferred — preserves the existing HACK).
    ///  12.  Start the display manager again.
    ///
    /// The initramfs is NOT rebuilt here. The caller (daemon) schedules that as
    /// an asynchronous background task so that the D-Bus call returns promptly.
    ///
    /// Returns the systemd unit name of the display manager that was restarted.
    pub fn switch_runtime(&mut self, vendor: GraphicsMode) -> Result<String, GraphicsDeviceError> {
        self.switchable_or_fail()?;

        // Models where every external display routes through the dGPU cannot
        // safely tear it down while a session may be active on those outputs.
        if self.get_external_displays_require_dgpu()? {
            log::warn!(
                "runtime graphics switching is not supported on this model: \
                 external displays require the dGPU"
            );
            return Err(GraphicsDeviceError::NotSwitchable);
        }

        // ── Phase 1: Teardown ────────────────────────────────────────────────

        let dm = runtime::detect_display_manager();
        if let Some(dm) = dm {
            log::info!("Stopping display manager: {}", dm);
            let status = systemd::stop(dm).map_err(|why| {
                GraphicsDeviceError::DisplayManagerStop { dm: dm.to_owned(), why }
            })?;
            if !status.success() {
                return Err(GraphicsDeviceError::DisplayManagerStop {
                    dm: dm.to_owned(),
                    why: io::Error::new(
                        io::ErrorKind::Other,
                        format!("systemctl stop {} exited with {}", dm, status),
                    ),
                });
            }
            // Give systemd time to fully stop the DM and release DRM file descriptors.
            std::thread::sleep(std::time::Duration::from_millis(2000));
        }

        // Stop NVIDIA daemon services that hold /dev/nvidia* file descriptors.
        runtime::stop_nvidia_services();

        // Unbind the kernel framebuffer console from the GPU memory.
        runtime::unbind_framebuffers();

        // Forcefully close any remaining /dev/nvidia* file descriptors.
        runtime::kill_nvidia_device_users();

        // Unload in strict reverse dependency order. Treat "not loaded" as a
        // non-error — only genuine failures (e.g. still in use) are propagated.
        for module in &["nvidia-drm", "nvidia-modeset", "nvidia_uvm", "nvidia"] {
            if let Err(why) = crate::modprobe::unload(module) {
                log::warn!("Could not unload {}: {} (continuing)", module, why);
            }
        }

        // Unbind PCI functions from their kernel drivers via sysfs.
        unsafe {
            for dev in &self.nvidia {
                dev.unbind()?;
            }
        }

        // Remove PCI devices from the bus (safe now that no driver is bound).
        unsafe {
            for dev in &self.nvidia {
                dev.remove()?;
            }
        }

        // ── Phase 2: Configuration (no initramfs rebuild) ────────────────────

        config::write_vendor_config(vendor)?;

        // ── Phase 3: Bring-up ────────────────────────────────────────────────

        if vendor == GraphicsMode::Integrated {
            // In integrated mode the NVIDIA GPU must remain removed from the PCI
            // bus. Do NOT rescan — that would bring it back online. Do NOT load
            // modules or start NVIDIA services.
            log::info!("Integrated mode: skipping PCI rescan, module load, and NVIDIA services");
        } else {
            // Refresh self from live PCI state. Graphics::new() already calls
            // bus.rescan() internally, so we do not need a separate rescan call.
            log::info!("Refreshing PCI device inventory");
            *self = Graphics::new().map_err(GraphicsDeviceError::Rescan)?;

            // Load the kernel modules required for the new mode.
            runtime::load_modules_for_mode(vendor)?;

            // Start NVIDIA daemon services that are enabled in systemd.
            runtime::start_enabled_nvidia_services();
        }

        // Set PCI power/control (5 s deferred thread — preserves the existing HACK).
        if let Some(first) = self.nvidia.first() {
            runtime::sysfs_power_control(first.id.clone(), vendor);
        }

        // Start the display manager. A failure here is logged but not fatal —
        // the GPU switch itself succeeded; the user can start the DM manually.
        let dm_name = dm.unwrap_or("gdm");
        log::info!("Starting display manager: {}", dm_name);
        let status = systemd::start(dm_name).map_err(|why| {
            GraphicsDeviceError::DisplayManagerStart { dm: dm_name.to_owned(), why }
        })?;
        if !status.success() {
            log::warn!(
                "systemctl start {} exited with {} — display may need manual restart",
                dm_name,
                status
            );
        }

        Ok(dm_name.to_owned())
    }

    pub fn get_power(&self) -> Result<bool, GraphicsDeviceError> {
        self.switchable_or_fail()?;
        Ok(self.nvidia.iter().any(GraphicsDevice::exists))
    }

    pub fn set_power(&self, power: bool) -> Result<(), GraphicsDeviceError> {
        self.switchable_or_fail()?;

        if power {
            log::info!("Enabling graphics power");
            self.bus.rescan().map_err(GraphicsDeviceError::Rescan)?;
            runtime::sysfs_power_control(self.nvidia[0].id.clone(), self.get_vendor()?);
        } else {
            log::info!("Disabling graphics power");

            // TODO: Don't allow turning off power if nvidia_drm modeset is enabled

            unsafe {
                // Unbind NVIDIA graphics devices and their functions
                let unbinds = self.nvidia.iter().map(|dev| dev.unbind());

                // Remove NVIDIA graphics devices and their functions
                let removes = self.nvidia.iter().map(|dev| dev.remove());

                unbinds.chain(removes).collect::<Result<(), _>>()?;
            }
        }

        Ok(())
    }

    pub fn auto_power(&self) -> Result<(), GraphicsDeviceError> {
        // Only disable power if in integrated mode and the device does not
        // support runtime power management.
        let vendor = self.get_vendor()?;
        let power =
            vendor != GraphicsMode::Integrated || nvidia::gpu_supports_runtimepm(&self.nvidia)?;
        self.set_power(power)
    }

    fn switchable_or_fail(&self) -> Result<(), GraphicsDeviceError> {
        if self.can_switch() { Ok(()) } else { Err(GraphicsDeviceError::NotSwitchable) }
    }
}
