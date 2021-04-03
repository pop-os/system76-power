use clap::Clap;

/// Queries or sets the power profile.\n\n - If an argument is not provided, the power profile will
/// be queried\n - Otherwise, that profile will be set, if it is a valid profile
#[derive(Clap)]
#[clap(about = "Query or set the power profile")]
pub struct Command {
    #[clap(arg_enum)]
    profile: Option<PowerProfile>,
}

#[derive(Clap)]
pub enum PowerProfile {
    Battery,
    Balanced,
    Performance,
}

impl Command {
    pub fn run(&self) -> Result<(), String> {
        let client = PowerClient::new()?;
        match self.profile {
            Some(PowerProfile::Battery) => client.battery(),
            Some(PowerProfile::Balanced) => client.balanced(),
            Some(PowerProfile::Performance) => client.performance(),
            None => print_profile().map_err(|e| format!("{}, e"))
        }
    }
}

fn print_profile(client: &mut PowerClient) -> io::Result<()> {
    let profile = client.get_profile().ok();
    let profile = profile.as_ref().map_or("?", |s| s.as_str());
    println!("Power Profile: {}", profile);

    if let Ok(values) = PState::new().and_then(|pstate| pstate.values()) {
        println!(
            "CPU: {}% - {}%, {}",
            values.min_perf_pct,
            values.max_perf_pct,
            if values.no_turbo { "No Turbo" } else { "Turbo" }
        );
    }

    for backlight in Backlight::iter() {
        let backlight = backlight?;
        let brightness = backlight.actual_brightness()?;
        let max_brightness = backlight.max_brightness()?;
        let ratio = (brightness as f64) / (max_brightness as f64);
        let percent = (ratio * 100.0) as u64;
        println!("Backlight {}: {}/{} = {}%", backlight.id(), brightness, max_brightness, percent);
    }

    for backlight in Leds::iter_keyboards() {
        let backlight = backlight?;
        let brightness = backlight.brightness()?;
        let max_brightness = backlight.max_brightness()?;
        let ratio = (brightness as f64) / (max_brightness as f64);
        let percent = (ratio * 100.0) as u64;
        println!(
            "Keyboard Backlight {}: {}/{} = {}%",
            backlight.id(),
            brightness,
            max_brightness,
            percent
        );
    }

    Ok(())
}