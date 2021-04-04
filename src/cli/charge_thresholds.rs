use clap::Clap;
use system76_power::{charge_thresholds::get_charge_profiles, client::PowerClient};

/// Set thresholds for battery charging
#[derive(Clap)]
pub struct Command {
    #[clap(subcommand)]
    subcommand: Option<Subcommand>,
}

impl Command {
    pub fn run(&self, client: &mut PowerClient) -> Result<(), String> {
        let subcommand = self.subcommand.as_ref().unwrap_or(&Subcommand::List);
        subcommand.run(client)
    }
}

#[derive(Clap)]
pub enum Subcommand {
    /// List profiles
    List,

    /// Set the profile
    SetProfile {
        #[clap(possible_values = possible_profile_names())]
        name: String,
    },

    /// Set charge thresholds
    SetThresholds { start: u8, end: u8 },
}

impl Subcommand {
    fn run(&self, client: &mut PowerClient) -> Result<(), String> {
        let profiles = client.charge_profiles()?;

        match self {
            Self::SetThresholds { start, end } => client.set_charge_thresholds((*start, *end))?,
            Self::SetProfile { name } => {
                let profile = profiles
                    .iter()
                    .find(|p| &(p.id) == name)
                    .unwrap_or_else(|| panic!("No such profile '{}'", name));
                client.set_charge_thresholds((profile.start, profile.end))?;
            }
            Self::List => {
                for profile in &profiles {
                    println!("{}", profile.id);
                    println!("  Title: {}", profile.title);
                    println!("  Description: {}", profile.description);
                    println!("  Start: {}", profile.start);
                    println!("  End: {}", profile.end);
                }
                return Ok(());
            }
        }

        let (start, end) = client.charge_thresholds()?;
        if let Some(profile) = profiles.iter().find(|p| p.start == start && p.end == end) {
            println!("Profile: {} ({})", profile.title, profile.id);
        } else {
            println!("Profile: Custom");
        }
        println!("Start: {}", start);
        println!("End: {}", end);

        Ok(())
    }
}

fn possible_profile_names() -> &'static [&'static str] {
    lazy_static::lazy_static! {
        static ref POSSIBLE_NAMES: Vec<String> = get_charge_profiles().into_iter().map(|profile| profile.id).collect();
        static ref POSSIBLE_NAMES_STR: Vec<&'static str> = POSSIBLE_NAMES.iter().map(String::as_str).collect();
    }

    &POSSIBLE_NAMES_STR
}
