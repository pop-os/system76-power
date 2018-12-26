#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ConfigPci {
    pub runtime_pm: bool
}

impl ConfigPci {
    pub fn battery() -> Self {
        Self {
            runtime_pm: true,
        }
    }

    pub fn balanced() -> Self {
        Self {
            runtime_pm: true,
        }
    }

    pub fn performance() -> Self {
        Self {
            runtime_pm: false
        }
    }
}
