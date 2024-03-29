// Copyright 2018-2021 System76 <info@system76.com>
//
// SPDX-License-Identifier: GPL-3.0-only

use fern::{Dispatch, InitError};
use log::LevelFilter;
use std::io;

pub fn setup(filter: LevelFilter) -> Result<(), InitError> {
    Dispatch::new()
        // Exclude logs for crates that we use
        .level(LevelFilter::Off)
        // Include only the logs for this binary
        .level_for("system76_power", filter)
        .format(|out, message, record| out.finish(format_args!("[{}] {}", record.level(), message)))
        .chain(io::stderr())
        .apply()?;
    Ok(())
}
