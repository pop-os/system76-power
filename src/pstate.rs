use std::io;
use std::path::PathBuf;

use util::{parse_file, write_file};

pub struct PState {
    path: PathBuf,
}

impl PState {
    pub fn new() -> io::Result<PState> {
        //TODO: Check for validity
        Ok(PState {
            path: PathBuf::from(
                "/sys/devices/system/cpu/intel_pstate"
            )
        })
    }

    pub fn min_perf_pct(&self) -> io::Result<u64> {
        parse_file(self.path.join("min_perf_pct"))
    }

    pub fn set_min_perf_pct(&mut self, value: u64) -> io::Result<()> {
        write_file(self.path.join("min_perf_pct"), format!("{}", value))
    }

    pub fn max_perf_pct(&self) -> io::Result<u64> {
        parse_file(self.path.join("max_perf_pct"))
    }

    pub fn set_max_perf_pct(&mut self, value: u64) -> io::Result<()> {
        write_file(self.path.join("max_perf_pct"), format!("{}", value))
    }

    pub fn no_turbo(&self) -> io::Result<bool> {
        let value: u64 = parse_file(self.path.join("no_turbo"))?;
        Ok(value > 0)
    }

    pub fn set_no_turbo(&mut self, value: bool) -> io::Result<()> {
        write_file(self.path.join("no_turbo"), if value { "1" } else { "0" })
    }

    pub fn get_all_values(&self) -> io::Result<(u64, u64, bool)> {
        self.min_perf_pct().and_then(|min| {
            self.max_perf_pct().and_then(|max| {
                self.no_turbo().map(|no_turbo| (min, max, no_turbo))
            })
        })
    }
}
