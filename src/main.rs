use clap::{App, AppSettings, Arg, SubCommand};
use log::LevelFilter;
use std::process;
use system76_power::{client, daemon, logging};

fn main() {
    let matches = App::new("system76-power")
        .about("Utility for managing graphics and power profiles")
        .version(env!("CARGO_PKG_VERSION"))
        .global_setting(AppSettings::ColoredHelp)
        .global_setting(AppSettings::UnifiedHelpMessage)
        .global_setting(AppSettings::VersionlessSubcommands)
        .setting(AppSettings::SubcommandRequiredElseHelp)
        .subcommand(
            SubCommand::with_name("daemon")
                .about("Runs the program in daemon mode")
                .long_about(
                    "Registers a new DBUS service and starts an event loopto listen for, and \
                     respond to, DBUS events from clients",
                )
                .arg(
                    Arg::with_name("quiet")
                        .short("q")
                        .long("quiet")
                        .help("Set the verbosity of daemon logs to 'off' [default is 'info']")
                        .global(true)
                        .group("verbosity"),
                )
                .arg(
                    Arg::with_name("verbose")
                        .short("v")
                        .long("verbose")
                        .help("Set the verbosity of daemon logs to 'debug' [default is 'info']")
                        .global(true)
                        .group("verbosity"),
                ),
        )
        .subcommand(
            SubCommand::with_name("profile")
                .about("Query or set the power profile")
                .long_about(
                    "Queries or sets the power profile.\n\n - If an argument is not provided, the \
                     power profile will be queried\n - Otherwise, that profile will be set, if it \
                     is a valid profile",
                )
                .arg(
                    Arg::with_name("profile")
                        .help("set the power profile")
                        .possible_values(&["battery", "balanced", "performance"])
                        .required(false),
                ),
        )
        .subcommand(
            SubCommand::with_name("graphics")
                .about("Query or set the graphics mode")
                .long_about(
                    "Query or set the graphics mode.\n\n - If an argument is not provided, the \
                     graphics profile will be queried\n - Otherwise, that profile will be set, if \
                     it is a valid profile",
                )
                .subcommand(
                    SubCommand::with_name("compute")
                        .about("Like integrated, but the dGPU is available for compute"))
                .subcommand(
                    SubCommand::with_name("hybrid")
                        .about("Set the graphics mode to Hybrid (PRIME)"),
                )
                .subcommand(SubCommand::with_name("integrated").about("Set the graphics mode to integrated"))
                .subcommand(
                    SubCommand::with_name("nvidia").about("Set the graphics mode to NVIDIA"),
                )
                .subcommand(
                    SubCommand::with_name("switchable")
                        .about("Determines if the system has switchable graphics"),
                )
                .subcommand(
                    SubCommand::with_name("power")
                        .about("Query or set the discrete graphics power state")
                        .arg(
                            Arg::with_name("state")
                                .help("Set whether discrete graphics should be on or off")
                                .possible_values(&["auto", "off", "on"]),
                        ),
                ),
        )
        .get_matches();

    let res = match matches.subcommand() {
        ("daemon", Some(matches)) => {
            if let Err(why) = logging::setup_logging(if matches.is_present("verbose") {
                LevelFilter::Debug
            } else if matches.is_present("quiet") {
                LevelFilter::Off
            } else {
                LevelFilter::Info
            }) {
                eprintln!("failed to set up logging: {}", why);
                process::exit(1);
            }

            if unsafe { libc::geteuid() } == 0 {
                daemon::daemon()
            } else {
                Err("must be run as root".to_string())
            }
        }
        (subcommand, Some(matches)) => client::client(subcommand, matches),
        _ => unreachable!(),
    };

    match res {
        Ok(()) => (),
        Err(err) => {
            eprintln!("{}", err);
            process::exit(1);
        }
    }
}
