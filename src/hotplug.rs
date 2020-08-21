use crate::sideband::{Sideband, SidebandError};
use std::{fs::read_to_string, io};

#[derive(Debug, Error)]
pub enum HotPlugDetectError {
    #[error(display = "failed to read DMI product version: {}", _0)]
    ProductVersion(io::Error),
    #[error(display = "error constructing sideband: {}", _0)]
    Sideband(SidebandError),
    #[error(display = "{} variant '{}' does not support hotplug detection", model, variant)]
    VariantUnsupported { model: &'static str, variant: String },
    #[error(display = "model '{}' does not support hotplug detection", _0)]
    ModelUnsupported(String),
    #[error(display = "failed to read {}'s subsystem device: {}", model, why)]
    SubsystemDevice { model: &'static str, why: io::Error },
}

pub struct HotPlugDetect {
    sideband: Sideband,
    port:     u8,
    pins:     [u8; 4],
}

impl HotPlugDetect {
    pub unsafe fn new(nvidia_device: Option<String>) -> Result<HotPlugDetect, HotPlugDetectError> {
        let model = read_to_string("/sys/class/dmi/id/product_version")
            .map_err(HotPlugDetectError::ProductVersion)?;

        match model.trim() {
            "addw1" => Ok(HotPlugDetect {
                sideband: Sideband::new(0xFD00_0000).map_err(HotPlugDetectError::Sideband)?,
                port:     0x6A,
                pins:     [
                    0x28, // USB-C on rear
                    0x2a, // HDMI
                    0x2c, // Mini DisplayPort
                    0x2e, // USB-C on right
                ],
            }),
            "addw2" => Ok(HotPlugDetect {
                sideband: Sideband::new(0xFD00_0000).map_err(HotPlugDetectError::Sideband)?,
                port:     0x6A,
                pins:     [
                    0x28, // USB-C on rear
                    0x2a, // HDMI
                    0x2c, // Mini DisplayPort
                    0x2e, // USB-C on right
                ],
            }),
            "gaze14" => {
                let variant = read_to_string("/sys/bus/pci/devices/0000:00:00.0/subsystem_device")
                    .map_err(|why| HotPlugDetectError::SubsystemDevice { model: "gaze14", why })?;

                match variant.trim() {
                    // NVIDIA GTX 1660 Ti
                    "0x8550" | "0x8551" => Ok(HotPlugDetect {
                        sideband: Sideband::new(0xFD00_0000)
                            .map_err(HotPlugDetectError::Sideband)?,
                        port:     0x6A,
                        pins:     [
                            0x2a, // HDMI
                            0x00, // Mini DisplayPort (0x2c) is connected to Intel graphics
                            0x2e, // USB-C
                            0x00, // Not Connected
                        ],
                    }),
                    // NVIDIA GTX 1650
                    "0x8560" | "0x8561" => Ok(HotPlugDetect {
                        sideband: Sideband::new(0xFD00_0000)
                            .map_err(HotPlugDetectError::Sideband)?,
                        port:     0x6A,
                        pins:     [
                            0x00, // HDMI (0x2a) is connected to Intel graphics
                            0x2e, // Mini DisplayPort
                            0x00, // Not Connected
                            0x00, // Not Connected
                        ],
                    }),
                    other => Err(HotPlugDetectError::VariantUnsupported {
                        model:   "gaze14",
                        variant: other.into(),
                    }),
                }
            },
            "gaze15" => {
                let variant = nvidia_device.unwrap_or("unknown".to_string());

                match variant.trim() {
                    // NVIDIA GTX 1660 Ti
                    "0x2191" => Ok(HotPlugDetect {
                        sideband: Sideband::new(0xFD00_0000)
                            .map_err(HotPlugDetectError::Sideband)?,
                        port:     0x6A,
                        pins:     [
                            0x2a, // HDMI
                            0x00, // Mini DisplayPort (0x2c) is connected to Intel graphics
                            0x2e, // USB-C
                            0x00, // Not Connected
                        ],
                    }),
                    // NVIDIA GTX 1650, 1650 Ti
                     "0x1f99" | "0x1f95" => Ok(HotPlugDetect {
                        sideband: Sideband::new(0xFD00_0000)
                            .map_err(HotPlugDetectError::Sideband)?,
                        port:     0x6A,
                        pins:     [
                            0x00, // HDMI (0x2a) is connected to Intel graphics
                            0x2e, // Mini DisplayPort
                            0x00, // Not Connected
                            0x00, // Not Connected
                        ],
                    }),
                    other => Err(HotPlugDetectError::VariantUnsupported {
                        model:   "gaze15",
                        variant: other.into(),
                    }),
                }
            },
            "oryp4" | "oryp4-b" | "oryp5" => Ok(HotPlugDetect {
                sideband: Sideband::new(0xFD00_0000).map_err(HotPlugDetectError::Sideband)?,
                port:     0x6A,
                pins:     [
                    0x28, // USB-C
                    0x2a, // HDMI
                    0x2c, // Mini DisplayPort
                    0x00, // Not Connected
                ],
            }),
            "oryp6" => Ok(HotPlugDetect {
                sideband: Sideband::new(0xFD00_0000).map_err(HotPlugDetectError::Sideband)?,
                port:     0x6A,
                pins:     [
                    0x2a, // HDMI
                    0x2c, // Mini DisplayPort
                    0x2e, // USB-C
                    0x00, // Not Connected
                ],
            }),
            other => Err(HotPlugDetectError::ModelUnsupported(other.into())),
        }
    }

    pub unsafe fn detect(&self) -> [bool; 4] {
        let mut hpd = [false; 4];
        for i in 0..self.pins.len() {
            let pin = self.pins[i];
            if pin > 0 {
                let data = self.sideband.gpio(self.port, pin);
                hpd[i] = data & 2 == 2;
            }
        }
        hpd
    }
}
