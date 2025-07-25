// Copyright 2018-2021 System76 <info@system76.com>
//
// SPDX-License-Identifier: GPL-3.0-only

use crate::{module::Module, pci::PciBus};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{self, Write},
    path,
    process::{self, ExitStatus},
};
use sysfs_class::{PciDevice, SysClass};

const MODPROBE_PATH: &str = "/etc/modprobe.d/system76-power.conf";

static MODPROBE_NVIDIA: &[u8] = br"# Automatically generated by system76-power
options nvidia-drm modeset=1
";

static MODPROBE_HYBRID: &[u8] = br"# Automatically generated by system76-power
blacklist i2c_nvidia_gpu
alias i2c_nvidia_gpu off
options nvidia NVreg_DynamicPowerManagement=0x02
options nvidia-drm modeset=1
";

static MODPROBE_HYBRID_NO_GC6: &[u8] = br"# Automatically generated by system76-power
blacklist i2c_nvidia_gpu
alias i2c_nvidia_gpu off
options nvidia NVreg_DynamicPowerManagement=0x01
options nvidia-drm modeset=1
";

static MODPROBE_COMPUTE: &[u8] = br"# Automatically generated by system76-power
blacklist i2c_nvidia_gpu
blacklist nvidia-drm
blacklist nvidia-modeset
alias i2c_nvidia_gpu off
alias nvidia-drm off
alias nvidia-modeset off
options nvidia NVreg_DynamicPowerManagement=0x02
";

static MODPROBE_COMPUTE_NO_GC6: &[u8] = br"# Automatically generated by system76-power
blacklist i2c_nvidia_gpu
blacklist nvidia-drm
blacklist nvidia-modeset
alias i2c_nvidia_gpu off
alias nvidia-drm off
alias nvidia-modeset off
options nvidia NVreg_DynamicPowerManagement=0x01
";

static MODPROBE_INTEGRATED: &[u8] = br"# Automatically generated by system76-power
blacklist i2c_nvidia_gpu
blacklist nouveau
blacklist nvidia
blacklist nvidia-drm
blacklist nvidia-modeset
alias i2c_nvidia_gpu off
alias nouveau off
alias nvidia off
alias nvidia-drm off
alias nvidia-modeset off
";

// Systems that cannot use other sleep options
static SYSTEM_SLEEP_EMPTY: &[u8] = b"";

// Systems using S0ix must enable S0ix-based power management.
static SYSTEM_SLEEP_S0IX: &[u8] = br"# Preserve video memory through suspend
options nvidia NVreg_EnableS0ixPowerManagement=1
";

// Systems using S3 had suspend issues with WebRender.
static SYSTEM_SLEEP_S3: &[u8] = br"# Preserve video memory through suspend
options nvidia NVreg_PreserveVideoMemoryAllocations=1
";

const XORG_CONF_PATH: &str = "/usr/share/X11/xorg.conf.d/11-nvidia-discrete.conf";

// The use of hybrid or discrete is determined by the "PrimaryGPU" option.
static XORG_CONF_DISCRETE: &[u8] = br#"# Automatically generated by system76-power
Section "OutputClass"
    Identifier "NVIDIA"
    MatchDriver "nvidia-drm"
    Driver "nvidia"
    Option "PrimaryGPU" "Yes"
    ModulePath "/lib/x86_64-linux-gnu/nvidia/xorg"
EndSection
"#;

const PRIME_DISCRETE_PATH: &str = "/etc/prime-discrete";

const EXTERNAL_DISPLAY_REQUIRES_NVIDIA: &[&str] = &[
    "addw1",
    "addw2",
    "addw3",
    "addw4",
    "addw5",
    "bonw15",
    "bonw15-b",
    "bonw16",
    "gaze14",
    "gaze15",
    "gaze16-3050",
    "gaze16-3060",
    "gaze16-3060-b",
    "gaze17-3050",
    "gaze17-3060-b",
    "gaze20",
    "kudu6",
    "oryp4",
    "oryp4-b",
    "oryp5",
    "oryp6",
    "oryp7",
    "oryp8",
    "oryp9",
    "oryp10",
    "oryp11",
    "oryp12",
    "serw13",
    "serw14",
];

const SYSTEMCTL_CMD: &str = "systemctl";
const UPDATE_INITRAMFS_CMD: &str = "update-initramfs";

