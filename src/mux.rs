use crate::sideband::{Sideband, SidebandError};
use std::{fs::read_to_string, io};

#[derive(Debug, Error)]
pub enum DisplayPortMuxError {
    #[error(display = "error constructing sideband: {}", _0)]
    Sideband(SidebandError),
    #[error(display = "failed to read DMI product version: {}", _0)]
    ProductVersion(io::Error),
    #[error(display = "model '{}' does not support hotplug detection", _0)]
    UnsupportedHotPlugDetect(String),
}

impl From<SidebandError> for DisplayPortMuxError {
    fn from(err: SidebandError) -> Self { DisplayPortMuxError::Sideband(err) }
}

pub struct DisplayPortMux {
    sideband: Sideband,
    hpd:      (u8, u8),
    mux:      (u8, u8),
}

impl DisplayPortMux {
    pub unsafe fn new() -> Result<DisplayPortMux, DisplayPortMuxError> {
        let model_line = read_to_string("/sys/class/dmi/id/product_version")
            .map_err(DisplayPortMuxError::ProductVersion)?;

        let model = model_line.trim();
        match model {
            "bonw14" => Ok(DisplayPortMux {
                sideband: Sideband::new(0xFD00_0000)?,
                hpd:      (0x6A, 0x2E), // GPP_I3
                mux:      (0x6B, 0x0A), // GPP_K5
            }),
            "galp2" | "galp3" | "galp3-b" => Ok(DisplayPortMux {
                sideband: Sideband::new(0xFD00_0000)?,
                hpd:      (0xAE, 0x31), // GPP_E13
                mux:      (0xAF, 0x16), // GPP_A22
            }),
            "darp5" | "darp6" | "galp3-c" | "galp4" => Ok(DisplayPortMux {
                sideband: Sideband::new(0xFD00_0000)?,
                hpd:      (0x6A, 0x4A), // GPP_E13
                mux:      (0x6E, 0x2C), // GPP_A22
            }),
            _ => Err(DisplayPortMuxError::UnsupportedHotPlugDetect(model.to_owned())),
        }
    }

    pub unsafe fn step(&self) {
        let hpd_data = self.sideband.gpio(self.hpd.0, self.hpd.1);

        if hpd_data & 2 == 2 {
            // HPD high, not switching
        } else {
            let mut mux_data = self.sideband.gpio(self.mux.0, self.mux.1);

            if mux_data & 1 == 1 {
                // HPD low, switching to mDP
                mux_data &= !1;
            } else {
                // HPD low, switching to USB-C
                mux_data |= 1;
            }

            self.sideband.set_gpio(self.mux.0, self.mux.1, mux_data);
        }
    }
}
