use pstate::PStateValues;
use std::io::Write;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ConfigPState {
    #[serde(rename = "pstate_min")]
    pub min: u8,
    #[serde(rename = "pstate_max")]
    pub max: u8,
    #[serde(rename = "pstate_murbo")]
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
            "pstate_min = {}\n\
            pstate_max = {}\n\
            pstate_turbo = {}",
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
