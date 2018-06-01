extern crate dbus;
extern crate upower_dbus;
extern crate libc;

use std::{env, process};

pub mod ac_events;
mod backlight;
pub(crate) mod client;
mod daemon;
mod kbd_backlight;
mod graphics;
mod module;
mod pci;
mod pstate;
mod util;

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
    let res = if env::args().nth(1).map_or(false, |arg| arg == "daemon") {
        if unsafe { libc::geteuid() } == 0 {
            daemon::daemon()
        } else {
            Err(format!("must be run as root"))
        }
    } else {
        client::client(env::args().skip(1))
    };

    match res {
        Ok(()) => (),
        Err(err) => {
            eprintln!("system76-power: {}", err);
            process::exit(1);
        }
    }
}
