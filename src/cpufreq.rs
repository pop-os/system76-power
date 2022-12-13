// Copyright 2022 System76 <info@system76.com>
// SPDX-License-Identifier: GPL-3.0-only

use crate::{util::write_value, Profile};
use std::fs;

pub fn set(profile: Profile, max_percent: u8) {
    if let Some(driver) = scaling_driver(0) {
        let governor = if "intel_pstate" == driver.as_str() {
            match profile {
                Profile::Battery | Profile::Balanced => "powersave",
                Profile::Performance => "performance",
            }
        } else {
            match profile {
                Profile::Battery => "conservative",
                Profile::Balanced => "schedutil",
                Profile::Performance => "performance",
            }
        };

        if let Some((cpus, (min, max))) =
            num_cpus().zip(frequency_minimum().zip(frequency_maximum()))
        {
            let max = max * max_percent.min(100) as usize / 100;
            eprintln!("setting {} with max {}", governor, max);

            for cpu in 0..=cpus {
                set_frequency_minimum(cpu, min);
                set_frequency_maximum(cpu, max);
                set_governor(cpu, governor);
            }
        }
    }
}

#[must_use]
pub fn num_cpus() -> Option<usize> {
    let info = fs::read_to_string("/sys/devices/system/cpu/possible").ok()?;
    
    info.split('-').nth(1)?.trim_end().parse().ok()
}

#[must_use]
pub fn frequency_maximum() -> Option<usize> {
    let path = sys_path(0, "cpuinfo_max_freq");
    let string = fs::read_to_string(path).ok()?;
    string.trim_end().parse().ok()
}

#[must_use]
pub fn frequency_minimum() -> Option<usize> {
    let path = sys_path(0, "cpuinfo_min_freq");
    let string = fs::read_to_string(path).ok()?;

    string.trim_end().parse().ok()
}

#[must_use]
pub fn scaling_driver(core: usize) -> Option<String> {
    let path = sys_path(core, "scaling_driver");
    fs::read_to_string(&path)
        .map(trim_end_in_place)
        .ok()
}

pub fn set_frequency_maximum(core: usize, frequency: usize) {
    let path = sys_path(core, "scaling_max_freq");

    write_value(&path, frequency);
}

pub fn set_frequency_minimum(core: usize, frequency: usize) {
    let path = sys_path(core, "scaling_min_freq");
    
    write_value(&path, frequency);
}

pub fn set_governor(core: usize, governor: &str) {
    let path = sys_path(core, "scaling_governor");
    write_value(&path, governor);
}

#[must_use]
fn trim_end_in_place(mut string: String) -> String {
    let new_length = string.trim_end().len();
    
    string.truncate(new_length);

    string
}

fn sys_path(core: usize, subpath: &str) -> String { format!("/sys/devices/system/cpu/cpu{core}/cpufreq/{subpath}") }