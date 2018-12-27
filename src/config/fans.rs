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
    #[default = "standard()"]
    #[serde(default = "standard")]
    pub active: Cow<'static, str>,

    #[default = "FanCurve::standard()"]
    #[serde(default = "FanCurve::standard")]
    pub standard: FanCurve,

    #[serde(flatten)]
    #[serde(default)]
    pub custom: HashMap<String, FanCurve>
}

impl ConfigFans {
    pub fn get_active(&self) -> FanCurve {
        match self.active.as_ref() {
            "standard" => self.standard.clone(),
            other => self.custom.get(other)
                .cloned()
                .unwrap_or_else(|| self.standard.clone())
        }
    }

    pub(crate) fn serialize_toml(&self, out: &mut Vec<u8>) {
        let defaults = Self::default();

        let curves = iter::once(("standard", &self.standard))
            .chain(self.custom.iter().map(|(k, v)| (k.as_str(), v)));

        let mut values = String::new();

        for (name, curve) in curves {
            let should_comment = name == "standard" && curve == &defaults.standard;
            let comment = if should_comment { "# " } else { "" };
            toml_curve(&mut values, name, curve, comment);
        }

        let _ = write!(
            out,
            "# Settings for controlling fan curves\n\
             [fan_curves]\n\
             # The fan curve to set when starting the daemon. Default is 'standard'.\n\
             active = '{}'\n\n\
             # The default fan curve.\n\
             {}",
             self.active.as_ref(),
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
