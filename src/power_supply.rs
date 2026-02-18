// Copyright 2018-2021 System76 <info@system76.com>
//
// SPDX-License-Identifier: GPL-3.0-only

//! AC power adapter detection and monitoring
//!
//! This module monitors AC adapter connection status via Netlink uevent notifications and provides
//! callbacks for power source changes. Used for automatic profile switching.

use std::{
    fs,
    io::{self, Read},
    os::fd::{AsRawFd, FromRawFd},
    path::Path,
    time::Duration,
};
use tokio::io::unix::AsyncFd;

/// Possible AC adapter sysfs paths across different systems
const AC_PATHS: &[&str] = &[
    "/sys/class/power_supply/AC0/online",
    "/sys/class/power_supply/AC/online",
    "/sys/class/power_supply/ADP0/online",
    "/sys/class/power_supply/ACAD/online",
];

/// Netlink constants for uevent monitoring
const NETLINK_KOBJECT_UEVENT: i32 = 15;
const UEVENT_BUFFER_SIZE: usize = 4096;

/// Current power source
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerSource {
    /// AC adapter is connected
    AC,
    /// Running on battery power
    Battery,
}

/// Detect the current power source by reading AC adapter status
///
/// Returns `Some(PowerSource::AC)` if AC is connected,
/// `Some(PowerSource::Battery)` if on battery,
/// or `None` if status cannot be determined.
pub fn get_power_source() -> Option<PowerSource> {
    for path in AC_PATHS {
        if let Ok(content) = fs::read_to_string(path) {
            log::debug!("Reading AC status from: {}", path);
            return match content.trim() {
                "1" => {
                    log::debug!("AC adapter detected: online");
                    Some(PowerSource::AC)
                }
                "0" => {
                    log::debug!("AC adapter detected: offline (on battery)");
                    Some(PowerSource::Battery)
                }
                _ => {
                    log::warn!("Unexpected AC status value: {}", content.trim());
                    None
                }
            };
        }
    }

    log::warn!("No AC adapter sysfs path found, checking battery status as fallback");

    // Fallback: Check battery status
    // If battery is discharging, we're on battery power
    // If battery is charging/full/not charging, we're on AC power
    if let Ok(status) = fs::read_to_string("/sys/class/power_supply/BAT0/status") {
        log::debug!("Battery status: {}", status.trim());
        return match status.trim() {
            "Discharging" => Some(PowerSource::Battery),
            "Charging" | "Full" | "Not charging" => Some(PowerSource::AC),
            _ => None,
        };
    }

    log::error!("Could not detect power source - no AC adapter or battery found");
    None
}

/// Find the AC adapter sysfs path on this system
///
/// Returns the first existing AC adapter path, or None if not found.
pub fn find_ac_path() -> Option<&'static str> {
    for &path in AC_PATHS {
        if Path::new(path).exists() {
            log::info!("Found AC adapter at: {}", path);
            return Some(path);
        }
    }
    log::warn!("No AC adapter sysfs path found in standard locations");
    None
}

