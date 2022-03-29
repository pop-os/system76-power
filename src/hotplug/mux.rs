// Copyright 2018-2021 System76 <info@system76.com>
//
// SPDX-License-Identifier: GPL-3.0-only

use crate::hotplug::{
    sideband::{Sideband, PCR_BASE_ADDRESS},
    HotPlugDetectError,
};
use std::fs;

pub struct DisplayPortMux {
    sideband: Sideband,
    hpd:      (u8, u8),
    mux:      (u8, u8),
}

impl DisplayPortMux {
    pub unsafe fn new() -> Result<Self, HotPlugDetectError> {
        let model = fs::read_to_string("/sys/class/dmi/id/product_version")
            .map_err(HotPlugDetectError::ProductVersion)?;

        match model.trim() {
            "bonw14" => Ok(Self {
                sideband: Sideband::new(PCR_BASE_ADDRESS)?,
                hpd:      (0x6A, 0x2E), // GPP_I3
                mux:      (0x6B, 0x0A), // GPP_K5
            }),
            "galp2" | "galp3" | "galp3-b" => Ok(Self {
                sideband: Sideband::new(PCR_BASE_ADDRESS)?,
                hpd:      (0xAE, 0x31), // GPP_E13
                mux:      (0xAF, 0x16), // GPP_A22
            }),
            "darp5" | "darp6" | "galp3-c" | "galp4" => Ok(Self {
                sideband: Sideband::new(PCR_BASE_ADDRESS)?,
                hpd:      (0x6A, 0x4A), // GPP_E13
                mux:      (0x6E, 0x2C), // GPP_A22
            }),
            other => Err(HotPlugDetectError::ModelUnsupported(other.into())),
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
