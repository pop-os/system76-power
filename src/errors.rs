// Copyright 2018-2021 System76 <info@system76.com>
//
// SPDX-License-Identifier: GPL-3.0-only

use intel_pstate::PStateError;
use std::{io, path::PathBuf, process};

#[derive(Debug, thiserror::Error)]
pub enum ProfileError {
    #[error("failed to set backlight profiles: {0}")]
    Backlight(#[from] BacklightError),
    #[error("failed to set disk power profiles: {0}")]
    DiskPower(#[from] DiskPowerError),
    #[error("failed to set model profiles: {0}")]
    Model(#[from] ModelError),
    #[error("failed to set pci device profiles: {0}")]
    PciDevice(#[from] PciDeviceError),
    #[error("failed to set pstate profiles: {0}")]
    PState(#[from] PStateError),
    #[error("failed to set scsi host profiles: {0}")]
    ScsiHost(#[from] ScsiHostError),
}

#[derive(Debug, thiserror::Error)]
pub enum BacklightError {
    #[error("failed to set backlight on {0}: {1}")]
    Set(String, io::Error),
}

#[derive(Debug, thiserror::Error)]
pub enum DiskPowerError {
    #[error("failed to set disk APM level on {:?} to {}: {}", _0, _1, _2)]
    ApmLevel(PathBuf, u8, io::Error),
    #[error("failed to set disk autosuspend delay on {:?} to {}: {}", _0, _1, _2)]
    AutosuspendDelay(PathBuf, i32, io::Error),
}

#[derive(Debug, thiserror::Error)]
pub enum ModelError {
    #[error("failed to stop thermald: {}", _0)]
    Thermald(io::Error),
    #[error("failed to set PL1: {}", _0)]
    Pl1(io::Error),
    #[error("failed to set PL2: {}", _0)]
    Pl2(io::Error),
    #[error("failed to modprobe msr: {}", _0)]
    ModprobeIo(io::Error),
    #[error("failed to modprobe msr: {}", _0)]
    ModprobeExitStatus(process::ExitStatus),
    #[error("failed to open msr: {}", _0)]
    MsrOpen(io::Error),
    #[error("failed to seek msr: {}", _0)]
    MsrSeek(io::Error),
    #[error("failed to read msr: {}", _0)]
    MsrRead(io::Error),
    #[error("failed to write msr: {}", _0)]
    MsrWrite(io::Error),
    #[error("failed to set TCC: {}", _0)]
    Tcc(io::Error),
}

#[derive(Debug, thiserror::Error)]
pub enum PciDeviceError {
    #[error("failed to set PCI device runtime PM on {}: {}", _0, _1)]
    SetRuntimePm(String, io::Error),
}

#[derive(Debug, thiserror::Error)]
pub enum ScsiHostError {
    #[error("failed to set link time power management policy {} on {}: {}", _0, _1, _2)]
    LinkTimePolicy(&'static str, String, io::Error),
}
