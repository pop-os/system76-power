// Copyright 2018-2021 System76 <info@system76.com>
//
// SPDX-License-Identifier: GPL-3.0-only

use std::{fs::write, io, path::PathBuf};

pub struct PciBus {
    path: PathBuf,
}

impl PciBus {
    pub fn new() -> io::Result<Self> {
        let path = PathBuf::from("/sys/bus/pci");
        if path.is_dir() {
            Ok(Self { path })
        } else {
            Err(io::Error::new(io::ErrorKind::NotFound, "pci directory not found"))
        }
    }

    pub fn rescan(&self) -> io::Result<()> { write(self.path.join("rescan"), "1") }
}
