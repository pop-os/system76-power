use super::*;
use std::io::Write;

fn perf() -> ProfileKind {
    ProfileKind::Performance
}

fn batt() -> ProfileKind {
    ProfileKind::Battery
}

fn bala() -> ProfileKind {
    ProfileKind::Balanced
}

#[derive(Clone, Debug, Deserialize, Serialize, SmartDefault)]
pub struct ConfigDefaults {
    #[default = "ProfileKind::Performance"]
    #[serde(default = "perf")]
    pub ac: ProfileKind,

    #[default = "ProfileKind::Battery"]
    #[serde(default = "batt")]
    pub battery: ProfileKind,

    #[default = "ProfileKind::Balanced"]
    #[serde(default = "bala")]
    pub last_profile: ProfileKind,

    #[serde(default)]
    pub experimental: bool,
}

impl ConfigDefaults {
    pub(crate) fn serialize_toml(&self, out: &mut Vec<u8>) {
        let defaults = Self::default();
        let _ = writeln!(
            out,
            "# The default profile that will be set on connecting to AC.\n\
             {}\n\n\
             # The default profile that will be set on disconnecting from AC.\n\
             {}\n\n\
             # The last profile that was activated\n\
             last_profile = {}\n",
            if let ProfileKind::Custom(_) = self.ac {
                format!("{{ custom = '{}' }}", <&str>::from(&self.ac))
            } else {
                comment_if_default(
                    true,
                    "ac",
                    &defaults.ac,
                    &self.ac,
                    <&str>::from(&self.ac)
                )
            },
            if let ProfileKind::Custom(_) = self.battery {
                format!("{{ custom = '{}' }}", <&str>::from(&self.ac))
            } else {
                comment_if_default(
                    true,
                    "battery",
                    &defaults.battery,
                    &self.battery,
                    <&str>::from(&self.battery)
                )
            },
            if let ProfileKind::Custom(_) = self.last_profile {
                format!("{{ custom = '{}' }}", <&str>::from(&self.last_profile))
            } else {
                format!("'{}'", <&str>::from(&self.last_profile))
            }
        );

        let exp: &[u8] = if self.experimental {
            b"# Uncomment to enable extra untested power-saving features\n\
            experimental = true\n\n"
        } else {
            b"# Uncomment to enable extra untested power-saving features\n\
            # experimental = true\n\n"
        };

        out.extend_from_slice(exp);
    }
}