/// Monitor AC adapter status and send changes to async channel
///
/// This function uses Netlink uevent monitoring to detect power source changes instantly.
/// Netlink provides hardware interrupt-driven notifications from the kernel when the
/// power_supply subsystem detects changes, providing:
/// - Instant detection (hardware interrupt driven)
/// - Zero CPU usage when idle (event-driven, not polling)
/// - Reliable kernel notifications (proper uevent mechanism)
/// - Deduplication of redundant kernel events
///
/// This is an async function that should be spawned on the tokio runtime.
///
/// # Arguments
///
/// * `tx` - Tokio channel sender for power source changes
///
/// # Errors
///
/// Returns an error if no AC adapter is found or if Netlink socket setup fails
pub async fn monitor_channel(
    tx: tokio::sync::mpsc::UnboundedSender<PowerSource>,
) -> anyhow::Result<()> {
    let ac_path = find_ac_path()
        .ok_or_else(|| anyhow::anyhow!("No AC adapter found - auto-switching will be disabled"))?;

    log::info!("Starting Netlink uevent-based AC adapter monitoring for: {}", ac_path);

    // Setup Netlink socket for uevent notifications
    let socket_fd = unsafe {
        let fd = libc::socket(
            libc::AF_NETLINK,
            libc::SOCK_DGRAM | libc::SOCK_CLOEXEC | libc::SOCK_NONBLOCK,
            NETLINK_KOBJECT_UEVENT,
        );
        if fd < 0 {
            return Err(anyhow::anyhow!(
                "Failed to create Netlink socket: {}",
                io::Error::last_os_error()
            ));
        }
        fd
    };

    // Bind to Netlink socket to receive uevent notifications
    let mut sockaddr: libc::sockaddr_nl = unsafe { std::mem::zeroed() };
    sockaddr.nl_family = libc::AF_NETLINK as u16;
    sockaddr.nl_groups = 1; // Listen to kernel uevents

    unsafe {
        if libc::bind(
            socket_fd,
            &sockaddr as *const _ as *const libc::sockaddr,
            std::mem::size_of::<libc::sockaddr_nl>() as u32,
        ) < 0
        {
            libc::close(socket_fd);
            return Err(anyhow::anyhow!(
                "Failed to bind Netlink socket: {}",
                io::Error::last_os_error()
            ));
        }
    }

    // Wrap socket in AsyncFd for tokio integration
    let file = unsafe { std::fs::File::from_raw_fd(socket_fd) };
    let async_fd = AsyncFd::new(file)?;

    // Track last known state for deduplication
    let mut last_known_state: Option<PowerSource> = None;

    // Read and send initial state
    if let Ok(initial_state) = read_ac_status(ac_path) {
        last_known_state = Some(initial_state);
        log::info!("Initial power source: {:?}", initial_state);

        // Send initial state through channel
        if tx.send(initial_state).is_err() {
            log::warn!("Power monitoring channel closed before monitoring started");
            return Ok(());
        }
        log::info!("Initial power state sent to daemon: {:?}", initial_state);
    }

    log::info!("Listening for hardware interrupts via Netlink uevents...");

    loop {
        // Wait for Netlink uevent
        let mut guard = async_fd.readable().await?;
        let mut buffer = [0u8; UEVENT_BUFFER_SIZE];

        match guard.try_io(|inner_file| {
            let n = unsafe {
                libc::recv(
                    inner_file.as_raw_fd(),
                    buffer.as_mut_ptr() as *mut libc::c_void,
                    buffer.len(),
                    0,
                )
            };
            if n < 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(n as usize)
        }) {
            Ok(Ok(n)) => {
                // Check if this is a power_supply subsystem event
                if is_power_supply_event(&buffer[..n]) {
                    log::debug!("Power supply uevent received, checking AC status");

                    // Read current state from sysfs (source of truth)
                    match read_ac_status(ac_path) {
                        Ok(current_state) => {
                            // Deduplication: only send if state actually changed
                            // (kernel fires 3-5 redundant events per change)
                            if Some(current_state) != last_known_state {
                                log::info!(
                                    "Power source changed: {:?} -> {:?}",
                                    last_known_state.unwrap_or(PowerSource::Battery),
                                    current_state
                                );
                                last_known_state = Some(current_state);

                                // Send to channel
                                if tx.send(current_state).is_err() {
                                    log::warn!(
                                        "Power monitoring channel closed, stopping monitoring"
                                    );
                                    break;
                                }
                                log::info!("Power source change sent to daemon successfully");
                            } else {
                                log::debug!("Uevent received but state unchanged (spurious event)");
                            }
                        }
                        Err(e) => {
                            log::warn!("Failed to read AC status from sysfs: {}", e);
                        }
                    }
                } else {
                    log::debug!("Non-power_supply uevent received, ignoring");
                }
            }
            Ok(Err(e)) => {
                log::error!("Error reading from Netlink socket: {}", e);
                return Err(e.into());
            }
            Err(_would_block) => {
                // Would block, continue waiting
                continue;
            }
        }

        guard.clear_ready();
    }

    Ok(())
}

/// Check if a Netlink uevent belongs to the power_supply subsystem
///
/// Netlink payloads are null-terminated strings, we scan for "SUBSYSTEM=power_supply"
fn is_power_supply_event(buffer: &[u8]) -> bool {
    buffer
        .split(|&b| b == 0)
        .any(|part| String::from_utf8_lossy(part).contains("SUBSYSTEM=power_supply"))
}

/// Read AC adapter status from sysfs file
///
/// Returns PowerSource::AC if online (1), PowerSource::Battery if offline (0)
fn read_ac_status(path: &str) -> io::Result<PowerSource> {
    let mut file = std::fs::File::open(path)?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;

    match contents.trim() {
        "1" => Ok(PowerSource::AC),
        "0" => Ok(PowerSource::Battery),
        other => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Unexpected AC status value: {}", other),
        )),
    }
}

/// Monitor AC adapter status and call callback on changes (legacy)
///
/// This is the legacy callback-based interface. New code should use `monitor_channel`.
///
/// This function uses periodic polling (every 2 seconds) to check for power source changes.
/// While inotify would be more efficient, sysfs files in the power_supply subsystem often
/// don't generate reliable inotify events across different kernel versions and hardware.
///
/// Polling every 2 seconds provides:
/// - Reliable detection on all systems
/// - Negligible CPU usage (one file read every 2 seconds)
/// - Fast enough response time for user experience (2 seconds is imperceptible)
///
/// This function blocks indefinitely. Run in a separate thread.
///
/// # Arguments
///
/// * `on_change` - Callback function called when power source changes
///
/// # Errors
///
/// Returns an error if no AC adapter is found
pub fn monitor<F>(mut on_change: F) -> anyhow::Result<()>
where
    F: FnMut(PowerSource),
{
    let _ac_path = find_ac_path()
        .ok_or_else(|| anyhow::anyhow!("No AC adapter found - auto-switching will be disabled"))?;

    // Get initial state
    let mut last_source = get_power_source();
    if let Some(initial_source) = last_source {
        log::info!("Initial power source: {:?}", initial_source);
    }

    log::info!("AC adapter monitoring active - polling every 2 seconds");

    loop {
        std::thread::sleep(Duration::from_secs(2));

        if let Some(source) = get_power_source() {
            if last_source != Some(source) {
                log::info!("Power source changed to: {:?}", source);
                last_source = Some(source);
                on_change(source);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_ac_path() {
        // This test will only pass on systems with AC adapter
        // Just verify it doesn't crash
        let _ = find_ac_path();
    }

    #[test]
    fn test_get_power_source() {
        // This test will only pass on systems with AC adapter or battery
        // Just verify it doesn't crash
        let _ = get_power_source();
    }
}
