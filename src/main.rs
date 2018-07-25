#[macro_use]
extern crate clap;
extern crate dbus;
extern crate libc;

use clap::{Arg, App, AppSettings, SubCommand};
use std::process;

mod backlight;
mod client;
mod daemon;
mod disks;
mod kbd_backlight;
mod kernel_parameters;
mod graphics;
mod hotplug;
mod modprobe;
mod module;
mod pci;
mod pstate;
mod radeon;
mod snd;
mod scsi;
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
    let version = format!("{} ({})", crate_version!(), short_sha());
    let matches = App::new("system76-power")
        .about("Utility for managing power profiles")
        .version(version.as_str())
        .global_setting(AppSettings::ColoredHelp)
        .global_setting(AppSettings::UnifiedHelpMessage)
        .setting(AppSettings::SubcommandRequiredElseHelp)
        .subcommand(SubCommand::with_name("daemon")
            .about("Runs the program in daemon mode")
            .arg(Arg::with_name("experimental")
                .long("experimental")
                .help("enables experimental features"))
        )
        .subcommand(SubCommand::with_name("profile")
            .about("Query or set the power profile")
            .arg(Arg::with_name("performance")
                .help("set the power profile to performance")
                .group("power_profile"))
            .arg(Arg::with_name("balanced")
                .help("set the power profile to balanced")
                .group("power_profile"))
            .arg(Arg::with_name("battery")
                .help("set the power profile to battery")
                .group("power_profile"))
        )
        .subcommand(SubCommand::with_name("graphics")
            .about("Query or set the graphics mode")
            .subcommand(SubCommand::with_name("intel")
                .about("Set the graphics mode to Intel"))
            .subcommand(SubCommand::with_name("nvidia")
                .about("Set the graphics mode to NVIDIA"))
            .subcommand(SubCommand::with_name("power")
                .about("Query or set the discrete graphics power state")
                .arg(Arg::with_name("auto")
                    .help("Turn off discrete graphics if not in use")
                    .group("power_state"))
                .arg(Arg::with_name("off")
                    .help("Power off discrete graphics")
                    .group("power_state"))
                .arg(Arg::with_name("on")
                    .help("Power on discrete graphics")
                    .group("power_state"))
            )
        )
        .get_matches();


    let res = match matches.subcommand() {
        ("daemon", Some(matches)) => {
            if unsafe { libc::geteuid() } == 0 {
                daemon::daemon(matches.is_present("experimental"))
            } else {
                Err(format!("must be run as root"))
            }
        }
        (subcommand, Some(matches)) => client::client(subcommand, matches),
        _ => unreachable!()
    };

    match res {
        Ok(()) => (),
        Err(err) => {
            eprintln!("system76-power: {}", err);
            process::exit(1);
        }
    }
}
