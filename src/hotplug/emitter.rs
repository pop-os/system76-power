// Copyright 2023 System76 <info@system76.com>
// SPDX-License-Identifier: GPL-3.0-only

use super::{
    mux::{self, DisplayPortMux},
    Detect, HotPlugDetect,
};

pub struct Emitter {
    last_detection: [bool; 4],
    last_result:    super::Result<HotPlugDetect>,
    mux_result:     super::Result<DisplayPortMux>,
}

impl Emitter {
    #[must_use]
    pub fn new(nvidia_device_id: Option<String>) -> Self {
        let mut emitter = Self {
            last_result:    unsafe { HotPlugDetect::new(nvidia_device_id) },
            mux_result:     unsafe { mux::DisplayPortMux::new() },
            last_detection: [false; 4],
        };

        emitter.last_detection = emitter.detect();
        emitter
    }

    pub fn emit_on_detect(&mut self) -> impl Iterator<Item = usize> + '_ {
        let hotplug_detect = self.detect();
        let last_detection = self.last_detection;
        self.last_detection = hotplug_detect;

        (0..hotplug_detect.len()).filter(move |i| {
            let i = *i;
            hotplug_detect[i] != last_detection[i] && hotplug_detect[i]
        })
    }

    pub fn detect(&mut self) -> [bool; 4] {
        if let Ok(ref mut hotplug_detect) = self.last_result {
            unsafe { hotplug_detect.detect() }
        } else {
            [false; 4]
        }
    }

    pub fn mux_step(&self) {
        if let Ok(ref mux) = self.mux_result {
            unsafe {
                mux.step();
            }
        }
    }
}
