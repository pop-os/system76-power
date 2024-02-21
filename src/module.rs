// Copyright 2018-2021 System76 <info@system76.com>
//
// SPDX-License-Identifier: GPL-3.0-only

use std::{fs::read_to_string, io};

pub struct Module {
    pub name: String,
}

impl Module {
    pub fn all() -> io::Result<Vec<Self>> {
        read_to_string("/proc/modules")?.lines().map(parse).collect()
    }
}

fn parse(line: &str) -> io::Result<Module> {
    let name = line
        .split(' ')
        .next()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "module name not found"))?
        .to_string();

    Ok(Module { name })
}
