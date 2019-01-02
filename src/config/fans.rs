use fan::FanCurve;
use std::borrow::Cow;
use std::collections::HashMap;
use std::io::Write;
use std::iter;
use std::str;

fn standard() -> Cow<'static, str> {
    "standard".into()
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, SmartDefault)]
pub struct ConfigFans {
    #[default = "FanCurve::standard()"]
    #[serde(default = "FanCurve::standard")]
    pub standard: FanCurve,

    #[serde(flatten)]
    #[serde(default)]
    pub custom: HashMap<String, FanCurve>
}

impl ConfigFans {
    pub fn get(&self, profile: &str) -> Option<&FanCurve> {
        match profile {
            "standard" => Some(&self.standard),
            other => self.custom.get(other)
        }
    }

    pub fn get_profiles<'a>(&'a self) -> Box<Iterator<Item = &'a str> + 'a> {
        Box::new(iter::once("standard").chain(self.custom.keys().map(|x| x.as_str())))
    }

    pub(crate) fn serialize_toml(&self, out: &mut Vec<u8>) {
        let defaults = Self::default();

        let curves = iter::once(("standard", &self.standard))
            .chain(self.custom.iter().map(|(k, v)| (k.as_str(), v)));

        let mut values = String::new();
        let default_is_standard = defaults.standard == self.standard;

        for (name, curve) in curves {
            let should_comment = name == "standard" && default_is_standard;
            let comment = if should_comment { "# " } else { "" };
            toml_curve(&mut values, name, curve, comment);
        }

        let comment = if default_is_standard && self.custom.is_empty() { "# " } else { "" };

        let _ = write!(
            out,
            "# Configurations for available fan curve profiles.\n\
            #\n\
            # A curve is defined as a collection of points, each point containing a:\n\
            #   - `temp`: System temperature, in hundredths of a degree.\n\
            #   - `duty`: Fan speed, in hundredths of a percent.\n\
            {}[fan_curves]\n\
            # The default fan curve.\n\
            {}",
            comment,
            values
        );
    }
}

fn toml_curve(out: &mut String, name: &str, curve: &FanCurve, maybe_comment: &str) {
    let mut curves = curve.points.iter();

    let indent = str::repeat(" ", name.len() + 5);

    if let Some(curve) = curves.next() {
        out.push_str(&format!(
            "{}{} = [ {{ temp = {}, duty = {} }}",
            maybe_comment,
            name,
            curve.temp,
            curve.duty
        ));

        for curve in curves {
            out.push_str(&format!(
                ",\n{}{}{{ temp = {}, duty = {} }}",
                maybe_comment,
                indent,
                curve.temp,
                curve.duty
            ));
        }

        out.push_str(" ]\n\n");
    }
}
