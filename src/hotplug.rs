use err_str;
use sideband::Sideband;
use util::read_file;

pub struct HotPlugDetect {
    sideband: Sideband,
    port: u8,
    pins: [u8; 3]
}

impl HotPlugDetect {
    pub unsafe fn new() -> Result<HotPlugDetect, String> {
        match read_file("/sys/class/dmi/id/product_version").map_err(err_str)?.trim() {
            "oryp4" | "oryp4-b" | "oryp5" => Ok(HotPlugDetect {
                sideband: Sideband::new(0xFD00_0000)?,
                port: 0x6A,
                pins: [40, 42, 44],
            }),
            other => Err(format!("{} does not support hotplug detection", other))
        }
    }

    pub unsafe fn detect(&self) -> [bool; 3] {
        let mut hpd = [false; 3];
        for i in 0..self.pins.len() {
            let data = self.sideband.gpio(self.port, self.pins[i]);
            hpd[i] = data & 2 == 2;
        }
        hpd
    }
}
