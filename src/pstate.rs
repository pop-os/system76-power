use std::io;
use std::path::PathBuf;

use config::ConfigPState;
use util::{parse_file, write_file};

pub struct PState {
    path: PathBuf,
}

impl PState {
    pub fn new() -> io::Result<PState> {
        let path = PathBuf::from("/sys/devices/system/cpu/intel_pstate");
        if path.is_dir() {
            Ok(PState { path })
        } else {
            Err(io::Error::new(io::ErrorKind::NotFound, "intel_pstate directory not found"))
        }
    }

    pub fn set_config(&mut self, config: Option<&ConfigPState>, defaults: (u8, u8, bool)) -> io::Result<()> {
        self.set_min_perf_pct(config.map_or(defaults.0, |p| p.min) as u64)?;
        self.set_max_perf_pct(config.map_or(defaults.1, |p| p.max) as u64)?;
        self.set_no_turbo(config.map_or(!defaults.2, |p| !p.turbo))
    }

    pub fn min_perf_pct(&self) -> io::Result<u64> {
        parse_file(self.path.join("min_perf_pct"))
    }

    pub fn set_min_perf_pct(&mut self, value: u64) -> io::Result<()> {
        debug!("setting intel pstate min perf to {}%", value);
        write_file(self.path.join("min_perf_pct"), format!("{}", value))
    }

    pub fn max_perf_pct(&self) -> io::Result<u64> {
        parse_file(self.path.join("max_perf_pct"))
    }

    pub fn set_max_perf_pct(&mut self, value: u64) -> io::Result<()> {
        debug!("setting intel pstate max perf to {}%0", value);
        write_file(self.path.join("max_perf_pct"), format!("{}", value))
    }

    pub fn no_turbo(&self) -> io::Result<bool> {
        let value: u64 = parse_file(self.path.join("no_turbo"))?;
        Ok(value > 0)
    }

    pub fn set_no_turbo(&mut self, value: bool) -> io::Result<()> {
        eprintln!("setting intel no_turbo to {}", value);
        write_file(self.path.join("no_turbo"), if value { "1" } else { "0" })
    }
}


