// Copyright 2018-2021 System76 <info@system76.com>
//
// SPDX-License-Identifier: GPL-3.0-only

use intel_pstate::PStateError;
use std::{io, path::PathBuf, process};

#[derive(Debug, thiserror::Error)]
pub enum ProfileError {
    #[error("failed to set backlight profiles: {}", _0)]
    Backlight(BacklightError),
    #[error("failed to set disk power profiles: {}", _0)]
    DiskPower(DiskPowerError),
    #[error("failed to set model profiles: {}", _0)]
    Model(ModelError),
    #[error("failed to set pci device profiles: {}", _0)]
    PciDevice(PciDeviceError),
    #[error("failed to set pstate profiles: {}", _0)]
    PState(PStateError),
    #[error("failed to set scsi host profiles: {}", _0)]
    ScsiHost(ScsiHostError),
}

impl From<BacklightError> for ProfileError {
    fn from(why: BacklightError) -> ProfileError { ProfileError::Backlight(why) }
}

impl From<DiskPowerError> for ProfileError {
    fn from(why: DiskPowerError) -> ProfileError { ProfileError::DiskPower(why) }
}

impl From<ModelError> for ProfileError {
    fn from(why: ModelError) -> ProfileError { ProfileError::Model(why) }
}

impl From<PciDeviceError> for ProfileError {
    fn from(why: PciDeviceError) -> ProfileError { ProfileError::PciDevice(why) }
}

impl From<PStateError> for ProfileError {
    fn from(why: PStateError) -> ProfileError { ProfileError::PState(why) }
}

impl From<ScsiHostError> for ProfileError {
    fn from(why: ScsiHostError) -> ProfileError { ProfileError::ScsiHost(why) }
}

#[derive(Debug, thiserror::Error)]
pub enum BacklightError {
    #[error("failed to set backlight on {}: {}", _0, _1)]
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
