use std::{io, path::PathBuf, process::ExitStatus};
use pstate::PStateError;

#[derive(Debug, Error)]
pub enum ProfileError {
    #[error(display = "failed to set backlight profiles: {}", _0)]
    Backlight(BacklightError),
    #[error(display = "failed to set disk power profiles: {}", _0)]
    DiskPower(DiskPowerError),
    #[error(display = "failed to set pci device profiles: {}", _0)]
    PciDevice(PciDeviceError),
    #[error(display = "failed to set pstate profiles: {}", _0)]
    PState(PStateError),
    #[error(display = "failed to set scsi host profiles: {}", _0)]
    ScsiHost(ScsiHostError),
}

impl From<BacklightError> for ProfileError {
    fn from(why: BacklightError) -> ProfileError {
        ProfileError::Backlight(why)
    }
}

impl From<DiskPowerError> for ProfileError {
    fn from(why: DiskPowerError) -> ProfileError {
        ProfileError::DiskPower(why)
    }
}

impl From<PciDeviceError> for ProfileError {
    fn from(why: PciDeviceError) -> ProfileError {
        ProfileError::PciDevice(why)
    }
}

impl From<PStateError> for ProfileError {
    fn from(why: PStateError) -> ProfileError {
        ProfileError::PState(why)
    }
}

impl From<ScsiHostError> for ProfileError {
    fn from(why: ScsiHostError) -> ProfileError {
        ProfileError::ScsiHost(why)
    }
}

#[derive(Debug, Error)]
pub enum BacklightError {
    #[error(display = "failed to set backlight on {}: {}", _0, _1)]
    Set(String, io::Error)
}

#[derive(Debug, Error)]
pub enum DiskPowerError {
    #[error(display = "failed to set disk APM level on {:?} to {}: {}", _0, _1, _2)]
    ApmLevel(PathBuf, u8, io::Error),
    #[error(display = "failed to set disk autosuspend delay on {:?} to {}: {}", _0, _1, _2)]
    AutosuspendDelay(PathBuf, i32, io::Error)
}

#[derive(Debug, Error)]
pub enum PciDeviceError {
    #[error(display = "failed to set PCI device runtime PM on {}: {}", _0, _1)]
    SetRuntimePM(String, io::Error)
}

#[derive(Debug, Error)]
pub enum ScsiHostError {
    #[error(display = "failed to set link time power management policy {} on {}: {}", _0, _1, _2)]
    LinkTimePolicy(&'static str, String, io::Error)
}

#[derive(Debug, Error)]
pub enum GraphicsDeviceError {
    #[error(display = "failed to execute {} command: {}", cmd, why)]
    Command { cmd: &'static str, why: io::Error },
    #[error(display = "{} in use by {}", func, driver)]
    DeviceInUse { func: String, driver: String },
    #[error(display = "failed to open system76-power modprobe file: {}", _0)]
    ModprobeFileOpen(io::Error),
    #[error(display = "failed to write to system76-power modprobe file: {}", _0)]
    ModprobeFileWrite(io::Error),
    #[error(display = "failed to fetch list of active kernel modules: {}", _0)]
    ModulesFetch(io::Error),
    #[error(display = "does not have switchable graphics")]
    NotSwitchable,
    #[error(display = "PCI driver error on {}: {}", device, why)]
    PciDriver { device: String, why: io::Error },
    #[error(display = "failed to remove PCI device {}: {}", device, why)]
    Remove { device: String, why: io::Error },
    #[error(display = "failed to rescan PCI bus: {}", _0)]
    Rescan(io::Error),
    #[error(display = "failed to unbind {} on PCI driver {}: {}", func, driver, why)]
    Unbind { func: String, driver: String, why: io::Error },
    #[error(display = "update-initramfs failed with {} status", _0)]
    UpdateInitramfs(ExitStatus),
}

#[derive(Debug, Error)]
pub enum SidebandError {
    #[error(display = "failed to open /dev/mem: {}", _0)]
    DevMemOpen(io::Error),
    #[error(display = "failed to map sideband memory: {}", _0)]
    MapFailed(io::Error)
}
