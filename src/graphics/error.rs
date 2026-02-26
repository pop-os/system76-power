// Copyright 2018-2021 System76 <info@system76.com>
//
// SPDX-License-Identifier: GPL-3.0-only

use std::{io, process::ExitStatus};

#[derive(Debug, thiserror::Error)]
pub enum GraphicsDeviceError {
    #[error("failed to execute {} command: {}", cmd, why)]
    Command { cmd: &'static str, why: io::Error },
    #[error("{} in use by {}", func, driver)]
    DeviceInUse { func: String, driver: String },
    #[error("failed to stop display manager {}: {}", dm, why)]
    DisplayManagerStop { dm: String, why: io::Error },
    #[error("failed to start display manager {}: {}", dm, why)]
    DisplayManagerStart { dm: String, why: io::Error },
    #[error("failed to probe driver features: {}", _0)]
    Json(io::Error),
    #[error("failed to load kernel module {}: {}", module, why)]
    ModuleLoad { module: &'static str, why: io::Error },
    #[error("failed to unload kernel module {}: {}", module, why)]
    ModuleUnload { module: &'static str, why: io::Error },
    #[error("failed to open system76-power modprobe file: {}", _0)]
    ModprobeFileOpen(io::Error),
    #[error("failed to write to system76-power modprobe file: {}", _0)]
    ModprobeFileWrite(io::Error),
    #[error("failed to fetch list of active kernel modules: {}", _0)]
    ModulesFetch(io::Error),
    #[error("does not have switchable graphics")]
    NotSwitchable,
    #[error("PCI driver error on {}: {}", device, why)]
    PciDriver { device: String, why: io::Error },
    #[error("failed to get PRIME value: {}", _0)]
    PrimeModeRead(io::Error),
    #[error("failed to set PRIME value: {}", _0)]
    PrimeModeWrite(io::Error),
    #[error("failed to remove PCI device {}: {}", device, why)]
    Remove { device: String, why: io::Error },
    #[error("failed to rescan PCI bus: {}", _0)]
    Rescan(io::Error),
    #[error("failed to access sysfs info: {}", _0)]
    SysFs(io::Error),
    #[error("failed to unbind {} on PCI driver {}: {}", func, driver, why)]
    Unbind { func: String, driver: String, why: io::Error },
    #[error("update-initramfs failed with {} status", _0)]
    UpdateInitramfs(ExitStatus),
    #[error("failed to access Xserver config: {}", _0)]
    XserverConf(io::Error),
}
