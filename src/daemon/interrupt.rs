// Copyright 2023 System76 <info@system76.com>
// SPDX-License-Identifier: GPL-3.0-only

use futures::FutureExt;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::signal::unix::{signal, SignalKind};

pub static CONTINUE: AtomicBool = AtomicBool::new(true);

pub fn handle() {
    let mut int = signal(SignalKind::interrupt()).unwrap();
    let mut hup = signal(SignalKind::hangup()).unwrap();
    let mut term = signal(SignalKind::terminate()).unwrap();

    tokio::spawn(async move {
        let sig = futures::select! {
            _ = int.recv().fuse() => "SIGINT",
            _ = hup.recv().fuse() => "SIGHUP",
            _ = term.recv().fuse() => "SIGTERM"
        };

        log::info!("caught signal: {}", sig);
        CONTINUE.store(false, Ordering::SeqCst);
    });
}
