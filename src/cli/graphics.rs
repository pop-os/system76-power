use clap::Clap;

/// Query or set the graphics mode.\n\n - If an argument is not provided, the graphics profile will be queried\n - Otherwise, that profile will be set, if it is a valid profile
#[derive(Clap)]
#[clap(about = "Query or set the graphics mode")]
pub enum Command {
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
    }
}

#[derive(Clap)]
enum State {
    Auto,
    On,
    Off,
}

impl Command {
    pub fn run(&self) {
        todo!()
    }
}