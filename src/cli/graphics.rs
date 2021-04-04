use clap::Clap;
use system76_power::{client::PowerClient};

/// Query or set the graphics mode.\n\n - If an argument is not provided, the graphics profile will
/// be queried\n - Otherwise, that profile will be set, if it is a valid profile
#[derive(Clap)]
#[clap(about = "Query or set the graphics mode")]
pub struct Command {
    #[clap(subcommand)]
    subcommand: Option<Subcommand>,
}

impl Command {
    pub fn run(&self, client: &mut PowerClient) -> Result<(), String> {
        if let Some(subcommand) = &self.subcommand {
            subcommand.run(client)
        } else {
            println!("{}", client.graphics()?);
            Ok(())
        }
    }
}

#[derive(Clap)]
pub enum Subcommand {
    /// Like integrated, but the dGPU is available for compute
    Compute,

    /// Set the graphics mode to Hybrid (PRIME)
    Hybrid,

    /// Set the graphics mode to integrated
    Integrated,

    /// Set the graphics mode to NVIDIA
    Nvidia,

    /// Determines if the system has switchable graphics
    Switchable,

    /// Query or set the discrete graphics power state
    Power {
        /// Set whether discrete graphics should be on or off
        #[clap(arg_enum)]
        state: Option<State>,
    },
}

#[derive(Clap)]
pub enum State {
    Auto,
    On,
    Off,
}

impl Subcommand {
    pub fn run(&self, client: &mut PowerClient) -> Result<(), String> {
        match self {
            Self::Compute => client.set_graphics("compute"),
            Self::Hybrid => client.set_graphics("hybrid"),
            Self::Integrated => client.set_graphics("integrated"),
            Self::Nvidia => client.set_graphics("nvidia"),
            Self::Switchable => {
                if client.get_switchable()? {
                    println!("switchable");
                } else {
                    println!("not switchable");
                }
                Ok(())
            }
            Self::Power { state: Some(State::Auto) } => client.auto_graphics_power(),
            Self::Power { state: Some(State::Off) } => client.set_graphics_power(false),
            Self::Power { state: Some(State::On) } => client.set_graphics_power(true),
            Self::Power { state: None } => {
                if client.graphics_power()? {
                    println!("on (discrete)");
                } else {
                    println!("off (discrete)");
                }
                Ok(())
            }
        }
    }
}
