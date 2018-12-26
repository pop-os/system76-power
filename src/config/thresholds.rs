use super::*;
use std::io::Write;

#[derive(Clone, Debug, Deserialize, Serialize, SmartDefault)]
pub struct ConfigThresholds {
    #[serde(default)]
    #[default = "25"]
    pub critical: u8,

    #[serde(default)]
    #[default = "50"]
    pub normal: u8,
}

impl ConfigThresholds {
    pub(crate) fn serialize_toml(&self, out: &mut Vec<u8>) {
        let default = Self::default();
        let disabled = default.critical == self.critical && default.normal == self.normal;
        let _ = writeln!(
            out,
            "{}[threshold]\n\
             # Defines what percentage of battery is required to set the profile to 'battery'.\n\
             {}\n\n\
             # Defines what percentage of battery is required to revert the critical change.\n\
             {}\n",
            if disabled { "# " } else { "" },
            comment_if_default(
                false,
                "critical",
                default.critical,
                self.critical,
                &self.critical.to_string()
            ),
            comment_if_default(
                false,
                "normal",
                default.normal,
                self.normal,
                &self.normal.to_string()
            )
        );
    }
}