#[derive(Debug, thiserror::Error)]
pub enum GraphicsDeviceError {
    #[error("failed to execute {} command: {}", cmd, why)]
    Command { cmd: &'static str, why: io::Error },
    #[error("{} in use by {}", func, driver)]
    DeviceInUse { func: String, driver: String },
    #[error("failed to probe driver features: {}", _0)]
    Json(io::Error),
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

pub struct GraphicsDevice {
    id:        String,
    devid:     u16,
    functions: Vec<PciDevice>,
}

impl GraphicsDevice {
    #[must_use]
    pub fn new(id: String, devid: u16, functions: Vec<PciDevice>) -> Self {
        Self { id, devid, functions }
    }

    #[must_use]
    pub fn exists(&self) -> bool { self.functions.iter().any(|func| func.path().exists()) }

    #[must_use]
    pub const fn device(&self) -> u16 { self.devid }

    pub unsafe fn unbind(&self) -> Result<(), GraphicsDeviceError> {
        for func in &self.functions {
            if func.path().exists() {
                match func.driver() {
                    Ok(driver) => {
                        log::info!("{}: Unbinding {}", driver.id(), func.id());
                        driver.unbind(func).map_err(|why| GraphicsDeviceError::Unbind {
                            driver: driver.id().to_owned(),
                            func: func.id().to_owned(),
                            why,
                        })?;
                    }
                    Err(why) => match why.kind() {
                        io::ErrorKind::NotFound => (),
                        _ => {
                            return Err(GraphicsDeviceError::PciDriver {
                                device: self.id.clone(),
                                why,
                            })
                        }
                    },
                }
            }
        }

        Ok(())
    }

    pub unsafe fn remove(&self) -> Result<(), GraphicsDeviceError> {
        for func in &self.functions {
            if func.path().exists() {
                match func.driver() {
                    Ok(driver) => {
                        log::error!("{}: in use by {}", func.id(), driver.id());
                        return Err(GraphicsDeviceError::DeviceInUse {
                            func:   func.id().to_owned(),
                            driver: driver.id().to_owned(),
                        });
                    }
                    Err(why) => match why.kind() {
                        io::ErrorKind::NotFound => {
                            log::info!("{}: Removing", func.id());
                            func.remove().map_err(|why| GraphicsDeviceError::Remove {
                                device: self.id.clone(),
                                why,
                            })?;
                        }
                        _ => {
                            return Err(GraphicsDeviceError::PciDriver {
                                device: self.id.clone(),
                                why,
                            })
                        }
                    },
                }
            } else {
                log::warn!("{}: Already removed", func.id());
            }
        }

        Ok(())
    }
}

// supported-gpus.json
#[derive(Serialize, Deserialize, Debug)]
struct NvidiaDevice {
    devid:        String,
    subdeviceid:  Option<String>,
    subvendorid:  Option<String>,
    name:         String,
    legacybranch: Option<String>,
    features:     Vec<String>,
}

#[derive(Serialize, Deserialize, Debug)]
struct SupportedGpus {
    chips: Vec<NvidiaDevice>,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum GraphicsMode {
    Integrated,
    Compute,
    Hybrid,
    Discrete,
}

impl From<GraphicsMode> for &'static str {
    fn from(mode: GraphicsMode) -> &'static str {
        match mode {
            GraphicsMode::Integrated => "integrated",
            GraphicsMode::Compute => "compute",
            GraphicsMode::Hybrid => "hybrid",
            GraphicsMode::Discrete => "nvidia",
        }
    }
}

impl From<&str> for GraphicsMode {
    fn from(vendor: &str) -> Self {
        match vendor {
            "nvidia" => GraphicsMode::Discrete,
            "hybrid" => GraphicsMode::Hybrid,
            "compute" => GraphicsMode::Compute,
            _ => GraphicsMode::Integrated,
        }
    }
}

pub struct Graphics {
    pub bus:    PciBus,
    pub amd:    Vec<GraphicsDevice>,
    pub intel:  Vec<GraphicsDevice>,
    pub nvidia: Vec<GraphicsDevice>,
    pub other:  Vec<GraphicsDevice>,
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

    fn get_nvidia_device(id: u16) -> Result<NvidiaDevice, GraphicsDeviceError> {
        let supported_gpus: Vec<path::PathBuf> = fs::read_dir("/usr/share/doc")
            .map_err(|e| {
                GraphicsDeviceError::Json(io::Error::new(io::ErrorKind::InvalidData, e.to_string()))
            })?
            .filter_map(Result::ok)
            .map(|f| f.path())
            .filter(|f| f.to_str().unwrap_or_default().contains("nvidia-driver-"))
            .map(|f| f.join("supported-gpus.json"))
            .filter(|f| f.exists())
            .collect();

        // There should be only 1 driver version installed.
        if supported_gpus.len() != 1 {
            return Err(GraphicsDeviceError::Json(io::Error::new(
                io::ErrorKind::InvalidData,
                "NVIDIA drivers misconfigured",
            )));
        }

        let raw = fs::read_to_string(&supported_gpus[0]).map_err(GraphicsDeviceError::Json)?;
        let gpus: SupportedGpus = serde_json::from_str(&raw).map_err(|e| {
            GraphicsDeviceError::Json(io::Error::new(io::ErrorKind::InvalidData, e.to_string()))
        })?;

        // There may be multiple entries that share the same device ID.
        for dev in gpus.chips {
            let did = dev.devid.trim_start_matches("0x").trim();
            let did = u16::from_str_radix(did, 16).unwrap_or_default();
            if did == id {
                return Ok(dev);
            }
        }

        Err(GraphicsDeviceError::Json(io::Error::new(
            io::ErrorKind::NotFound,
            "GPU device not found",
        )))
    }

