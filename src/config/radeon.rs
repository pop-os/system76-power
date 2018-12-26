#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ConfigRadeon {
    pub profile: String,
    pub dpm_state: String,
    pub dpm_perf: String
}

impl ConfigRadeon {
    pub fn battery() -> Self {
        Self {
            profile: "low".into(),
            dpm_state: "battery".into(),
            dpm_perf: "low".into()
        }
    }

    pub fn balanced() -> Self {
        Self {
            profile: "auto".into(),
            dpm_state: "performance".into(),
            dpm_perf: "auto".into()
        }
    }

    pub fn performance() -> Self {
        Self {
            profile: "high".into(),
            dpm_state: "performance".into(),
            dpm_perf: "auto".into()
        }
    }
}
