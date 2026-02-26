// Copyright 2018-2021 System76 <info@system76.com>
//
// SPDX-License-Identifier: GPL-3.0-only

//! Helpers for the runtime (no-reboot) graphics mode switch.
//!
//! The switch is performed in three phases:
//!   1. **Teardown** — stop services, unbind framebuffers, kill device users,
//!      unload modules, unbind and remove PCI devices.
//!   2. **Configuration** — write config files (handled by `config::write_vendor_config`).
//!   3. **Bring-up** — rescan PCI bus, load modules, start services, restart DM.

use std::{fs, path, process, thread, time};

use crate::modprobe;

use super::error::GraphicsDeviceError;
use super::mode::GraphicsMode;
use super::systemd;

// ── Display manager ───────────────────────────────────────────────────────────

/// Detect the first active display manager from a known list.
///
/// Checks `gdm`, `gdm3`, `sddm`, and `lightdm` in that order via
/// `systemctl is-active --quiet`. Returns the unit name of the first active
/// service, or `None` if none are found.
pub(crate) fn detect_display_manager() -> Option<&'static str> {
    const DMS: &[&str] = &["gdm", "gdm3", "sddm", "lightdm"];
    for dm in DMS {
        if systemd::is_active(dm) {
            log::info!("Detected active display manager: {}", dm);
            return Some(dm);
        }
    }
    log::warn!("No active display manager detected");
    None
}

// ── Framebuffer unbind ────────────────────────────────────────────────────────

/// Unbind the kernel framebuffer console from the GPU.
///
/// During boot the kernel uses `efifb` to draw text. When the NVIDIA driver
/// loads it takes over that memory region. If the vtconsole / EFI framebuffer
/// driver is not unbound before `modprobe -r nvidia`, the kernel holds a
/// reference that silently blocks the unload (or causes a panic).
///
/// Failures are logged as warnings rather than propagated as errors because
/// these sysfs paths may not exist on all hardware configurations.
pub(crate) fn unbind_framebuffers() {
    // vtcon1 is normally the GPU framebuffer console (vtcon0 is the CPU/software console).
    let vtcon1 = "/sys/class/vtconsole/vtcon1/bind";
    if path::Path::new(vtcon1).exists() {
        log::info!("Unbinding vtcon1 framebuffer");
        if let Err(e) = fs::write(vtcon1, "0") {
            log::warn!("Failed to unbind vtcon1: {}", e);
        }
    }

    // EFI framebuffer driver holds a reference on systems that booted via UEFI.
    let efifb_device = "/sys/bus/platform/drivers/efi-framebuffer/efi-framebuffer.0";
    let efifb_unbind = "/sys/bus/platform/drivers/efi-framebuffer/unbind";
    if path::Path::new(efifb_device).exists() {
        log::info!("Unbinding EFI framebuffer");
        if let Err(e) = fs::write(efifb_unbind, "efi-framebuffer.0") {
            log::warn!("Failed to unbind efi-framebuffer: {}", e);
        }
    }
}

// ── Kill device users ─────────────────────────────────────────────────────────

/// Forcefully terminate any processes that still hold open file descriptors on
/// `/dev/nvidia*` device nodes.
///
/// Uses `fuser -k` (SIGKILL) rather than parsing `lsof`, which is the
/// POSIX-standard atomic approach for scripts and daemons. A brief sleep after
/// the kill gives the kernel time to clean up the file descriptor table before
/// `modprobe -r` runs.
///
/// Failures (including "no processes found") are non-fatal and only logged.
pub(crate) fn kill_nvidia_device_users() {
    const NVIDIA_DEVICES: &[&str] =
        &["/dev/nvidia0", "/dev/nvidiactl", "/dev/nvidia-modeset", "/dev/nvidia-uvm"];

    let existing: Vec<&str> =
        NVIDIA_DEVICES.iter().copied().filter(|d| path::Path::new(d).exists()).collect();

    if existing.is_empty() {
        log::info!("No /dev/nvidia* devices found; skipping fuser");
        return;
    }

    log::info!("Killing processes holding /dev/nvidia* fds: {:?}", existing);
    // fuser exits non-zero when no processes are found; treat that as success.
    let result = process::Command::new("fuser").arg("-k").args(&existing).status();
    match result {
        Ok(s) => log::info!("fuser exited with {}", s),
        Err(e) => log::warn!("fuser failed to execute: {} (psmisc installed?)", e),
    }

    // Brief pause for processes to die before modprobe -r runs.
    thread::sleep(time::Duration::from_millis(500));
}

// ── Module management ─────────────────────────────────────────────────────────

