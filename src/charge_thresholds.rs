// Copyright 2018-2021 System76 <info@system76.com>
//
// SPDX-License-Identifier: GPL-3.0-only

use std::{fs, path::Path};
use system76_power_zbus::ChargeProfile;

const START_THRESHOLD: &str = "/sys/class/power_supply/BAT0/charge_control_start_threshold";
const END_THRESHOLD: &str = "/sys/class/power_supply/BAT0/charge_control_end_threshold";
const UNSUPPORTED_ERROR: &str = "Not running System76 firmware with charge threshold support";
const OUT_OF_RANGE_ERROR: &str = "Charge threshold out of range: should be 0-100";
const ORDER_ERROR: &str = "Charge end threshold must be strictly greater than start";

fn is_supported() -> bool {
    // For now, only support thresholds on System76 hardware
    Path::new("/sys/bus/acpi/devices/17761776:00").is_dir() ||
    // and Huawei
    Path::new("/sys/devices/platform/huawei-wmi/charge_control_thresholds").exists()
}

fn supports_thresholds() -> bool {
    Path::new(START_THRESHOLD).exists() && Path::new(END_THRESHOLD).exists()
}

#[must_use]
pub fn get_charge_profiles() -> Vec<ChargeProfile> {
    vec![
        ChargeProfile {
            id:          "full_charge".to_string(),
            title:       "Full Charge".to_string(),
            description: "Battery is charged to its full capacity for the longest possible use on \
                          battery power. Charging resumes when the battery falls below 96% charge."
                .to_string(),
            start:       90,
            end:         100,
        },
        ChargeProfile {
            id:          "balanced".to_string(),
            title:       "Balanced".to_string(),
            description: "Use this threshold when you unplug frequently but don't need the full \
                          battery capacity. Charging stops when the battery reaches 90% capacity \
                          and resumes when the battery falls below 85%."
                .to_string(),
            start:       86,
            end:         90,
        },
        ChargeProfile {
            id:          "max_lifespan".to_string(),
            title:       "Maximum Lifespan".to_string(),
            description: "Use this threshold if you rarely use the system on battery for extended \
                          periods. Charging stops when the battery reaches 60% capacity and \
                          resumes when the battery falls below 50%."
                .to_string(),
            start:       50,
            end:         60,
        },
    ]
}

pub(crate) fn get_charge_thresholds() -> anyhow::Result<(u8, u8)> {
    if !is_supported() || !supports_thresholds() {
        return Err(anyhow::anyhow!(UNSUPPORTED_ERROR));
    }

    let start_str = fs::read_to_string(START_THRESHOLD)?;
    let end_str = fs::read_to_string(END_THRESHOLD)?;

    let start = start_str.trim().parse::<u8>()?;
    let end = end_str.trim().parse::<u8>()?;

    Ok((start, end))
}

pub(crate) fn set_charge_thresholds((start, end): (u8, u8)) -> anyhow::Result<()> {
    if !is_supported() || !supports_thresholds() {
        return Err(anyhow::anyhow!(UNSUPPORTED_ERROR));
    } else if start > 100 || end > 100 {
        return Err(anyhow::anyhow!(OUT_OF_RANGE_ERROR));
    } else if end <= start {
        return Err(anyhow::anyhow!(ORDER_ERROR));
    }

    // Without this, setting start threshold may fail if the previous end
    // threshold is higher.
    fs::write(END_THRESHOLD, "100")?;

    fs::write(START_THRESHOLD, format!("{}", start))?;
    fs::write(END_THRESHOLD, format!("{}", end))?;

    Ok(())
}
