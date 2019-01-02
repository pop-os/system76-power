use super::*;
use std::borrow::Cow;
use std::collections::HashMap;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, SmartDefault)]
pub struct Profiles {

    #[default = "Profile::battery()"]
    #[serde(default)]
    pub battery: Profile,

    #[default = "Profile::balanced()"]
    #[serde(default)]
    pub balanced: Profile,

    #[default = "Profile::performance()"]
    #[serde(default)]
    pub performance: Profile,

    #[serde(flatten)]
    #[serde(default)]
    pub custom: HashMap<String, Profile>
}

impl Profiles {
    pub fn get(&self, profile: &str) -> &Profile {
        match profile {
            "battery" => &self.battery,
            "balanced" => &self.balanced,
            "performance" => &self.performance,
            other => match self.custom.get(other) {
                Some(profile) => profile,
                None => {
                    error!("power profile '{}' not found -- using 'balanced'", other);
                    &self.balanced
                }
            }
        }
    }

    pub fn get_profiles<'a>(&'a self) -> Box<Iterator<Item = &'a str> + 'a> {
        use std::iter;

        Box::new(
            iter::once("battery")
                .chain(iter::once("balanced"))
                .chain(iter::once("performance"))
                .chain(self.custom.keys().map(|x| x.as_ref()))
        )
    }

    /// Fix missing data in default profiles.
    pub(crate) fn repair(&mut self) {
        use std::iter;

        let profiles = iter::once((&mut self.battery, Profile::battery()))
            .chain(iter::once((&mut self.balanced, Profile::balanced())))
            .chain(iter::once((&mut self.performance, Profile::performance())));

        macro_rules! validate {
            ($profile:ident, $default:ident, opt $field:ident) => (
                if $profile.$field.is_none() {
                    $profile.$field = $default.$field.take();
                }
            );

            ($profile:ident, $default:ident, int $field:ident) => (
                if $profile.$field == 0 {
                    $profile.$field = $default.$field;
                }
            );

            ($profile:ident, $default:ident { $($kind:tt $field:ident),* }) => (
                $(validate!($profile, $default, $kind $field);)*
            );
        }

        for (profile, mut default) in profiles {
            validate!(profile, default {
                opt backlight,
                int laptop_mode,
                int max_lost_work,
                opt pci,
                opt pstate,
                opt graphics
            });
        }
    }

    pub(crate) fn serialize_toml(&self, out: &mut Vec<u8>) {
        fn set_or_default(out: &mut Vec<u8>, profile: &str, current: &Profile, default: &Profile, document: bool) {
            macro_rules! document {
                ($description:expr) => (
                    if document { $description } else { "" }
                )
            }

            if current != default {
                out.extend_from_slice(format!("[profiles.{}]\n", profile).as_bytes());
                current.serialize_toml(out);
            } else {
                let backlight = default.backlight.as_ref().unwrap();
                let pstate = default.pstate.as_ref().unwrap();
                out.extend_from_slice(
                    format!(
                        "# [profiles.{}]\n\
                         {}# backlight_keyboard = {}\n\
                         {}# backlight_screen = {}\n\
                         {}# laptop_mode = {}\n\
                         {}# max_lost_work = {}\n\
                         {}# pci_runtime_pm = {}\n\
                         {}# pstate_min = {}\n\
                         {}# pstate_max = {}\n\
                         {}# pstate_turbo = {}\n\
                         {}# graphics = '{}'\n\n",
                         profile,
                         document!("# Set the backlight brightness for each keyboard.\n#\n"),
                         backlight.keyboard,
                         document!("\n# Set the backlight brightness for each screen.\n#\n"),
                         backlight.screen,
                         document!("\n# Enables laptop mode in the kernel if greater than 0.\n\
                            # Laptop mode schedules and batches disk I/O requests to keep\n\
                            # the system in a low power state for greater periods of time.\n#\n"),
                         default.laptop_mode,
                         document!(
                             "\n# Configures the kernel to keep up to N seconds of state stored in memory\n\
                             # before writing it to the disk. This means that sudden power loss could lose\n\
                             # up to N seconds of work, but power is saved by batching writes together.\n#\n"
                         ),
                         default.max_lost_work,
                         document!("\n# Configure runtime power management for PCI devices.\n#\n"),
                         default.pci.as_ref().unwrap().runtime_pm,
                         document!("\n# The minimum clock speed of an Intel CPU, as a percent.\n#\n"),
                         pstate.min,
                         document!("\n# The maximum clock speed of an Intel CPU, as a percent.\n#\n"),
                         pstate.max,
                         document!("\n# Whether an Intel CPU should have turbo enabled or disabled.\n#\n"),
                         pstate.turbo,
                         document!("\n# Set a power profile for graphics cards.\n#\n"),
                         default.graphics.as_ref().unwrap()
                    ).as_bytes()
                )
            }
        }

        set_or_default(out, "battery", &self.battery, &Profile::battery(), true);
        set_or_default(out, "balanced", &self.balanced, &Profile::balanced(), false);
        set_or_default(out, "performance", &self.performance, &Profile::performance(), false);

        for (key, value) in &self.custom {
            out.extend_from_slice(format!("[profiles.{}]\n", key).as_bytes());
            value.serialize_toml(out);
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, SmartDefault)]
pub struct Profile {
    #[serde(flatten)]
    pub backlight: Option<ConfigBacklight>,
    #[serde(default)]
    pub laptop_mode: u8,
    #[serde(default)]
    pub max_lost_work: u32,
    #[serde(flatten)]
    pub pci: Option<ConfigPci>,
    #[serde(flatten)]
    pub pstate: Option<ConfigPState>,
    #[serde(default)]
    pub graphics: Option<Cow<'static, str>>
}

impl Profile {
    pub(crate) fn battery() -> Self {
        Self {
            backlight: Some(ConfigBacklight::battery()),
            laptop_mode: 2,
            max_lost_work: 15,
            pci: Some(ConfigPci::battery()),
            pstate: Some(ConfigPState::battery()),
            graphics: Some("low".into())
        }
    }

    pub(crate) fn balanced() -> Self {
        Self {
            backlight: Some(ConfigBacklight::balanced()),
            laptop_mode: 0,
            max_lost_work: 15,
            pci: Some(ConfigPci::balanced()),
            pstate: Some(ConfigPState::balanced()),
            graphics: Some("balanced".into())
        }
    }

    pub(crate) fn performance() -> Self {
        Self {
            backlight: Some(ConfigBacklight::performance()),
            laptop_mode: 0,
            max_lost_work: 15,
            pci: Some(ConfigPci::performance()),
            pstate: Some(ConfigPState::performance()),
            graphics: Some("performance".into())
        }
    }

    pub(crate) fn serialize_toml(&self, out: &mut Vec<u8>) {
        if let Some(ref backlight) = self.backlight {
            backlight.serialize_toml(out);
        }

        if let Some(ref pstate) = self.pstate {
            pstate.serialize_toml(out);
        }

        out.push(b'\n');
    }
}
