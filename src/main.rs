#[macro_use]
extern crate clap;
extern crate dbus;
extern crate libc;

use std::{env, process};

mod backlight;
use clap::{Arg, App, AppSettings, SubCommand};
mod client;
mod daemon;
mod kbd_backlight;
mod graphics;
mod hotplug;
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
    let version = format!("{}", crate_version!());
    let matches = App::new("system76-power")
        .about("Utility for managing power profiles")
        .version(version.as_str())
        .global_setting(AppSettings::ColoredHelp)
        .global_setting(AppSettings::UnifiedHelpMessage)
        .setting(AppSettings::SubcommandRequiredElseHelp)
        .arg(Arg::with_name("quiet")
            .short("q")
            .long("quiet")
            .global(true)
            .group("verbosity"))
        .arg(Arg::with_name("verbose")
            .short("v")
            .long("verbose")
            .global(true)
            .group("verbosity"))
        .subcommand(SubCommand::with_name("daemon")
            .about("Runs the program in daemon mode")
            .arg(Arg::with_name("experimental")
                .long("experimental")
                .help("enables experimental features"))
        )
        .subcommand(SubCommand::with_name("profile")
            .about("Query or set the power profile")
            .arg(Arg::with_name("profile")
                .help("set the power profile")
                .possible_values(&["battery", "balanced", "performance"])
                .required(false))
        )
        .subcommand(SubCommand::with_name("graphics")
            .about("Query or set the graphics mode")
            .subcommand(SubCommand::with_name("intel")
                .about("Set the graphics mode to Intel"))
            .subcommand(SubCommand::with_name("nvidia")
                .about("Set the graphics mode to NVIDIA"))
            .subcommand(SubCommand::with_name("power")
                .about("Query or set the discrete graphics power state")
                .arg(Arg::with_name("state")
                    .help("Set whether discrete graphics should be on or off")
                    .possible_values(&["auto", "off", "on"]))
            )
        )
        .get_matches();

    let res = match matches.subcommand() {
        ("daemon", Some(_matches)) => {
            if unsafe { libc::geteuid() } == 0 {
                daemon::daemon()
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
