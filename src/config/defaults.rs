use super::*;
use std::borrow::Cow;
use std::io::Write;

fn perf() -> Cow<'static, str> {
    "performance".into()
}

fn batt() -> Cow<'static, str> {
    "battery".into()
}

#[derive(Clone, Debug, Deserialize, Serialize, SmartDefault)]
pub struct ConfigDefaults {
    #[default = "perf()"]
    #[serde(default = "perf")]
    pub ac: Cow<'static, str>,

    #[default = "batt()"]
    #[serde(default = "batt")]
    pub battery: Cow<'static, str>,

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
             {}\n",
            comment_if_default(
                true,
                "ac",
                &defaults.ac,
                &self.ac,
                self.ac.as_ref()
            ),
            comment_if_default(
                true,
                "battery",
                &defaults.battery,
                &self.battery,
                self.battery.as_ref(),
            )
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
