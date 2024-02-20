// Copyright 2018-2021 System76 <info@system76.com>
//
// SPDX-License-Identifier: GPL-3.0-only

use std::{io, process::Command};

pub fn reload(module: &str, options: &[&str]) -> io::Result<()> {
    unload(module).and_then(|()| load(module, options))
}

pub fn unload(module: &str) -> io::Result<()> {
    log::info!("Unloading module named {}", module);
    Command::new("modprobe").args(&["-r", module]).status().and_then(|stat| {
        if stat.success() {
            Ok(())
        } else {
            Err(io::Error::new(io::ErrorKind::Other, format!("failed to unload {}", module)))
        }
    })
}

pub fn load(module: &str, options: &[&str]) -> io::Result<()> {
    log::info!("Loading module named {} with options {:?}", module, options);
    Command::new("modprobe").arg(module).args(options).status().and_then(|stat| {
        if stat.success() {
            Ok(())
        } else {
            Err(io::Error::new(io::ErrorKind::Other, format!("failed to load {}", module)))
        }
    })
}
