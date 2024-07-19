// Copyright 2018-2021 System76 <info@system76.com>
//
// SPDX-License-Identifier: GPL-3.0-only

pub mod mux;
pub mod sideband;

use sideband::{Sideband, SidebandError, PCR_BASE_ADDRESS};
use std::{
    fs,
    io::{self, Read, Seek},
};

#[derive(Debug, thiserror::Error)]
pub enum HotPlugDetectError {
    #[error("failed to read DMI product version: {}", _0)]
    ProductVersion(io::Error),
    #[error("error constructing sideband: {}", _0)]
    Sideband(SidebandError),
    #[error("{} variant '{}' does not support hotplug detection", model, variant)]
    VariantUnsupported { model: &'static str, variant: String },
    #[error("model '{}' does not support hotplug detection", _0)]
    ModelUnsupported(String),
    #[error("failed to read {}'s subsystem device: {}", model, why)]
    SubsystemDevice { model: &'static str, why: io::Error },
    #[error("failed to open /dev/mem: {}", _0)]
    DevMemAccess(io::Error),
}

impl From<SidebandError> for HotPlugDetectError {
    fn from(err: SidebandError) -> Self { Self::Sideband(err) }
}

pub trait Detect {
    unsafe fn detect(&mut self) -> [bool; 4];
}

const AMD_FCH_GPIO_CONTROL_BASE: u32 = 0xFED8_1500;

struct Amd {
    mem:   fs::File,
    gpios: Vec<u32>,
}

impl Amd {
    unsafe fn new(gpios: Vec<u32>) -> Result<Self, HotPlugDetectError> {
        let mem = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open("/dev/mem")
            .map_err(HotPlugDetectError::DevMemAccess)?;

        Ok(Self { mem, gpios })
    }
}

impl Detect for Amd {
    unsafe fn detect(&mut self) -> [bool; 4] {
        let mut hpd = [false; 4];

        for (i, offset) in self.gpios.iter().enumerate() {
            let control_offset = AMD_FCH_GPIO_CONTROL_BASE + offset * 4;
            if self.mem.seek(io::SeekFrom::Start(u64::from(control_offset))).is_err() {
                return hpd;
            }

            let mut control = [0; 4];
            if self.mem.read(&mut control).is_err() {
                return hpd;
            }

            let value = u32::from_ne_bytes(control);
            hpd[i] = value & (1 << 16) == (1 << 16);
        }

        hpd
    }
}

const NO_PIN: u8 = 0xFF;

pub struct Intel {
    sideband: Sideband,
    port:     u8,
    pins:     [u8; 4],
}

impl Detect for Intel {
    unsafe fn detect(&mut self) -> [bool; 4] {
        let mut hpd = [false; 4];
        for (i, &pin) in self.pins.iter().enumerate() {
            if pin != NO_PIN {
                let data = self.sideband.gpio(self.port, pin);
                hpd[i] = data & 2 == 2;
            }
        }
        hpd
    }
}

enum Integrated {
    Amd(Amd),
    Intel(Intel),
}

pub struct HotPlugDetect {
    integrated: Integrated,
}

impl HotPlugDetect {
    /// # Errors
    ///
    /// - If `/sys/class/dmi/id/product_version` cannot be read
    /// - If `Sideband::new` fails
    #[allow(clippy::too_many_lines)]
    pub unsafe fn new(nvidia_device: Option<String>) -> Result<Self, HotPlugDetectError> {
        let model = fs::read_to_string("/sys/class/dmi/id/product_version")
            .map_err(HotPlugDetectError::ProductVersion)?;

        match model.trim() {
            "addw1" | "addw2" => Ok(Self {
                integrated: Integrated::Intel(Intel {
                    sideband: Sideband::new(PCR_BASE_ADDRESS)?,
                    port:     0x6A,
                    pins:     [
                        0x28, // USB-C on rear
                        0x2a, // HDMI
                        0x2c, // Mini DisplayPort
                        0x2e, // USB-C on right
                    ],
                }),
            }),
            "addw3" => Ok(Self {
                integrated: Integrated::Intel(Intel {
                    sideband: Sideband::new(0xE000_0000)?,
                    port:     0x6E,
                    pins:     [
                        0x04,   // Mini DisplayPort
                        0x08,   // HDMI
                        NO_PIN, // TODO: USB-C?
                        NO_PIN, // Not connected
                    ],
                }),
            }),
            "addw4" => Ok(Self {
                integrated: Integrated::Intel(Intel {
                    sideband: Sideband::new(0xE000_0000)?,
                    port:     0x6E,
                    pins:     [
                        0x02,   // USB-C
                        0x04,   // HDMI
                        NO_PIN, // NC
                        NO_PIN, // NC
                    ],
                }),
            }),
            "bonw15" | "bonw15-b" => Ok(Self {
                integrated: Integrated::Intel(Intel {
                    sideband: Sideband::new(0xE000_0000)?,
                    port:     0x6E,
                    pins:     [
                        0x02,   // Mini DisplayPort
                        0x06,   // HDMI
                        NO_PIN, // TODO: USB-C?
                        NO_PIN, // Not connected
                    ],
                }),
            }),
            "gaze14" => {
                let variant =
                    fs::read_to_string("/sys/bus/pci/devices/0000:00:00.0/subsystem_device")
                        .map_err(|why| HotPlugDetectError::SubsystemDevice {
                            model: "gaze14",
                            why,
                        })?;

                match variant.trim() {
                    // NVIDIA GTX 1660 Ti
                    "0x8550" | "0x8551" => Ok(Self {
                        integrated: Integrated::Intel(Intel {
                            sideband: Sideband::new(PCR_BASE_ADDRESS)?,
                            port:     0x6A,
                            pins:     [
                                0x2a,   // HDMI
                                NO_PIN, // Mini DisplayPort (0x2c) is connected to Intel graphics
                                0x2e,   // USB-C
                                NO_PIN, // Not Connected
                            ],
                        }),
                    }),
                    // NVIDIA GTX 1650
                    "0x8560" | "0x8561" => Ok(Self {
                        integrated: Integrated::Intel(Intel {
                            sideband: Sideband::new(PCR_BASE_ADDRESS)?,
                            port:     0x6A,
                            pins:     [
                                NO_PIN, // HDMI (0x2a) is connected to Intel graphics
                                0x2e,   // Mini DisplayPort
                                NO_PIN, // Not Connected
                                NO_PIN, // Not Connected
                            ],
                        }),
                    }),
                    other => Err(HotPlugDetectError::VariantUnsupported {
                        model:   "gaze14",
                        variant: other.into(),
                    }),
                }
            }
            "gaze15" => {
                let variant = nvidia_device.unwrap_or_else(|| "unknown".to_string());

                match variant.trim() {
                    // NVIDIA GTX 1660 Ti
                    "0x2191" => Ok(Self {
                        integrated: Integrated::Intel(Intel {
                            sideband: Sideband::new(PCR_BASE_ADDRESS)?,
                            port:     0x6A,
                            pins:     [
                                0x2a,   // HDMI
                                NO_PIN, // Mini DisplayPort (0x2c) is connected to Intel graphics
                                0x2e,   // USB-C
                                NO_PIN, // Not Connected
                            ],
                        }),
                    }),
                    // NVIDIA GTX 1650, 1650 Ti
                    "0x1f99" | "0x1f95" => Ok(Self {
                        integrated: Integrated::Intel(Intel {
                            sideband: Sideband::new(PCR_BASE_ADDRESS)?,
                            port:     0x6A,
                            pins:     [
                                NO_PIN, // HDMI (0x2a) is connected to Intel graphics
                                0x2e,   // Mini DisplayPort
                                NO_PIN, // Not Connected
                                NO_PIN, // Not Connected
                            ],
                        }),
                    }),
                    other => Err(HotPlugDetectError::VariantUnsupported {
                        model:   "gaze15",
                        variant: other.into(),
                    }),
                }
            }
            "gaze16-3050" => Ok(Self {
                integrated: Integrated::Intel(Intel {
                    sideband: Sideband::new(PCR_BASE_ADDRESS)?,
                    port:     0x6A,
                    pins:     [
                        NO_PIN, // HDMI (0x52) is connected to Intel graphics
                        0x58,   // Mini DisplayPort
                        NO_PIN, // Not Connected
                        NO_PIN, // Not Connected
                    ],
                }),
            }),
            "gaze16-3060" | "gaze16-3060-b" => Ok(Self {
                integrated: Integrated::Intel(Intel {
                    sideband: Sideband::new(PCR_BASE_ADDRESS)?,
                    port:     0x69,
                    pins:     [
                        0x02,   // Mini DisplayPort
                        0x04,   // USB-C
                        NO_PIN, // Not Connected
                        NO_PIN, // Not Connected
                    ],
                }),
            }),
            "gaze17-3060-b" => Ok(Self {
                integrated: Integrated::Intel(Intel {
                    sideband: Sideband::new(PCR_BASE_ADDRESS)?,
                    port:     0x6E,
                    pins:     [
                        0x72,   // Mini DisplayPort
                        0x78,   // HDMI
                        NO_PIN, // Not Connected
                        NO_PIN, // Not Connected
                    ],
                }),
            }),
            "kudu6" => {
                let gpios = vec![
                    0x02, // USB-C
                    0x03, // HDMI
                    0x15, // Mini DisplayPort
                ];
                Ok(Self { integrated: Integrated::Amd(Amd::new(gpios)?) })
            }

            "oryp4" | "oryp4-b" | "oryp5" => Ok(Self {
                integrated: Integrated::Intel(Intel {
                    sideband: Sideband::new(PCR_BASE_ADDRESS)?,
                    port:     0x6A,
                    pins:     [
                        0x28,   // USB-C
                        0x2a,   // HDMI
                        0x2c,   // Mini DisplayPort
                        NO_PIN, // Not Connected
                    ],
                }),
            }),
            "oryp6" | "oryp7" => Ok(Self {
                integrated: Integrated::Intel(Intel {
                    sideband: Sideband::new(PCR_BASE_ADDRESS)?,
                    port:     0x6A,
                    pins:     [
                        0x2a,   // HDMI
                        0x2c,   // Mini DisplayPort
                        0x2e,   // USB-C
                        NO_PIN, // Not Connected
                    ],
                }),
            }),
            "oryp8" => Ok(Self {
                integrated: Integrated::Intel(Intel {
                    sideband: Sideband::new(PCR_BASE_ADDRESS)?,
                    port:     0x69,
                    pins:     [
                        0x02,   // Mini DisplayPort
                        0x04,   // HDMI
                        0x06,   // USB-C
                        NO_PIN, // Not Connected
                    ],
                }),
            }),
            "oryp9" | "oryp10" => Ok(Self {
                integrated: Integrated::Intel(Intel {
                    sideband: Sideband::new(PCR_BASE_ADDRESS)?,
                    port:     0x6E,
                    pins:     [
                        0x72,   // Mini DisplayPort
                        0x78,   // HDMI
                        0x7C,   // USB-C
                        NO_PIN, // Not Connected
                    ],
                }),
            }),
            "oryp11" => Ok(Self {
                integrated: Integrated::Intel(Intel {
                    sideband: Sideband::new(PCR_BASE_ADDRESS)?,
                    port:     0x6E,
                    pins:     [
                        0x72,   // Mini DisplayPort
                        0x78,   // HDMI
                        NO_PIN, // TODO: USB-C?
                        NO_PIN, // Not connected
                    ],
                }),
            }),
            "oryp12" => Ok(Self {
                integrated: Integrated::Intel(Intel {
                    sideband: Sideband::new(0xE000_0000)?,
                    port:     0x6E,
                    pins:     [
                        0x04,   // HDMI
                        0x08,   // Mini DisplayPort
                        NO_PIN, // TOOD: USB-C?
                        NO_PIN, // NC
                    ],
                }),
            }),
            "serw13" => Ok(Self {
                integrated: Integrated::Intel(Intel {
                    sideband: Sideband::new(0xE000_0000)?,
                    port:     0x6E,
                    pins:     [
                        0x00,   // USB-C
                        NO_PIN, // TBT connected to iGPU
                        0x04,   // HDMI
                        0x08,   // Mini DisplayPort
                    ],
                }),
            }),
            other => Err(HotPlugDetectError::ModelUnsupported(other.into())),
        }
    }
}

impl Detect for HotPlugDetect {
    unsafe fn detect(&mut self) -> [bool; 4] {
        match &mut self.integrated {
            Integrated::Amd(amd) => amd.detect(),
            Integrated::Intel(intel) => intel.detect(),
        }
    }
}
