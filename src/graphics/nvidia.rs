// Copyright 2018-2021 System76 <info@system76.com>
//
// SPDX-License-Identifier: GPL-3.0-only

use serde::{Deserialize, Serialize};
use std::{fs, io, path};

use super::device::GraphicsDevice;
use super::error::GraphicsDeviceError;

// supported-gpus.json
#[derive(Serialize, Deserialize, Debug)]
pub(super) struct NvidiaDevice {
    pub devid: String,
    pub subdeviceid: Option<String>,
    pub subvendorid: Option<String>,
    pub name: String,
    pub legacybranch: Option<String>,
    pub features: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug)]
struct SupportedGpus {
    chips: Vec<NvidiaDevice>,
}

/// Look up a specific NVIDIA GPU by PCI device ID in the installed
/// `supported-gpus.json` from the nvidia-driver package.
pub(crate) fn get_nvidia_device(id: u16) -> Result<NvidiaDevice, GraphicsDeviceError> {
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

    Err(GraphicsDeviceError::Json(io::Error::new(io::ErrorKind::NotFound, "GPU device not found")))
}

/// Returns true if the first NVIDIA GPU in the list supports runtime power
/// management (the `runtimepm` feature flag in `supported-gpus.json`).
pub(crate) fn gpu_supports_runtimepm(
    nvidia_devices: &[GraphicsDevice],
) -> Result<bool, GraphicsDeviceError> {
    if nvidia_devices.is_empty() {
        return Ok(false);
    }
    let id = nvidia_devices[0].device();
    let dev = get_nvidia_device(id)?;
    log::info!("Device 0x{:04} features: {:?}", id, dev.features);
    Ok(dev.features.contains(&"runtimepm".to_string()))
}
