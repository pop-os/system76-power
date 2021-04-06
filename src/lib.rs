#![deny(clippy::all)]
#![allow(clippy::missing_safety_doc)]

pub mod charge_thresholds;
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
pub mod polkit;
pub mod radeon;
pub mod sideband;
pub mod snd;
pub mod util;
pub mod wifi;

use charge_thresholds::ChargeProfile;

pub static DBUS_NAME: &str = "com.system76.PowerDaemon";
pub static DBUS_PATH: &str = "/com/system76/PowerDaemon";
pub static DBUS_IFACE: &str = "com.system76.PowerDaemon";

pub trait Power {
    fn performance(&mut self) -> Result<(), String>;
    fn balanced(&mut self) -> Result<(), String>;
    fn battery(&mut self) -> Result<(), String>;
    fn get_external_displays_require_dgpu(&mut self) -> Result<bool, String>;
    fn get_default_graphics(&mut self) -> Result<String, String>;
    fn get_graphics(&mut self) -> Result<String, String>;
    fn get_profile(&mut self) -> Result<String, String>;
    fn get_switchable(&mut self) -> Result<bool, String>;
    fn set_graphics(&mut self, vendor: &str) -> Result<(), String>;
    fn get_graphics_power(&mut self) -> Result<bool, String>;
    fn set_graphics_power(&mut self, power: bool) -> Result<(), String>;
    fn auto_graphics_power(&mut self) -> Result<(), String>;
    fn get_charge_thresholds(&mut self) -> Result<(u8, u8), String>;
    fn set_charge_thresholds(&mut self, thresholds: (u8, u8)) -> Result<(), String>;
    fn get_charge_profiles(&mut self) -> Result<Vec<ChargeProfile>, String>;
}

// Helper function for errors
pub(crate) fn err_str<E: ::std::fmt::Display>(err: E) -> String { format!("{}", err) }
