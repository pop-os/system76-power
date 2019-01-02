use super::*;
use std::io::Write;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ConfigBacklight {
    #[serde(rename = "backlight_keyboard")]
    pub keyboard: u8,
    #[serde(rename = "backlight_screen")]
    pub screen: u8
}

impl ConfigBacklight {
    pub(crate) fn battery() -> Self {
        Self { keyboard: 0, screen: 10 }
    }

    pub(crate) fn balanced() -> Self {
        Self { keyboard: 50, screen: 40 }
    }

    pub(crate) fn performance() -> Self {
        Self { keyboard: 100, screen: 100 }
    }

    pub(crate) fn serialize_toml(&self, out: &mut Vec<u8>) {
        let _ = writeln!(
            out,
            "backlight_keyboard = '{}'\n\
            backlight_screen = '{}'",
            self.keyboard,
            self.screen
        );
    }
}
