use clap::Clap;

/// Registers a new DBUS service and starts an event loopto listen for, and respond to, DBUS events
/// from clients"
#[derive(Clap)]
#[clap(about = "Runs the program in daemon mode")]
pub struct Command{
    /// Set the verbosity of daemon logs to 'off' [default is 'info']
    #[clap(long, short, group = "verbosity")]
    quiet: bool,

    /// Set the verbosity of daemon logs to 'debug' [default is 'info']
    #[clap(long, short, group = "verbosity")]
    verbose: bool,
}

impl Command{
    pub fn run(&self) -> Result<(), &'static str> {
        let level = if self.quiet {
            LevelFilter::Off
        } else if self.verbose {
            LevelFilter::Debug
        } else {
            LevelFilter::Info
        };

        logging::setup_logging(level).unwrap_or_else(|why| {
            eprintln!("failed to set up logging: {}", why);
            process::exit(1);
        });

        if unsafe { libc::geteuid() } == 0 {
            system76_power::daemon::daemon()
        } else {
            Err("must be run as root")
        }
    }
}