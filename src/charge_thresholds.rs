use std::{fs, path::Path};

use crate::err_str;

const START_THRESHOLD: &str = "/sys/class/power_supply/BAT0/charge_control_start_threshold";
const END_THRESHOLD: &str = "/sys/class/power_supply/BAT0/charge_control_end_threshold";
const UNSUPPORTED_ERROR: &str = "Not running System76 firmware with charge threshold suppport";
const OUT_OF_RANGE_ERROR: &str = "Charge threshold out of range: should be 0-100";
const ORDER_ERROR: &str = "Charge end threshold must be strictly greater than start";

fn is_s76_ec() -> bool {
    // For now, only support thresholds on System76 hardware
    Path::new("/sys/bus/acpi/devices/17761776:00").is_dir()
}

fn supports_thresholds() -> bool {
    Path::new(START_THRESHOLD).exists() && Path::new(END_THRESHOLD).exists()
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
