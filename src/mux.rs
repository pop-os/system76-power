use err_str;
use sideband::Sideband;
use util::read_file;

pub struct DisplayPortMux {
    sideband: Sideband,
    hpd: (u8, u8),
    mux: (u8, u8),
}

impl DisplayPortMux {
    pub unsafe fn new() -> Result<DisplayPortMux, String> {
        let model_line = read_file("/sys/class/dmi/id/product_version").map_err(err_str)?;
        let model = model_line.trim();
        match model {
            "galp2" | "galp3" | "galp3-b" => Ok(DisplayPortMux {
                sideband: Sideband::new(0xFD00_0000)?,
                hpd: (0xAE, 0x31), // GPP_E13
                mux: (0xAF, 0x16), // GPP_A22
            }),
            "darp5" | "galp3-c" => Ok(DisplayPortMux {
                sideband: Sideband::new(0xFD00_0000)?,
                hpd: (0x6A, 0x4A), // GPP_E13
                mux: (0x6E, 0x2C), // GPP_A22
            }),
            _ => Err(format!("{} does not support hotplug detection", model))
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
                mux_data = mux_data & !1;
            } else {
                // HPD low, switching to USB-C
                mux_data = mux_data | 1;
            }

            self.sideband.set_gpio(self.mux.0, self.mux.1, mux_data);
        }
    }
}