    fn gpu_supports_runtimepm(&self) -> Result<bool, GraphicsDeviceError> {
        if self.nvidia.is_empty() {
            Ok(false)
        } else {
            let id = self.nvidia[0].device();
            let dev = Self::get_nvidia_device(id)?;
            log::info!("Device 0x{:04} features: {:?}", id, dev.features);
            Ok(dev.features.contains(&"runtimepm".to_string()))
        }
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

        let runtimepm = match self.gpu_supports_runtimepm() {
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

    fn get_prime_discrete() -> Result<String, GraphicsDeviceError> {
        fs::read_to_string(PRIME_DISCRETE_PATH)
            .map_err(GraphicsDeviceError::PrimeModeRead)
            .map(|mode| mode.trim().to_owned())
    }

    fn set_prime_discrete(mode: &str) -> Result<(), GraphicsDeviceError> {
        fs::write(PRIME_DISCRETE_PATH, mode).map_err(GraphicsDeviceError::PrimeModeWrite)
    }

    pub fn get_vendor(&self) -> Result<GraphicsMode, GraphicsDeviceError> {
        let modules = Module::all().map_err(GraphicsDeviceError::ModulesFetch)?;
        let vendor =
            if modules.iter().any(|module| module.name == "nouveau" || module.name == "nvidia") {
                let mode = match Self::get_prime_discrete() {
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

    pub fn set_vendor(&self, vendor: GraphicsMode) -> Result<(), GraphicsDeviceError> {
        self.switchable_or_fail()?;

        let mode = match vendor {
            GraphicsMode::Hybrid => "on-demand\n",
            GraphicsMode::Discrete => "on\n",
            _ => "off\n",
        };

        log::info!("Setting {} to {}", PRIME_DISCRETE_PATH, mode);
        Self::set_prime_discrete(mode)?;

        let bonw15_hack = {
            let dmi_vendor = fs::read_to_string("/sys/class/dmi/id/sys_vendor").unwrap_or_default();
            let dmi_model =
                fs::read_to_string("/sys/class/dmi/id/product_version").unwrap_or_default();
            match (dmi_vendor.trim(), dmi_model.trim()) {
                ("System76", "bonw15") => true,
                ("System76", "bonw15-b") => true,
                _ => false,
            }
        };

        {
            log::info!("Creating {}", MODPROBE_PATH);

            let mut file = fs::OpenOptions::new()
                .create(true)
                .truncate(true)
                .write(true)
                .open(MODPROBE_PATH)
                .map_err(GraphicsDeviceError::ModprobeFileOpen)?;

            let text = match vendor {
                GraphicsMode::Integrated => MODPROBE_INTEGRATED,
                GraphicsMode::Compute => {
                    if bonw15_hack {
                        MODPROBE_COMPUTE_NO_GC6
                    } else {
                        MODPROBE_COMPUTE
                    }
                }
                GraphicsMode::Hybrid => {
                    if bonw15_hack {
                        MODPROBE_HYBRID_NO_GC6
                    } else {
                        MODPROBE_HYBRID
                    }
                }
                GraphicsMode::Discrete => MODPROBE_NVIDIA,
            };

            file.write_all(text)
                .and_then(|()| file.sync_all())
                .map_err(GraphicsDeviceError::ModprobeFileWrite)?;

            // Power management must be configured depending on if the system
            // uses S0ix or S3 for suspend.
            if vendor != GraphicsMode::Integrated {
                // XXX: Better way to check?
                let s0ix = fs::read_to_string("/sys/power/mem_sleep")
                    .unwrap_or_default()
                    .contains("[s2idle]");

                let (sleep, action) = if bonw15_hack {
                    (SYSTEM_SLEEP_EMPTY, "disable")
                } else if s0ix {
                    (SYSTEM_SLEEP_S0IX, "enable")
                } else {
                    (SYSTEM_SLEEP_S3, "enable")
                };

                // We should also check if the GPU supports Video Memory Self
                // Refresh, but that requires already being in hybrid or nvidia
                // graphics mode. In compute mode, it just reports '?'.

                file.write_all(sleep)
                    .and_then(|()| file.sync_all())
                    .map_err(GraphicsDeviceError::ModprobeFileWrite)?;

                for service in
                    &["nvidia-hibernate.service", "nvidia-resume.service", "nvidia-suspend.service"]
                {
                    let status = process::Command::new(SYSTEMCTL_CMD)
                        .arg(action)
                        .arg(service)
                        .status()
                        .map_err(|why| GraphicsDeviceError::Command { cmd: SYSTEMCTL_CMD, why })?;

                    if !status.success() {
                        // Error is ignored in case this service is removed
                        log::warn!(
                            "systemctl {} {}: failed with {} (not an error if service does not \
                             exist!)",
                            action,
                            service,
                            status
                        );
                    }
                }
            }
        }

        // Configure X server
        if vendor == GraphicsMode::Discrete {
            let mut file = fs::OpenOptions::new()
                .create(true)
                .truncate(true)
                .write(true)
                .open(XORG_CONF_PATH)
                .map_err(GraphicsDeviceError::XserverConf)?;

            file.write_all(XORG_CONF_DISCRETE)
                .and_then(|()| file.sync_all())
                .map_err(GraphicsDeviceError::XserverConf)?;
        } else if path::Path::new(XORG_CONF_PATH).exists() {
            fs::remove_file(XORG_CONF_PATH).map_err(GraphicsDeviceError::XserverConf)?;
        }

        let action = if vendor == GraphicsMode::Discrete {
            log::info!("Enabling nvidia-fallback.service");
            "enable"
        } else {
            log::info!("Disabling nvidia-fallback.service");
            "disable"
        };

        let status = process::Command::new(SYSTEMCTL_CMD)
            .arg(action)
            .arg("nvidia-fallback.service")
            .status()
            .map_err(|why| GraphicsDeviceError::Command { cmd: SYSTEMCTL_CMD, why })?;

        if !status.success() {
            // Error is ignored in case this service is removed
            log::warn!(
                "systemctl: failed with {} (not an error if service does not exist!)",
                status
            );
        }

        log::info!("Updating initramfs");
        let status = process::Command::new(UPDATE_INITRAMFS_CMD)
            .arg("-u")
            .status()
            .map_err(|why| GraphicsDeviceError::Command { cmd: UPDATE_INITRAMFS_CMD, why })?;

        if !status.success() {
            return Err(GraphicsDeviceError::UpdateInitramfs(status));
        }

        Ok(())
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

            sysfs_power_control(self.nvidia[0].id.clone(), self.get_vendor()?);
        } else {
            log::info!("Disabling graphics power");

            // TODO: Don't allow turning off power if nvidia_drm modeset is enabled

            unsafe {
                // Unbind NVIDIA graphics devices and their functions
                let unbinds = self.nvidia.iter().map(|dev| dev.unbind());

                // Remove NVIDIA graphics devices and their functions
                let removes = self.nvidia.iter().map(|dev| dev.remove());

                unbinds.chain(removes).collect::<Result<_, _>>()?;
            }
        }

        Ok(())
    }

    pub fn auto_power(&self) -> Result<(), GraphicsDeviceError> {
        // Only disable power if in integrated mode and the device does not
        // support runtime power management.
        let vendor = self.get_vendor()?;
        let power = vendor != GraphicsMode::Integrated || self.gpu_supports_runtimepm()?;

        self.set_power(power)
    }

    fn switchable_or_fail(&self) -> Result<(), GraphicsDeviceError> {
        if self.can_switch() {
            Ok(())
        } else {
            Err(GraphicsDeviceError::NotSwitchable)
        }
    }
}

// HACK
// Normally, power/control would be set to "auto" by a udev rule in nvidia-drivers, but because
// of a bug we cannot enable automatic power management too early after turning on the GPU.
// Otherwise it will turn off before the NVIDIA driver finishes initializing, leaving the
// system in an invalid state that will eventually lock up. So defer setting power management
// using a thread.
//
// Ref: pop-os/nvidia-graphics-drivers@f9815ed603bd
// Ref: system76/firmware-open#160
fn sysfs_power_control(pciid: String, mode: GraphicsMode) {
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(5000));

        let pm = if mode == GraphicsMode::Discrete { "on\n" } else { "auto\n" };
        log::info!("Setting power management to {}", pm);

        let control = format!("/sys/bus/pci/devices/{}/power/control", pciid);
        let file = fs::OpenOptions::new().create(false).truncate(false).write(true).open(control);

        #[allow(unused_must_use)]
        if let Ok(mut file) = file {
            file.write_all(pm.as_bytes()).and_then(|()| file.sync_all());
        }
    });
}
