// Copyright 2022 System76 <info@system76.com>
// SPDX-License-Identifier: GPL-3.0-only

//! For information about this sysfs kernel feature, see the following:
//!
//! - Platform Profile Selection:
//!  - <https://www.kernel.org/doc/html/latest/userspace-api/sysfs-platform_profile.html>
//! - Available Platform Profiles:
//!  - <https://mjmwired.net/kernel/Documentation/ABI/testing/sysfs-platform_profile>

use once_cell::sync::Lazy;
use std::{fs, path::Path};

const SYSFS_PATH: &str = "/sys/firmware/acpi/platform_profile";

/// Displays available ACPI platform profiles to choose from.
pub fn choices() -> impl Iterator<Item = &'static str> {
    static CHOICES: Lazy<Option<Box<[Box<str>]>>> = Lazy::new(|| {
        let path = concat_in_place::strcat!(SYSFS_PATH "_choices");
        let choices = std::fs::read_to_string(path).ok()?;
        Some(Box::from(choices.split_ascii_whitespace().map(Box::from).collect::<Vec<_>>()))
    });

    CHOICES.iter().flat_map(|array| array.iter()).map(Box::as_ref)
}

/// Checks if the system supports ACPI platform profiles.
#[must_use]
pub fn supported() -> bool { Path::new(SYSFS_PATH).exists() }

/// Applies the `low-power` or `quiet` ACPI platform profile.
pub fn battery() {
    let mut first_choice = None;

    for choice in choices() {
        if first_choice.is_none() {
            first_choice = Some(choice);
        }
        match choice {
            "low-power" | "quiet" => {
                apply_profile(choice);
                return;
            }

            _ => (),
        }
    }

    // First profile is a best choice option, if unknown.
    if let Some(choice) = first_choice {
        apply_profile(choice);
    }
}

/// Applies the balanced ACPI platform profile.
pub fn balanced() { apply_profile("balanced"); }

/// Applies the performance ACPI platform profile.
pub fn performance() { apply_profile("performance"); }

/// Applies the ACPI platform profile.
fn apply_profile(profile: &str) {
    if let Err(why) = fs::write(SYSFS_PATH, profile) {
        log::error!("ACPI Platform Profile: could not set to {}: {}", profile, why);
    }
}
