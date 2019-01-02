use std::io::{self, Read, Write};
use std::fs::{self, File};
use std::borrow::Cow;
use std::path::Path;

use super::CONFIG_PARENT;

const STATE_CONFIG_PATH: &str = "/etc/system76-power/state.toml";

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, SmartDefault)]
pub struct ActiveState {
    #[default = "\"standard\".into()"]
    #[serde(default)]
    pub fan_curve: Cow<'static, str>,

    #[default = "\"balanced\".into()"]
    #[serde(default)]
    pub power_profile: Cow<'static, str>,
}

impl ActiveState {
    pub fn new() -> io::Result<Self> {
        let config_path = &Path::new(STATE_CONFIG_PATH);
        if !config_path.exists() {
            info!("config file does not exist at {}; creating it", STATE_CONFIG_PATH);
            let config = Self::default();

            if let Err(why) = config.write() {
                error!("failed to write config to file system: {}", why);
            }

            Ok(config)
        } else {
            Self::read()
        }
    }

    pub fn read() -> io::Result<Self> {
        let mut file = File::open(STATE_CONFIG_PATH)?;
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer)?;

        let config: ActiveState = ::toml::from_slice(&buffer).map_err(|why| {
            io::Error::new(
                io::ErrorKind::Other,
                format!("failed to deserialize active profiles: {}", why),
            )
        })?;

        Ok(config)
    }

    pub fn write(&self) -> io::Result<()> {
        let config_parent = &Path::new(CONFIG_PARENT);

        if !config_parent.exists() {
            fs::create_dir(config_parent)?;
        }

        let mut file = File::create(STATE_CONFIG_PATH)?;
        file.write_all(&self.serialize())?;

        Ok(())
    }

    /// Custom serialization to a more readable format.
    fn serialize(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(8 * 1024);
        {
            let out = &mut out;
            out.extend_from_slice(
                b"# This config is automatically generated and maintained by system76-power.\n\
                # Any changes to the formatting of this file will be lost when the daemon\n\
                # overwites this file on a profile change.\n\n"
            );

            writeln!(
                out,
                "# The power profile to set when starting the daemon. The default is 'balanced'.\n\
                power_profile = '{}'\n\n\
                # The fan curve profile to set when starting the daemon. The default is 'standard'.\n\
                fan_curve = '{}'",
                self.power_profile,
                self.fan_curve
            );
        }
        out
    }
}
