use err_str;
use sideband::Sideband;
use util::read_file;

pub struct HotPlugDetect {
    sideband: Sideband,
    port: u8,
    pins: [u8; 3],
}

impl HotPlugDetect {
    pub unsafe fn new() -> Result<HotPlugDetect, String> {
        let model = read_file("/sys/class/dmi/id/product_version")
            .map_err(err_str)?;
        match model.trim() {
            "gaze14" => {
                let variant = read_file("/sys/bus/pci/devices/0000:00:00.0/subsystem_device")
                    .map_err(err_str)?;
                match variant.trim() {
                    // NVIDIA GTX 1660 Ti
                    "0x8550" | "0x8551" => Ok(HotPlugDetect {
                        sideband: Sideband::new(0xFD00_0000)?,
                        port: 0x6A,
                        pins: [
                            0x2a, // HDMI
                            0x00, // Mini DisplayPort (0x2c) is connected to Intel graphics
                            0x2e, // USB-C
                        ],
                    }),
                    // NVIDIA GTX 1650
                    "0x8560" | "0x8561" => Ok(HotPlugDetect {
                        sideband: Sideband::new(0xFD00_0000)?,
                        port: 0x6A,
                        pins: [
                            0x00, // HDMI (0x2a) is connected to Intel graphics
                            0x2e, // Mini DisplayPort
                            0x00, // Only two external display connectors
                        ],
                    }),
                    other => Err(
                        format!("gaze14 variant '{}' does not support hotplug detection", other)
                    ),
                }
            },
            "oryp4" |
            "oryp4-b" |
            "oryp5" => Ok(HotPlugDetect {
                sideband: Sideband::new(0xFD00_0000)?,
                port: 0x6A,
                pins: [
                    0x28, // USB-C
                    0x2a, // HDMI
                    0x2c, // Mini DisplayPort
                ],
            }),
            other => Err(
                format!("model '{}' does not support hotplug detection", other)
            )
        }
    }

    pub unsafe fn detect(&self) -> [bool; 3] {
        let mut hpd = [false; 3];
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
