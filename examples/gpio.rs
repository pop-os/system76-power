use log::LevelFilter;
use std::process;
use system76_power::{
    hotplug::sideband::{Sideband, SidebandError, PCR_BASE_ADDRESS},
    logging,
};

struct GpioGroup<'a> {
    name:  &'a str,
    count: u8,
}

struct GpioCommunity<'a> {
    id:     u8,
    groups: &'a [GpioGroup<'a>],
}

impl<'a> GpioCommunity<'a> {
    pub const fn skylake() -> &'static [GpioCommunity<'static>] {
        &[
            GpioCommunity {
                id:     0xAF,
                groups: &[
                    GpioGroup { name: "GPP_A", count: 24 },
                    GpioGroup { name: "GPP_B", count: 24 },
                ],
            },
            GpioCommunity {
                id:     0xAE,
                groups: &[
                    GpioGroup { name: "GPP_C", count: 24 },
                    GpioGroup { name: "GPP_D", count: 24 },
                    GpioGroup { name: "GPP_E", count: 13 },
                    GpioGroup { name: "GPP_F", count: 24 },
                    GpioGroup { name: "GPP_G", count: 24 },
                    GpioGroup { name: "GPP_H", count: 24 },
                ],
            },
            GpioCommunity { id: 0xAD, groups: &[GpioGroup { name: "GPD", count: 12 }] },
            GpioCommunity { id: 0xAC, groups: &[GpioGroup { name: "GPP_I", count: 11 }] },
        ]
    }

    #[allow(dead_code)]
    pub const fn cannonlake() -> &'static [GpioCommunity<'static>] {
        &[
            GpioCommunity {
                id:     0x6E,
                groups: &[
                    GpioGroup { name: "GPP_A", count: 24 },
                    GpioGroup { name: "GPP_B", count: 24 },
                    GpioGroup { name: "GPP_G", count: 8 },
                ],
            },
            GpioCommunity {
                id:     0x6D,
                groups: &[
                    GpioGroup { name: "GPP_D", count: 24 },
                    GpioGroup { name: "GPP_F", count: 24 },
                    GpioGroup { name: "GPP_H", count: 24 },
                ],
            },
            GpioCommunity { id: 0x6C, groups: &[GpioGroup { name: "GPD", count: 12 }] },
            GpioCommunity {
                id:     0x6A,
                groups: &[
                    GpioGroup { name: "GPP_C", count: 24 },
                    GpioGroup { name: "GPP_E", count: 24 },
                ],
            },
        ]
    }
}

fn inner() -> Result<(), SidebandError> {
    let communities = GpioCommunity::skylake();

    let sideband = unsafe { Sideband::new(PCR_BASE_ADDRESS)? };

    for community in communities {
        let mut pad = 0;
        for group in community.groups {
            for i in 0..group.count {
                let data = unsafe { sideband.gpio(community.id, pad) };
                let low = data as u32;
                let high = (data >> 32) as u32;
                println!("{}{} = {:#>08x} {:#>08x}", group.name, i, low, high);
                pad += 1;
            }
        }
    }

    Ok(())
}

fn main() {
    if let Err(why) = logging::setup(LevelFilter::Debug) {
        eprintln!("failed to set up logging: {}", why);
        process::exit(1);
    }

    if unsafe { libc::geteuid() } != 0 {
        eprintln!("must be run as root");
        process::exit(1);
    }

    if let Err(err) = inner() {
        eprintln!("{:?}", err);
        process::exit(1);
    }
}
