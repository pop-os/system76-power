// Copyright 2018-2021 System76 <info@system76.com>
//
// SPDX-License-Identifier: GPL-3.0-only

use std::{fs::DirEntry, io, path::Path};

pub fn entries<T, F: FnMut(DirEntry) -> T>(path: &Path, mut func: F) -> io::Result<Vec<T>> {
    let mut ret = Vec::new();
    for entry_res in path.read_dir()? {
        ret.push(func(entry_res?));
    }

    Ok(ret)
}
