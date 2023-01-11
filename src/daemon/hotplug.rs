// Copyright 2023 System76 <info@system76.com>
// SPDX-License-Identifier: GPL-3.0-only

use crate::{
    hotplug::{
        self,
        mux::{self, DisplayPortMux},
        Detect, HotPlugDetect,
    },
    DBUS_NAME, DBUS_PATH,
};
use dbus::{channel::Sender, nonblock::SyncConnection, Message};

pub struct Emitter {
    last_detection: [bool; 4],
    last_result:    hotplug::Result<HotPlugDetect>,
    mux_result:     hotplug::Result<DisplayPortMux>,
}

impl Emitter {
    pub fn new(nvidia_device_id: Option<String>) -> Self {
        let mut emitter = Self {
            last_result:    unsafe { HotPlugDetect::new(nvidia_device_id) },
            mux_result:     unsafe { mux::DisplayPortMux::new() },
            last_detection: [false; 4],
        };

        emitter.last_detection = emitter.detect();
        emitter
    }

    pub fn emit_on_detect(&mut self, c: &SyncConnection) {
        let hotplug_detect = self.detect();
        #[allow(clippy::needless_range_loop)]
        for i in 0..hotplug_detect.len() {
            if hotplug_detect[i] != self.last_detection[i] && hotplug_detect[i] {
                log::info!("HotPlugDetect {}", i);
                let result = c.send(
                    Message::new_signal(DBUS_PATH, DBUS_NAME, "HotPlugDetect")
                        .unwrap()
                        .append1(i as u64),
                );

                if result.is_err() {
                    log::error!("failed to send HotPlugDetect signal");
                }
            }
        }

        self.last_detection = hotplug_detect;
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
