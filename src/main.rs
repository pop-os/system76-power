extern crate dbus;
extern crate fern;
extern crate libc;
#[macro_use]
extern crate log;

use log::LevelFilter;
use std::{env, process};

mod backlight;
mod client;
mod daemon;
mod disks;
mod graphics;
mod hotplug;
mod kbd_backlight;
mod kernel_parameters;
mod logging;
mod modprobe;
mod module;
mod pci;
mod pstate;
mod radeon;
mod scsi;
mod snd;
mod util;
mod wifi;

include!(concat!(env!("OUT_DIR"), "/version.rs"));

pub static DBUS_NAME: &'static str = "com.system76.PowerDaemon";
pub static DBUS_PATH: &'static str = "/com/system76/PowerDaemon";
pub static DBUS_IFACE: &'static str = "com.system76.PowerDaemon";

pub trait Power {
    fn performance(&mut self) -> Result<(), String>;
    fn balanced(&mut self) -> Result<(), String>;
    fn battery(&mut self) -> Result<(), String>;
    fn get_graphics(&mut self) -> Result<String, String>;
    fn set_graphics(&mut self, vendor: &str) -> Result<(), String>;
    fn get_graphics_power(&mut self) -> Result<bool, String>;
    fn set_graphics_power(&mut self, power: bool) -> Result<(), String>;
    fn auto_graphics_power(&mut self) -> Result<(), String>;
}

// Helper function for errors
pub (crate) fn err_str<E: ::std::fmt::Display>(err: E) -> String {
    format!("{}", err)
}

fn main() {
    let contains_verbose = env::args().skip(1).any(|x| x.as_str() == "--verbose");
    let contains_quiet = env::args().skip(1).any(|x| x.as_str() == "--quiet");
    let contains_experimental = env::args().skip(1).any(|x| x.as_str() == "--experimental");

    let res = if env::args().nth(1).map_or(false, |arg| arg == "daemon") {
        if unsafe { libc::geteuid() } == 0 {
            daemon::daemon(contains_experimental)
        } else {
            Err(format!("must be run as root"))
        }
    } else {
        client::client(env::args().skip(1))
    };

    if let Err(why) = logging::setup_logging(
        if contains_verbose {
            LevelFilter::Debug
        } else if contains_quiet {
            LevelFilter::Off
        } else {
            LevelFilter::Info
        }
    ) {
        eprintln!("failed to set up logging: {}", why);
        process::exit(1);
    }


    

    match res {
        Ok(()) => (),
        Err(err) => {
            error!("{}", err);
            process::exit(1);
        }
    }
}
