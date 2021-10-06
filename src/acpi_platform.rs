// Copyright 2018-2021 System76 <info@system76.com>
//
// SPDX-License-Identifier: GPL-3.0-only

//! For information about this sysfs kernel feature, see the following:
//!
//! - Platform Profile Selection:
//!  - https://www.kernel.org/doc/html/latest/userspace-api/sysfs-platform_profile.html
//! - Available Platform Profiles:
//!  - https://mjmwired.net/kernel/Documentation/ABI/testing/sysfs-platform_profile

use std::{fs, path::Path};

const SYSFS_PATH: &str = "/sys/firmware/acpi/platform_profile";

pub fn supported() -> bool { Path::new(SYSFS_PATH).exists() }

pub fn battery() {
    if let Err(why) = fs::write(SYSFS_PATH, "low-power") {
        eprintln!("ACPI Platform Profile: could not set to low-power: {}", why);
    }
}

pub fn balanced() {
    if let Err(why) = fs::write(SYSFS_PATH, "balanced-performance") {
        eprintln!("ACPI Platform Profile: could not set to balanced-performance: {}", why);
    }
}

pub fn performance() {
    if let Err(why) = fs::write(SYSFS_PATH, "performance") {
        eprintln!("ACPI Platform Profile: could not set to performance: {}", why);
    }
}
