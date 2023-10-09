// Copyright 2022 System76 <info@system76.com>
// SPDX-License-Identifier: GPL-3.0-only

use crate::{util::write_value, Profile};
use concat_in_place::strcat;
use std::{
    fmt::Write,
    fs::{self, File},
    io::Read,
};

pub fn set(profile: Profile, max_percent: u8) {
    let mut core = Cpu::new(0);

    let min_freq = core.frequency_minimum();
    let max_freq = core.frequency_maximum();

    if let Some(driver) = core.scaling_driver() {
        let is_amd_pstate = driver.starts_with("amd-pstate");

        // The profile for the `energy_performance_preference`.
        let mut epp = None;

        // Decide the scaling governor to use with this profile.
        let governor = match profile {
            // Prefer battery life over efficiency
            Profile::Battery => match driver {
                "amd-pstate" | "intel_pstate" => "powersave",
                "amd-pstate-epp" => {
                    epp = Some("balance_power");
                    "powersave"
                }
                _ => "conservative",
            },
            // The most energy-efficient profile
            Profile::Balanced => match driver {
                "amd-pstate" => "ondemand",
                "amd-pstate-epp" => {
                    epp = Some("balance_performance");
                    "powersave"
                }
                "intel_pstate" => "powersave",
                _ => "schedutil",
            },
            // Maximum performance
            Profile::Performance => {
                epp = (driver == "amd-pstate-epp").then_some("performance");
                "performance"
            }
        };

        if let Some((cpus, (min, max))) = num_cpus().zip(min_freq.zip(max_freq)) {
            let max = max * max_percent.min(100) as usize / 100;
            eprintln!("setting {} with max {}", governor, max);

            for cpu in 0..=cpus {
                core.load(cpu);

                if !is_amd_pstate {
                    core.set_frequency_minimum(min);
                    core.set_frequency_maximum(max);
                }

                core.set_governor(governor);

                if let Some(preference) = epp {
                    core.set_epp(preference);
                }
            }
        }
    }
}

pub struct Cpu {
    /// Stores the path of the file being accessed.
    path:        String,
    /// Know where to truncate the path.
    path_len:    usize,
    /// Scratch space for read files
    read_buffer: Vec<u8>,
}

impl Cpu {
    #[must_use]
    pub fn new(core: usize) -> Self {
        let mut path = String::with_capacity(38);
        cpu_path(&mut path, core);

        Self { path_len: path.len(), path, read_buffer: Vec::with_capacity(16) }
    }

    pub fn load(&mut self, core: usize) {
        self.path.clear();
        cpu_path(&mut self.path, core);
        self.path_len = self.path.len();
    }

    #[must_use]
    pub fn frequency_maximum(&mut self) -> Option<usize> {
        self.get_value("cpuinfo_max_freq").and_then(|value| value.parse::<usize>().ok())
    }

    #[must_use]
    pub fn frequency_minimum(&mut self) -> Option<usize> {
        self.get_value("cpuinfo_min_freq").and_then(|value| value.parse::<usize>().ok())
    }

    #[must_use]
    pub fn scaling_driver(&mut self) -> Option<&str> { self.get_value("scaling_driver") }

    pub fn set_epp(&mut self, preference: &str) {
        self.set_value("energy_performance_preference", preference);
    }

    pub fn set_frequency_maximum(&mut self, frequency: usize) {
        self.set_value("scaling_max_freq", frequency);
    }

    pub fn set_frequency_minimum(&mut self, frequency: usize) {
        self.set_value("scaling_min_freq", frequency);
    }

    pub fn set_governor(&mut self, governor: &str) { self.set_value("scaling_governor", governor); }

    fn set_value<V: std::fmt::Display>(&mut self, file: &str, value: V) {
        self.path.truncate(self.path_len);
        write_value(strcat!(&mut self.path, file), value);
    }

    fn get_value(&mut self, file: &str) -> Option<&str> {
        self.path.truncate(self.path_len);
        let mut file = match File::open(strcat!(&mut self.path, file)) {
            Ok(file) => file,
            Err(_) => return None,
        };

        self.read_buffer.clear();
        let _res = file.read_to_end(&mut self.read_buffer);

        std::str::from_utf8(&self.read_buffer).ok().map(str::trim)
    }
}

#[must_use]
pub fn num_cpus() -> Option<usize> {
    let info = fs::read_to_string("/sys/devices/system/cpu/possible").ok()?;
    info.split('-').nth(1)?.trim_end().parse::<usize>().ok()
}

fn cpu_path(buffer: &mut String, core: usize) {
    let _ = write!(buffer, "/sys/devices/system/cpu/cpu{}/cpufreq/", core);
}
