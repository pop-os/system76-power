use std::io;

use pci::{PciBus, PciDevice};

pub struct Graphics {
    intel: Vec<PciDevice>,
    nvidia: Vec<PciDevice>
}

impl Graphics {
    pub fn new() -> io::Result<Graphics> {
        let bus = PciBus::new()?;

        bus.rescan()?;

        let mut intel = Vec::new();
        let mut nvidia = Vec::new();

        for dev in bus.devices()? {
            let class = dev.class()?;
            if class == 0x030000 {
                match dev.vendor()? {
                    0x10DE => nvidia.push(dev),
                    0x8086 => intel.push(dev),
                    other => println!("{}: Unsupported graphics vendor {:X}", dev.name(), other),
                }
            }
        }

        for dev in intel.iter() {
            match dev.driver() {
                Ok(driver) => println!("{}: Intel: {}", dev.name(), driver.name()),
                Err(err) => println!("{}: Intel: driver not loaded", dev.name()),
            }
        }

        for dev in nvidia.iter() {
            match dev.driver() {
                Ok(driver) => {
                    println!("{}: NVIDIA: {}", dev.name(), driver.name());
                },
                Err(err) => {
                    println!("{}: NVIDIA: driver not loaded", dev.name());
                },
            }
        }

        Ok(Graphics {
            intel: intel,
            nvidia: nvidia,
        })
    }

    pub fn can_switch(&self) -> bool {
        self.intel.len() > 0 && self.nvidia.len() > 0
    }
}
