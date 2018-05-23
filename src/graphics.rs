use std::io;

use pci::{PciBus, PciDevice};

pub struct Graphics {
    pub intel: Vec<PciDevice>,
    pub nvidia: Vec<PciDevice>,
    pub other: Vec<PciDevice>,
}

impl Graphics {
    pub fn new() -> io::Result<Graphics> {
        let bus = PciBus::new()?;

        bus.rescan()?;

        let mut intel = Vec::new();
        let mut nvidia = Vec::new();
        let mut other = Vec::new();

        for dev in bus.devices()? {
            let class = dev.class()?;
            if class == 0x030000 {
                match dev.vendor()? {
                    0x10DE => nvidia.push(dev),
                    0x8086 => intel.push(dev),
                    _ => other.push(dev),
                }
            }
        }

        Ok(Graphics {
            intel: intel,
            nvidia: nvidia,
            other: other,
        })
    }

    pub fn can_switch(&self) -> bool {
        self.intel.len() > 0 && self.nvidia.len() > 0
    }
}
