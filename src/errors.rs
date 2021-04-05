use intel_pstate::PStateError;
use std::{io, path::PathBuf, process};

#[derive(Debug, err_derive::Error)]
pub enum ProfileError {
    #[error(display = "failed to set backlight profiles: {}", _0)]
    Backlight(BacklightError),
    #[error(display = "failed to set disk power profiles: {}", _0)]
    DiskPower(DiskPowerError),
    #[error(display = "failed to set model profiles: {}", _0)]
    Model(ModelError),
    #[error(display = "failed to set pci device profiles: {}", _0)]
    PciDevice(PciDeviceError),
    #[error(display = "failed to set pstate profiles: {}", _0)]
    PState(PStateError),
    #[error(display = "failed to set scsi host profiles: {}", _0)]
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

#[derive(Debug, err_derive::Error)]
pub enum BacklightError {
    #[error(display = "failed to set backlight on {}: {}", _0, _1)]
    Set(String, io::Error),
}

#[derive(Debug, err_derive::Error)]
pub enum DiskPowerError {
    #[error(display = "failed to set disk APM level on {:?} to {}: {}", _0, _1, _2)]
    ApmLevel(PathBuf, u8, io::Error),
    #[error(display = "failed to set disk autosuspend delay on {:?} to {}: {}", _0, _1, _2)]
    AutosuspendDelay(PathBuf, i32, io::Error),
}

#[derive(Debug, err_derive::Error)]
pub enum ModelError {
    #[error(display = "failed to stop thermald: {}", _0)]
    Thermald(io::Error),
    #[error(display = "failed to set PL1: {}", _0)]
    Pl1(io::Error),
    #[error(display = "failed to set PL2: {}", _0)]
    Pl2(io::Error),
    #[error(display = "failed to modprobe msr: {}", _0)]
    ModprobeIo(io::Error),
    #[error(display = "failed to modprobe msr: {}", _0)]
    ModprobeExitStatus(process::ExitStatus),
    #[error(display = "failed to open msr: {}", _0)]
    MsrOpen(io::Error),
    #[error(display = "failed to seek msr: {}", _0)]
    MsrSeek(io::Error),
    #[error(display = "failed to read msr: {}", _0)]
    MsrRead(io::Error),
    #[error(display = "failed to write msr: {}", _0)]
    MsrWrite(io::Error),
    #[error(display = "failed to set TCC: {}", _0)]
    Tcc(io::Error),
}

#[derive(Debug, err_derive::Error)]
pub enum PciDeviceError {
    #[error(display = "failed to set PCI device runtime PM on {}: {}", _0, _1)]
    SetRuntimePM(String, io::Error),
}

#[derive(Debug, err_derive::Error)]
pub enum ScsiHostError {
    #[error(display = "failed to set link time power management policy {} on {}: {}", _0, _1, _2)]
    LinkTimePolicy(&'static str, String, io::Error),
}
