// Copyright 2018-2021 System76 <info@system76.com>
//
// SPDX-License-Identifier: GPL-3.0-only

#![deny(clippy::all)]

use clap::Parser;
use log::LevelFilter;
use std::process;
use system76_power::{args::Args, client, daemon, logging};

fn main() {
    let args = Args::parse();

    let res = match args {
        Args::Daemon { quiet, verbose } => {
            if let Err(why) = logging::setup(if verbose {
                LevelFilter::Debug
            } else if quiet {
                LevelFilter::Off
            } else {
                LevelFilter::Info
            }) {
                eprintln!("failed to set up logging: {}", why);
                process::exit(1);
            }

            if unsafe { libc::geteuid() } == 0 {
                daemon::daemon()
            } else {
                Err(anyhow::anyhow!("must be run as root"))
            }
        }
        _ => client::client(&args),
    };

    match res {
        Ok(()) => (),
        Err(err) => {
            eprintln!("{:?}", err);
            process::exit(1);
        }
    }
}
