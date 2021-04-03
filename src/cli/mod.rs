use clap::{Clap, AppSettings};

mod daemon;
mod profile;
mod graphics;
mod charge_thresholds;

/// Utility for managing graphics and power profiles
#[derive(Clap)]
#[clap(global_settings = &[AppSettings::ColoredHelp, AppSettings::UnifiedHelpMessage, AppSettings::VersionlessSubcommands])]
pub enum Command {
    Daemon(daemon::Command),
    Profile(profile::Command),
    Graphics(graphics::Command),
    ChargeThresholds(charge_thresholds::Command),
}

impl Command {
    pub fn run(&self) {
        match self {
            Self::Daemon(command) => command.run(),
            _ => todo!(),
        }
    }
}

#[derive(Clap)]
pub struct Graphics {

}