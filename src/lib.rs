#[macro_use]
extern crate err_derive;
extern crate intel_pstate as pstate;
#[macro_use]
extern crate log;

use anyhow::Result;

pub mod client;
pub mod daemon;
pub mod disks;
pub mod errors;
pub mod fan;
pub mod graphics;
pub mod hid_backlight;
pub mod hotplug;
pub mod kernel_parameters;
pub mod logging;
pub mod modprobe;
pub mod module;
pub mod mux;
pub mod pci;
pub mod radeon;
pub mod sideband;
pub mod snd;
pub mod util;
pub mod wifi;

include!(concat!(env!("OUT_DIR"), "/version.rs"));

pub static DBUS_NAME: &'static str = "com.system76.PowerDaemon";
pub static DBUS_PATH: &'static str = "/com/system76/PowerDaemon";
pub static DBUS_IFACE: &'static str = "com.system76.PowerDaemon";

pub trait Power {
    fn performance(&mut self) -> Result<()>;
    fn balanced(&mut self) -> Result<()>;
    fn battery(&mut self) -> Result<()>;
    fn get_graphics(&mut self) -> Result<String>;
    fn get_profile(&mut self) -> Result<String>;
    fn get_switchable(&mut self) -> Result<bool>;
    fn set_graphics(&mut self, vendor: &str) -> Result<()>;
    fn get_graphics_power(&mut self) -> Result<bool>;
    fn set_graphics_power(&mut self, power: bool) -> Result<()>;
    fn auto_graphics_power(&mut self) -> Result<()>;
}