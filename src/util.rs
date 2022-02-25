// Copyright 2018-2022 System76 <info@system76.com>
//
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    fmt::Display,
    fs::{DirEntry, File},
    io::{self, Write},
    path::Path,
};

pub fn entries<T, F: FnMut(DirEntry) -> T>(path: &Path, mut func: F) -> io::Result<Vec<T>> {
    let mut ret = Vec::new();
    for entry_res in path.read_dir()? {
        ret.push(func(entry_res?));
    }

    Ok(ret)
}

/// Write a value that implements `Display` to a file
pub fn write_value<V: Display>(path: &str, value: V) {
    // eprintln!("writing {} to {}", value, path);
    let write_to_file = |path, value| -> io::Result<()> {
        let mut file = File::create(path)?;
        write!(file, "{}", value)?;

        Ok(())
    };

    if let Err(why) = write_to_file(path, value) {
        eprintln!("failed to set value in {}: {}", path, why);
    }
}
