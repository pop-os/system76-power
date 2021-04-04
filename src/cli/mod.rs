use clap::{AppSettings, Clap};
use system76_power::client::PowerClient;

mod charge_thresholds;
mod daemon;
mod graphics;
mod profile;

/// Utility for managing graphics and power profiles
#[derive(Clap)]
#[clap(global_setting = AppSettings::ColoredHelp, global_setting = AppSettings::UnifiedHelpMessage, global_setting = AppSettings::VersionlessSubcommands)]
pub enum Command {
    Daemon(daemon::Command),
    Profile(profile::Command),
    Graphics(graphics::Command),
    ChargeThresholds(charge_thresholds::Command),
}

impl Command {
    pub fn run(&self) -> Result<(), String> {
        let mut client = PowerClient::new()?;
        match self {
            Self::Daemon(command) => command.run(),
            Self::Profile(command) => command.run(&mut client),
            Self::Graphics(command) => command.run(&mut client),
            Self::ChargeThresholds(command) => command.run(&mut client),
        }
    }
}
