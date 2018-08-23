use std::io;
use std::path::PathBuf;
use std::sync::Once;
use util::{parse_file, write_file};

// use once_cell::sync::OnceCell;
// pub static ORIGINAL_PSTATE: OnceCell<Option<PStateValues>> = OnceCell::INIT;

// The PState values that were initially set at the time of starting the daemon.
static mut ORIGINAL_PSTATE_: Option<PStateValues> = None;
pub static ORIGINAL_PSTATE: Once = Once::new();

pub fn get_original_pstate() -> Option<&'static PStateValues> {
    unsafe {
        ORIGINAL_PSTATE.call_once(|| {
            ORIGINAL_PSTATE_ = PStateValues::current();
        });

        ORIGINAL_PSTATE_.as_ref()
    }
}

pub fn set_original_pstate() {
    if let Some(ref original) = get_original_pstate() {
        eprintln!("setting original pstate values: {:?}", original);
        if let Err(why) = original.set() {
            eprintln!("failed to set original pstate values: {:?}", why);
        }
    }
}

#[derive(Clone, Debug)]
pub struct PStateValues {
    min_perf_pct: u64,
    max_perf_pct: u64,
    no_turbo: bool
}

impl PStateValues {
    pub fn current() -> Option<PStateValues> {
        PState::new().ok().and_then(|pstate| Some(Self {
            min_perf_pct: pstate.min_perf_pct().ok()?,
            max_perf_pct: pstate.max_perf_pct().ok()?,
            no_turbo: pstate.no_turbo().ok()?
        }))
    }

    pub fn set(&self) -> io::Result<()> {
        PState::new().and_then(|mut pstate| {
            pstate.set_min_perf_pct(self.min_perf_pct)?;
            pstate.set_max_perf_pct(self.max_perf_pct)?;
            pstate.set_no_turbo(self.no_turbo)
        })
    }
}

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
}