/// Load the kernel modules required for the given graphics mode.
///
/// Module load order matters: `nvidia` core must be loaded before
/// `nvidia-modeset`, which must be loaded before `nvidia-drm`.
/// For PRIME/DRM modes `nvidia-drm` must be loaded with `modeset=1`.
pub(crate) fn load_modules_for_mode(vendor: GraphicsMode) -> Result<(), GraphicsDeviceError> {
    match vendor {
        GraphicsMode::Integrated => {
            // Nothing to load; the NVIDIA driver stack stays absent.
            log::info!("Integrated mode: no NVIDIA modules to load");
            Ok(())
        }
        GraphicsMode::Compute => {
            // nvidia core only — drm/modeset remain blacklisted so the GPU is
            // accessible for CUDA but does not participate in display.
            modprobe::load("nvidia", &[])
                .map_err(|why| GraphicsDeviceError::ModuleLoad { module: "nvidia", why })
        }
        GraphicsMode::Hybrid | GraphicsMode::Discrete => {
            modprobe::load("nvidia", &[])
                .map_err(|why| GraphicsDeviceError::ModuleLoad { module: "nvidia", why })?;
            modprobe::load("nvidia-modeset", &[])
                .map_err(|why| GraphicsDeviceError::ModuleLoad { module: "nvidia-modeset", why })?;
            // modeset=1 is required for PRIME offload (Hybrid) and for DRM
            // master hand-off (Discrete).
            modprobe::load("nvidia-drm", &["modeset=1"])
                .map_err(|why| GraphicsDeviceError::ModuleLoad { module: "nvidia-drm", why })
        }
    }
}

// ── NVIDIA service management ─────────────────────────────────────────────────

/// Stop the NVIDIA daemon services that hold open references to the driver stack.
///
/// Only the two long-running daemon services are considered:
/// - `nvidia-powerd`       — NVIDIA power management daemon
/// - `nvidia-persistenced` — keeps NVIDIA contexts persistent
///
/// The oneshot hook services (nvidia-suspend, nvidia-hibernate, nvidia-resume,
/// nvidia-suspend-then-hibernate) run and exit immediately; they are never
/// "active" at query time and hold no file-descriptor references, so they are
/// intentionally skipped.
pub(crate) fn stop_nvidia_services() {
    const NVIDIA_DAEMONS: &[&str] = &["nvidia-powerd", "nvidia-persistenced"];

    for svc in NVIDIA_DAEMONS {
        if systemd::is_active(svc) {
            log::info!("Stopping NVIDIA service: {}", svc);
            match systemd::stop(svc) {
                Ok(s) if s.success() => {}
                Ok(s) => log::warn!("systemctl stop {} exited with {} (continuing)", svc, s),
                Err(e) => log::warn!("Failed to stop {}: {} (continuing)", svc, e),
            }
        }
    }
}

/// Start NVIDIA daemon services that are enabled in systemd.
///
/// Called during bring-up after a runtime switch to a GPU-active mode
/// (hybrid, compute, nvidia). We check `systemctl is-enabled` rather than
/// relying on whether the service was running before teardown, because the
/// prior running state may not reflect the user's intended configuration
/// (e.g., after a transition from integrated mode where the service was
/// correctly not running).
pub(crate) fn start_enabled_nvidia_services() {
    const NVIDIA_DAEMONS: &[&str] = &["nvidia-powerd", "nvidia-persistenced"];

    for svc in NVIDIA_DAEMONS {
        if systemd::is_enabled(svc) {
            log::info!("Starting enabled NVIDIA service: {}", svc);
            match systemd::start(svc) {
                Ok(s) if s.success() => log::info!("Started {}", svc),
                Ok(s) => {
                    log::warn!(
                        "systemctl start {} exited with {} — may need manual restart",
                        svc,
                        s
                    )
                }
                Err(e) => log::warn!("Failed to start {}: {}", svc, e),
            }
        }
    }
}

// ── PCI power control ─────────────────────────────────────────────────────────

// HACK: Normally, power/control would be set to "auto" by a udev rule in
// nvidia-drivers, but because of a bug we cannot enable automatic power
// management too early after turning on the GPU. Otherwise, it will turn off
// before the NVIDIA driver finishes initializing, leaving the system in an
// invalid state that will eventually lock up. So defer setting power
// management using a thread.
//
// Ref: pop-os/nvidia-graphics-drivers@f9815ed603bd
// Ref: system76/firmware-open#160
pub(crate) fn sysfs_power_control(pciid: String, mode: GraphicsMode) {
    thread::spawn(move || {
        thread::sleep(time::Duration::from_millis(5000));

        let pm = if mode == GraphicsMode::Discrete { "on\n" } else { "auto\n" };
        log::info!("Setting power management to {}", pm);

        let control = format!("/sys/bus/pci/devices/{}/power/control", pciid);
        let file = fs::OpenOptions::new().create(false).truncate(false).write(true).open(control);

        #[allow(unused_must_use)]
        if let Ok(mut file) = file {
            use std::io::Write;
            file.write_all(pm.as_bytes()).and_then(|()| file.sync_all());
        }
    });
}
