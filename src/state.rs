use std::io;
use backlight::Backlight;
use kbd_backlight::KeyboardBacklight;

pub struct State {
    pub battery_backlight: BacklightState,
    pub balanced_backlight: BacklightState,
    pub performance_backlight: BacklightState,
    pub pstate: CPUState,
    pub profile: Profile,
}

impl Default for State {
    fn default() -> State {
        State {
            battery_backlight: BacklightState::default(),
            balanced_backlight: BacklightState::default(),
            performance_backlight: BacklightState::default(),
            pstate: CPUState::default(),
            profile: Profile::Balanced
        }
    }
}

impl State {
    pub fn get_active_backlight_mut(&mut self) -> &mut BacklightState {
        match self.profile {
            Profile::Battery => &mut self.battery_backlight,
            Profile::Balanced => &mut self.balanced_backlight,
            Profile::HighPerformance => &mut self.performance_backlight,
        }
    }
}

pub enum Profile {
    Battery,
    Balanced,
    HighPerformance,
}

pub struct BacklightState {
    pub display: Vec<(Backlight, u64)>,
    pub kbd: Vec<(KeyboardBacklight, u64)>,
}

impl Default for BacklightState {
    fn default() -> BacklightState {
        BacklightState {
            display: Vec::new(),
            kbd: Vec::new(),
        }
    }
}

impl BacklightState {
    pub fn is_set(&self) -> bool {
        !self.display.is_empty() || !self.kbd.is_empty()
    }

    pub fn restore(&mut self) -> io::Result<()> {
        for &mut (ref mut backlight, old) in &mut self.display {
            backlight.set_brightness(old)?;
        }

        for &mut (ref mut backlight, old) in &mut self.kbd {
            backlight.set_brightness(old)?;
        }

        Ok(())
    }

    pub fn store(&mut self) -> io::Result<()> {
        let mut display = Vec::new();
        let mut kbd = Vec::new();

        for backlight in Backlight::all()? {
            let brightness = backlight.brightness()?;
            display.push((backlight, brightness));
        }

        for backlight in KeyboardBacklight::all()? {
            let brightness = backlight.brightness()?;
            kbd.push((backlight, brightness));
        }

        self.display = display;
        self.kbd = kbd;

        Ok(())
    }
}

pub struct CPUState {
    pub min_perf: u64,
    pub max_perf: u64,
    pub no_turbo: bool,
}

impl Default for CPUState {
    fn default() -> CPUState {
        CPUState {
            min_perf: 0,
            max_perf: 0,
            no_turbo: false
        }
    }
}
