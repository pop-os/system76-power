use log::LevelFilter;
use std::{process, thread, time};
use system76_power::{
    logging,
    sideband::{Sideband, SidebandError},
};

fn inner() -> Result<(), SidebandError> {
    let sideband = unsafe { Sideband::new(0xFD00_0000)? };

    let hpd = (0x6A, 0x4A);
    let mux = (0x6E, 0x2C);

    loop {
        let hpd_data = unsafe { sideband.gpio(hpd.0, hpd.1) };
        println!("HPD = {:#>08x} {:#>08x}", hpd_data as u32, (hpd_data >> 32) as u32);

        let mut mux_data = unsafe { sideband.gpio(mux.0, mux.1) };
        println!("MUX = {:#>08x} {:#>08x}", mux_data as u32, (mux_data >> 32) as u32);

        if hpd_data & 2 == 2 {
            println!("HPD high, not switching");
        } else {
            if mux_data & 1 == 1 {
                println!("HPD low, switching to mDP");
                mux_data &= !1;
            } else {
                println!("HPD low, switching to USB-C");
                mux_data |= 1;
            }

            println!("MUX = {:#>08x} {:#>08x}", mux_data as u32, (mux_data >> 32) as u32);
            unsafe { sideband.set_gpio(mux.0, mux.1, mux_data) };
        }

        thread::sleep(time::Duration::new(1, 0));
    }
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
