#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct AmdSettings {
    pub profile: &'static str,
    pub dpm_state: &'static str,
    pub dpm_perf: &'static str
}

impl AmdSettings {
    pub fn battery() -> Self {
        Self {
            profile: "low",
            dpm_state: "battery",
            dpm_perf: "low"
        }
    }

    pub fn balanced() -> Self {
        Self {
            profile: "auto",
            dpm_state: "performance",
            dpm_perf: "auto"
        }
    }

    pub fn performance() -> Self {
        Self {
            profile: "high",
            dpm_state: "performance",
            dpm_perf: "auto"
        }
    }
}
