use pstate::PStateValues;
use std::io::Write;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ConfigPState {
    pub min: u8,
    pub max: u8,
    pub turbo: bool,
}

impl ConfigPState {
    pub fn new(min: u8, max: u8, turbo: bool) -> Self {
        Self { min, max, turbo }
    }

    pub(crate) fn battery() -> Self {
        Self {
            min: 0,
            max: 50,
            turbo: false,
        }
    }

    pub(crate) fn balanced() -> Self {
        Self {
            min: 0,
            max: 100,
            turbo: true,
        }
    }

    pub(crate) fn performance() -> Self {
        Self {
            min: 50,
            max: 100,
            turbo: true,
        }
    }

    pub(crate) fn serialize_toml(&self, out: &mut Vec<u8>) {
        let _ = writeln!(
            out,
            "pstate = {{ min = {}, max = {}, turbo = {} }}",
            self.min, self.max, self.turbo
        );
    }
}

impl Into<PStateValues> for ConfigPState {
    fn into(self) -> PStateValues {
        PStateValues {
            min_perf_pct: self.min,
            max_perf_pct: self.max,
            no_turbo: self.turbo,
        }
    }
}
