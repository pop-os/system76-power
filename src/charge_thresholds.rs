use dbus::{
    arg::{
        Append,
        Arg,
        ArgType,
        cast,
        Get,
        Iter,
        IterAppend,
        RefArg,
        Variant,
    },
    strings::Signature,
};
use std::{
    collections::HashMap,
    fs,
    path::Path,
};

use crate::err_str;

const START_THRESHOLD: &str = "/sys/class/power_supply/BAT0/charge_control_start_threshold";
const END_THRESHOLD: &str = "/sys/class/power_supply/BAT0/charge_control_end_threshold";
const UNSUPPORTED_ERROR: &str = "Not running System76 firmware with charge threshold suppport";
const OUT_OF_RANGE_ERROR: &str = "Charge threshold out of range: should be 0-100";
const ORDER_ERROR: &str = "Charge end threshold must be strictly greater than start";

#[derive(Debug)]
pub struct ChargeProfile {
    pub id: String,
    pub title: String,
    pub description: String,
    pub start: u8,
    pub end: u8,
}

type DbusChargeProfile<'a> = HashMap<&'a str, Variant<Box<dyn RefArg>>>;

impl ChargeProfile {
    fn to_dbus(&self) -> DbusChargeProfile<'static> {
        let mut map: DbusChargeProfile = HashMap::new();
        map.insert("id", Variant(Box::new(self.id.clone())));
        map.insert("title", Variant(Box::new(self.title.clone())));
        map.insert("description", Variant(Box::new(self.description.clone())));
        map.insert("start", Variant(Box::new(self.start)));
        map.insert("end", Variant(Box::new(self.end)));
        map
    }

    fn from_dbus(map: &DbusChargeProfile) -> Option<Self> {
        type RefVariant = Variant<Box<dyn RefArg>>;
        Some(Self {
            id: map.get("id")?.as_str()?.to_string(),
            title: map.get("title")?.as_str()?.to_string(),
            description: map.get("description")?.as_str()?.to_string(),
            start: *cast(&cast::<RefVariant>(map.get("start")?)?.0)?,
            end: *cast(&cast::<RefVariant>(map.get("end")?)?.0)?,
        })
    }
}

impl Arg for ChargeProfile {
    const ARG_TYPE: ArgType = DbusChargeProfile::ARG_TYPE;

    fn signature() -> Signature<'static> {
        DbusChargeProfile::signature()
    }
}

impl Append for ChargeProfile {
    fn append_by_ref(&self, i: &mut IterAppend) {
        self.to_dbus().append_by_ref(i);
    }
}

impl<'a> Get<'a> for ChargeProfile {
    fn get(i: &mut Iter<'a>) -> Option<Self> {
        let map: DbusChargeProfile = i.get()?;
        Self::from_dbus(&map)
    }
}

fn is_s76_ec() -> bool {
    // For now, only support thresholds on System76 hardware
    Path::new("/sys/bus/acpi/devices/17761776:00").is_dir()
}

fn supports_thresholds() -> bool {
    Path::new(START_THRESHOLD).exists() && Path::new(END_THRESHOLD).exists()
}

pub fn get_charge_profiles() -> Vec<ChargeProfile> {
    vec![
        ChargeProfile {
            id: "full_charge".to_string(),
            title: "Full Charge".to_string(),
            description: "Battery is charged to its full capacity for the longest possible use on battery power. Charging resumes when the battery falls below 96% charge.".to_string(),
            start: 96,
            end: 100,
        },
        ChargeProfile {
            id: "balanced".to_string(),
            title: "Balanced".to_string(),
            description: "Use this threshold when you unplug frequently but don't need the full battery capacity. Charging stops when the battery reaches 90% capacity and resumes when the battery falls below 85%.".to_string(),
            start: 86,
            end: 90,
        },
        ChargeProfile {
            id: "max_lifespan".to_string(),
            title: "Maximum Lifespan".to_string(),
            description: "Use this threshold if you rarely use the system on battery for extended periods. Charging stops when the battery reaches 60% capacity and resumes when the battery falls below 50%.".to_string(),
            start: 50,
            end: 60,
        },
    ]
}

pub(crate) fn get_charge_thresholds() -> Result<(u8, u8), String> {
    if !is_s76_ec() || !supports_thresholds() {
        return Err(UNSUPPORTED_ERROR.to_string());
    }

    let start_str = fs::read_to_string(START_THRESHOLD).map_err(err_str)?;
    let end_str = fs::read_to_string(END_THRESHOLD).map_err(err_str)?;

    let start = u8::from_str_radix(start_str.trim(), 10).map_err(err_str)?;
    let end = u8::from_str_radix(end_str.trim(), 10).map_err(err_str)?;

    Ok((start, end))
}

pub(crate) fn set_charge_thresholds((start, end): (u8, u8)) -> Result<(), String> {
    if !is_s76_ec() || !supports_thresholds() {
        return Err(UNSUPPORTED_ERROR.to_string());
    } else if start > 100 || end > 100 {
        return Err(OUT_OF_RANGE_ERROR.to_string());
    } else if end <= start {
        return Err(ORDER_ERROR.to_string());
    }

    // Without this, setting start threshold may fail if the previous end
    // threshold is higher.
    fs::write(END_THRESHOLD, "100").map_err(err_str)?;

    fs::write(START_THRESHOLD, format!("{}", start)).map_err(err_str)?;
    fs::write(END_THRESHOLD, format!("{}", end)).map_err(err_str)?;

    Ok(())
}
