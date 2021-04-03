use clap::Clap;


/// Set thresholds for battery charging
#[derive(Clap)]
pub struct Command {
    #[clap(subcommand)]
    subcommand: Option<Subcommand>
}

pub enum Subcommand {
    /// List profiles
    List,

    /// Set the profile
    Set {
        #[clap(possible_values = get_charge_profiles())]
        profile_name: String
    },


    /// Set charge thresholds
    SetThresholds {
        start: u8,
        end: u8,
    }
}