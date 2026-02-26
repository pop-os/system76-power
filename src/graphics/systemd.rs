// Copyright 2018-2021 System76 <info@system76.com>
//
// SPDX-License-Identifier: GPL-3.0-only

use std::{io, process};

pub(crate) const SYSTEMCTL_CMD: &str = "systemctl";

/// Run `systemctl <args>` and return the exit status.
pub(crate) fn systemctl(args: &[&str]) -> io::Result<process::ExitStatus> {
    process::Command::new(SYSTEMCTL_CMD).args(args).status()
}

/// Returns true if the given systemd unit is currently active (running).
pub(crate) fn is_active(unit: &str) -> bool {
    systemctl(&["is-active", "--quiet", unit]).map(|s| s.success()).unwrap_or(false)
}

/// Returns true if the given systemd unit is enabled (will start on boot).
pub(crate) fn is_enabled(unit: &str) -> bool {
    systemctl(&["is-enabled", "--quiet", unit]).map(|s| s.success()).unwrap_or(false)
}

/// Start a systemd unit. Returns the exit status.
pub(crate) fn start(unit: &str) -> io::Result<process::ExitStatus> {
    systemctl(&["start", unit])
}

/// Stop a systemd unit. Returns the exit status.
pub(crate) fn stop(unit: &str) -> io::Result<process::ExitStatus> {
    systemctl(&["stop", unit])
}

/// Enable a systemd unit. Returns the exit status.
pub(crate) fn enable(unit: &str) -> io::Result<process::ExitStatus> {
    systemctl(&["enable", unit])
}

/// Disable a systemd unit. Returns the exit status.
pub(crate) fn disable(unit: &str) -> io::Result<process::ExitStatus> {
    systemctl(&["disable", unit])
}
