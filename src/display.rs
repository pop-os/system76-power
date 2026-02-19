// Copyright 2018-2021 System76 <info@system76.com>
//
// SPDX-License-Identifier: GPL-3.0-only

//! Display refresh rate management
//!
//! This module provides functionality to control display refresh rates based on
//! power profiles. It supports GNOME Wayland (via gnome-randr) and can be extended
//! to support other display servers.

use std::io;
use std::process::Command;

use crate::errors::DisplayError;

/// Compositor processes to search for, in priority order
const COMPOSITOR_PROCESSES: &[&str] = &[
    "gnome-shell",  // GNOME Wayland/X11
    "kwin_wayland", // KDE Plasma Wayland
    "kwin_x11",     // KDE Plasma X11
    "plasmashell",  // KDE Plasma (fallback)
    "sway",         // Sway
    "Hyprland",     // Hyprland
    "wayfire",      // Wayfire
    "labwc",        // labwc
    "river",        // River
    "dwl",          // dwl
    "Xorg",         // X11 server (fallback)
];

/// Environment variables to extract from user session
const SESSION_ENV_VARS: &[&str] = &[
    "DISPLAY",
    "WAYLAND_DISPLAY",
    "XDG_SESSION_TYPE",
    "XDG_CURRENT_DESKTOP",
    "XDG_RUNTIME_DIR",
    "DBUS_SESSION_BUS_ADDRESS",
    "WAYLAND_SOCKET",
    "QT_QPA_PLATFORM",
];

/// Represents a display mode with resolution and refresh rate
#[derive(Debug, Clone)]
pub struct DisplayMode {
    /// Full mode string (e.g., "2560x1440@165.001+vrr")
    pub mode_string: String,
    /// Resolution as (width, height)
    pub resolution: (u32, u32),
    /// Refresh rate in Hz
    pub refresh_rate: f32,
    /// Whether VRR (Variable Refresh Rate) is supported
    pub has_vrr: bool,
}

/// Display refresh rate configuration
#[derive(Debug, Clone)]
pub struct RefreshRateConfig {
    pub enabled: bool,
    pub battery: u32,
    pub balanced: u32,
    pub performance: u32,
}

impl Default for RefreshRateConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            battery: 60,      // Power saving on battery
            balanced: 60,     // Conservative on AC
            performance: 165, // Max refresh for Razer Blade 14 and similar
        }
    }
}

/// Display mode specification - can be explicit mode string or resolution + refresh rate
#[derive(Debug, Clone)]
pub enum ModeSpec {
    /// Explicit mode string (e.g., "2560x1440@165.001+vrr")
    ModeString(String),
    /// Resolution and refresh rate (width, height, hz)
    ResolutionAndRate(u32, u32, u32),
}

/// Display mode configuration for AC auto-switching
/// This allows changing both resolution and refresh rate when plugging/unplugging AC
#[derive(Debug, Clone)]
pub struct DisplayModeConfig {
    pub enabled: bool,
    pub ac_mode: Option<ModeSpec>,
    pub battery_mode: Option<ModeSpec>,
}

impl Default for DisplayModeConfig {
    fn default() -> Self {
        Self {
            enabled: false, // Disabled by default, opt-in feature
            ac_mode: None,
            battery_mode: None,
        }
    }
}

/// Display server types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayServer {
    GnomeWayland,
    Wayland,
    X11,
    Unknown,
}

/// Detect which display server is currently running
pub fn detect_display_server() -> DisplayServer {
    // First, try to get environment from the current process (works for direct invocation)
    if let Some(server) = detect_from_env() {
        return server;
    }

    // If running as a system service, query the user session environment
    if let Some(server) = detect_from_user_session() {
        return server;
    }

    DisplayServer::Unknown
}

/// Detect display server from current process environment variables
fn detect_from_env() -> Option<DisplayServer> {
    let session_type = std::env::var("XDG_SESSION_TYPE").ok();
    let current_desktop = std::env::var("XDG_CURRENT_DESKTOP").ok();
    let wayland_display = std::env::var("WAYLAND_DISPLAY").ok();
    let x11_display = std::env::var("DISPLAY").ok();

    // Check for GNOME Wayland specifically
    if session_type.as_deref() == Some("wayland")
        && current_desktop.as_ref().map(|d| d.contains("GNOME")).unwrap_or(false)
    {
        log::debug!("Detected GNOME Wayland from process environment");
        return Some(DisplayServer::GnomeWayland);
    }

    // Check for other Wayland compositors
    if wayland_display.is_some() {
        log::debug!("Detected Wayland from process environment");
        return Some(DisplayServer::Wayland);
    }

    // Check for X11
    if x11_display.is_some() {
        log::debug!("Detected X11 from process environment");
        return Some(DisplayServer::X11);
    }

    None
}

/// Detect display server from user session environment
/// This is used when running as a system service without access to session variables
fn detect_from_user_session() -> Option<DisplayServer> {
    log::debug!("Attempting to detect display server from user session");

    // Get the display user
    let user = match get_display_user() {
        Ok(u) => u,
        Err(e) => {
            log::debug!("Failed to get display user: {}", e);
            return None;
        }
    };

    // Get environment variables from user session
    let env_vars = match get_user_session_env(&user) {
        Ok(vars) => vars,
        Err(e) => {
            log::debug!("Failed to get user session environment: {}", e);
            return None;
        }
    };

    detect_server_from_env_vars(&env_vars)
}

/// Detect display server from environment variables
/// Helper function that can work with both current process env and session env
fn detect_server_from_env_vars(env_vars: &[(String, String)]) -> Option<DisplayServer> {
    let mut session_type = None;
    let mut current_desktop = None;
    let mut wayland_display = None;
    let mut x11_display = None;

    // Parse environment variables
    for (key, value) in env_vars {
        match key.as_str() {
            "XDG_SESSION_TYPE" => session_type = Some(value.clone()),
            "XDG_CURRENT_DESKTOP" => current_desktop = Some(value.clone()),
            "WAYLAND_DISPLAY" => wayland_display = Some(value.clone()),
            "DISPLAY" => x11_display = Some(value.clone()),
            _ => {}
        }
    }

    log::debug!(
        "Environment variables: session_type={:?}, desktop={:?}, wayland={:?}, x11={:?}",
        session_type,
        current_desktop,
        wayland_display,
        x11_display
    );

    // Check for GNOME Wayland specifically
    if session_type.as_deref() == Some("wayland")
        && current_desktop.as_ref().map(|d| d.contains("GNOME")).unwrap_or(false)
    {
        log::debug!("Detected GNOME Wayland");
        return Some(DisplayServer::GnomeWayland);
    }

    // Check for other Wayland compositors
    if wayland_display.is_some() {
        log::debug!("Detected Wayland");
        return Some(DisplayServer::Wayland);
    }

    // Check for X11
    if x11_display.is_some() {
        log::debug!("Detected X11");
        return Some(DisplayServer::X11);
    }

    None
}

/// Session information from loginctl
#[derive(Debug)]
struct SessionInfo {
    id: String,
    user: String,
    seat: String,
    session_type: Option<String>,
    state: Option<String>,
}

/// Session context containing user and environment variables
/// This is fetched once and passed to all helper functions to avoid redundant lookups
#[derive(Debug, Clone)]
pub struct SessionContext {
    pub user: String,
    pub env_vars: Vec<(String, String)>,
}

/// Query detailed information about a specific session
fn query_session_details(session_id: &str) -> Result<SessionInfo, io::Error> {
    let output = Command::new("loginctl")
        .arg("show-session")
        .arg(session_id)
        .arg("--property=Type")
        .arg("--property=State")
        .arg("--property=Seat")
        .arg("--property=Name")
        .output()?;

    if !output.status.success() {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            format!("Failed to query session {}", session_id),
        ));
    }

    let info = String::from_utf8_lossy(&output.stdout);

    let mut session_type = None;
    let mut state = None;
    let mut seat = String::from("seat0"); // Default
    let mut user = String::new();

    for line in info.lines() {
        if let Some((key, value)) = line.split_once('=') {
            match key {
                "Type" => session_type = Some(value.to_string()),
                "State" => state = Some(value.to_string()),
                "Seat" => seat = value.to_string(),
                "Name" => user = value.to_string(),
                _ => {}
            }
        }
    }

    log::debug!(
        "Session {} details: type={:?}, state={:?}, seat={}, user={}",
        session_id,
        session_type,
        state,
        seat,
        user
    );

    Ok(SessionInfo { id: session_id.to_string(), user, seat, session_type, state })
}

/// Check if a session is a graphical session (Wayland or X11)
fn is_graphical_session(session: &SessionInfo) -> bool {
    matches!(session.session_type.as_deref(), Some("wayland") | Some("x11"))
}

/// Check if a session is active
fn is_active_session(session: &SessionInfo) -> bool {
    session.state.as_deref() == Some("active")
}

/// Check if a session is on the primary graphics seat
fn is_primary_seat(session: &SessionInfo) -> bool {
    session.seat == "seat0"
}

/// Get the username running the active graphical session
fn get_display_user() -> Result<String, io::Error> {
    log::debug!("Detecting display user");

    // Method 1: Check SUDO_USER environment variable (when run via sudo)
    if let Ok(user) = std::env::var("SUDO_USER") {
        if !user.is_empty() && user != "root" {
            log::debug!("Found display user from SUDO_USER: {}", user);
            return Ok(user);
        }
    }

    // Method 2: Find active graphical session via loginctl
    let output = Command::new("loginctl").arg("list-sessions").arg("--no-legend").output()?;

    if !output.status.success() {
        return Err(io::Error::new(io::ErrorKind::Other, "loginctl list-sessions failed"));
    }

    let sessions = String::from_utf8_lossy(&output.stdout);

    log::debug!("Scanning sessions for active graphical session");

    // Collect and analyze sessions
    let mut candidates: Vec<(String, String)> = Vec::new(); // (session_id, user)

    for line in sessions.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 3 {
            continue;
        }

        let session_id = parts[0];
        let user = parts[2];

        // Skip root sessions
        if user == "root" {
            log::debug!("Skipping root session: {}", session_id);
            continue;
        }

        candidates.push((session_id.to_string(), user.to_string()));
    }

    log::debug!("Found {} non-root sessions to evaluate", candidates.len());

    // Priority 1: Active graphical session on seat0
    for (session_id, user) in &candidates {
        match query_session_details(session_id) {
            Ok(session) => {
                if is_graphical_session(&session)
                    && is_active_session(&session)
                    && is_primary_seat(&session)
                {
                    log::info!(
                        "Found ideal session: user={}, session={}, type={:?}, seat={}",
                        user,
                        session_id,
                        session.session_type,
                        session.seat
                    );
                    return Ok(user.clone());
                }
            }
            Err(e) => {
                log::debug!("Failed to query session {}: {}", session_id, e);
            }
        }
    }

    // Priority 2: Any graphical session (relaxed seat/state requirements)
    for (session_id, user) in &candidates {
        match query_session_details(session_id) {
            Ok(session) => {
                if is_graphical_session(&session) {
                    log::warn!(
                        "Using graphical session (not ideal): user={}, session={}, type={:?}, state={:?}, seat={}",
                        user,
                        session_id,
                        session.session_type,
                        session.state,
                        session.seat
                    );
                    return Ok(user.clone());
                }
            }
            Err(e) => {
                log::debug!("Failed to query session {}: {}", session_id, e);
            }
        }
    }

    // Priority 3: Fallback - first non-root user (original behavior)
    if let Some((_, user)) = candidates.first() {
        log::warn!("No graphical session found, using first non-root user as fallback: {}", user);
        return Ok(user.clone());
    }

    Err(io::Error::new(io::ErrorKind::NotFound, "No display user found (no non-root sessions)"))
}

/// Find the PID of a compositor process for a specific user
fn find_compositor_pid(user: &str) -> Option<String> {
    log::debug!("Searching for compositor process for user: {}", user);

    for process_name in COMPOSITOR_PROCESSES {
        log::debug!("Trying process: {}", process_name);

        let output = Command::new("pgrep").arg("-u").arg(user).arg("-x").arg(process_name).output();

        match output {
            Ok(output) if output.status.success() && !output.stdout.is_empty() => {
                let pid_str = String::from_utf8_lossy(&output.stdout);
                let first_pid = pid_str.lines().next().unwrap_or("").trim();

                if !first_pid.is_empty() {
                    log::info!("Found compositor: {} (PID: {})", process_name, first_pid);
                    return Some(first_pid.to_string());
                }
            }
            Ok(_) => {
                log::debug!("Process '{}' not found", process_name);
            }
            Err(e) => {
                log::debug!("Failed to search for '{}': {}", process_name, e);
            }
        }
    }

    log::debug!("No compositor process found");
    None
}

/// Find the session leader PID for a user via loginctl
fn find_session_leader_pid(user: &str) -> Result<String, io::Error> {
    log::debug!("Finding session leader for user: {}", user);

    let session_output =
        Command::new("loginctl").arg("list-sessions").arg("--no-legend").output()?;

    let sessions = String::from_utf8_lossy(&session_output.stdout);

    for line in sessions.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 3 && parts[2] == user {
            let session_id = parts[0];
            log::debug!("Found session {} for user {}", session_id, user);

            let leader_output = Command::new("loginctl")
                .arg("show-session")
                .arg(session_id)
                .arg("--property=Leader")
                .arg("--value")
                .output()?;

            if leader_output.status.success() {
                let pid = String::from_utf8_lossy(&leader_output.stdout).trim().to_string();
                log::info!("Session leader PID: {}", pid);
                return Ok(pid);
            }
        }
    }

    Err(io::Error::new(
        io::ErrorKind::NotFound,
        format!("Could not find session leader for user {}", user),
    ))
}

/// Read environment variables from /proc/<pid>/environ
fn read_process_environment(pid: &str) -> Result<Vec<(String, String)>, io::Error> {
    log::debug!("Reading environment from PID: {}", pid);

    let environ_path = format!("/proc/{}/environ", pid);
    let environ_data = std::fs::read(&environ_path)
        .map_err(|e| io::Error::new(e.kind(), format!("Failed to read {}: {}", environ_path, e)))?;

    let mut env_vars = Vec::new();

    // Parse null-separated environment variables
    for entry in environ_data.split(|&b| b == 0) {
        if let Ok(env_str) = std::str::from_utf8(entry) {
            if let Some((key, value)) = env_str.split_once('=') {
                // Check if this is a session environment variable we care about
                if SESSION_ENV_VARS.contains(&key) {
                    env_vars.push((key.to_string(), value.to_string()));
                    log::debug!("Found env: {}={}", key, value);
                }
            }
        }
    }

    if env_vars.is_empty() {
        log::warn!("No display environment variables found in PID {}", pid);
    } else {
        log::info!("Extracted {} environment variables from PID {}", env_vars.len(), pid);
    }

    Ok(env_vars)
}

/// Get environment variables from the user session
/// Returns a vector of (key, value) tuples for relevant display environment variables
fn get_user_session_env(user: &str) -> Result<Vec<(String, String)>, io::Error> {
    log::info!("Getting session environment for user: {}", user);

    // Strategy 1: Find compositor process (most reliable for display environment)
    if let Some(pid) = find_compositor_pid(user) {
        let env_vars = read_process_environment(&pid)?;

        if !env_vars.is_empty() {
            return Ok(env_vars);
        }

        log::warn!("Compositor PID {} had no display environment, trying fallback", pid);
    }

    // Strategy 2: Use session leader (fallback)
    log::debug!("Falling back to session leader PID");
    let pid = find_session_leader_pid(user)?;
    let env_vars = read_process_environment(&pid)?;

    if env_vars.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("No display environment variables found for user {}", user),
        ));
    }

    Ok(env_vars)
}

/// Get session context (user + environment) - call this once and pass it around
/// This avoids redundant session detection and environment extraction
pub fn get_session_context() -> Result<SessionContext, DisplayError> {
    log::info!("=== Fetching Session Context (once) ===");

    let user = get_display_user()?;
    let env_vars = get_user_session_env(&user)?;

    log::info!("Session context ready: user={}, env_vars={}", user, env_vars.len());

    Ok(SessionContext { user, env_vars })
}

/// Run an external program as the session user, injecting the session environment
///
/// Builds: `sudo -u <user> env KEY=VALUE... <program> [args...]`
/// This is the single canonical way to invoke display tools (gnome-randr, xrandr)
/// from a privileged daemon context.
fn run_command_as_user(
    ctx: &SessionContext,
    program: &str,
    args: &[&str],
) -> Result<std::process::Output, io::Error> {
    let mut cmd = Command::new("sudo");
    cmd.arg("-u").arg(&ctx.user).arg("env");
    for (key, value) in &ctx.env_vars {
        cmd.arg(format!("{}={}", key, value));
    }
    cmd.arg(program).args(args);
    cmd.output()
}

// ── Display backend abstraction ───────────────────────────────────────────────

/// Abstraction over GNOME Wayland and X11 display backends.
///
/// Each implementation queries the display and applies modes using the
/// appropriate tool (gnome-randr for GNOME Wayland, xrandr for X11).
trait DisplayManager {
    /// Query the built-in display name and all available modes in one call.
    fn get_display_info(&self) -> Result<(String, Vec<DisplayMode>), DisplayError>;

    /// Apply a specific mode to the named display.
    fn apply_mode(&self, display_name: &str, mode: &DisplayMode) -> Result<(), DisplayError>;
}

/// GNOME Wayland backend — uses `gnome-randr`
struct GnomeManager<'a> {
    ctx: &'a SessionContext,
}

impl DisplayManager for GnomeManager<'_> {
    fn get_display_info(&self) -> Result<(String, Vec<DisplayMode>), DisplayError> {
        log::debug!("Querying gnome-randr for display info (name + modes)");

        let output = run_command_as_user(self.ctx, "gnome-randr", &["query"])?;

        if !output.status.success() {
            return Err(DisplayError::CommandFailed {
                command: "gnome-randr query".to_string(),
                stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            });
        }

        let stdout = String::from_utf8_lossy(&output.stdout);

        // First pass: find the built-in display name (eDP-*)
        let mut display_name = None;
        for line in stdout.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("eDP") && !trimmed.contains("disconnected") {
                if let Some(name) = trimmed.split_whitespace().next() {
                    log::debug!("Found built-in display: {}", name);
                    display_name = Some(name.to_string());
                    break;
                }
            }
        }
        let display_name = display_name.unwrap_or_else(|| {
            log::warn!("Could not detect built-in display name, using default: eDP-1");
            "eDP-1".to_string()
        });

        // Second pass: collect modes for the detected display
        let mut modes = Vec::new();
        let mut in_display_section = false;

        log::debug!("Parsing gnome-randr output for display '{}'", display_name);

        for line in stdout.lines() {
            let trimmed = line.trim_start();

            if !line.starts_with(' ') && !line.starts_with('\t') {
                if line.starts_with(display_name.as_str()) {
                    log::debug!("Found display section: {}", line);
                    in_display_section = true;
                    continue;
                } else if in_display_section {
                    log::debug!("Exiting display section (found different display)");
                    break;
                }
            }

            if in_display_section && trimmed.len() < line.len() && trimmed.contains('@') {
                if let Some(mode) = parse_gnome_mode_line(line) {
                    modes.push(mode);
                }
            }
        }

        log::info!("Found {} available modes for display '{}'", modes.len(), display_name);

        if modes.is_empty() {
            log::warn!("No modes found! gnome-randr output:\n{}", stdout);
            return Err(DisplayError::ModeNotFound {
                display: display_name,
                spec: "any".to_string(),
            });
        }

        Ok((display_name, modes))
    }

    fn apply_mode(&self, display_name: &str, mode: &DisplayMode) -> Result<(), DisplayError> {
        log::debug!(
            "Executing: sudo -u {} env [vars...] gnome-randr modify {} --mode {}",
            self.ctx.user,
            display_name,
            mode.mode_string
        );
        let output = run_command_as_user(
            self.ctx,
            "gnome-randr",
            &["modify", display_name, "--mode", &mode.mode_string],
        )?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            log::error!("gnome-randr modify failed: {}", stderr);
            return Err(DisplayError::CommandFailed {
                command: "gnome-randr modify".to_string(),
                stderr: stderr.into_owned(),
            });
        }
        Ok(())
    }
}

/// X11 backend — uses `xrandr`
struct X11Manager<'a> {
    ctx: &'a SessionContext,
}

impl DisplayManager for X11Manager<'_> {
    fn get_display_info(&self) -> Result<(String, Vec<DisplayMode>), DisplayError> {
        log::debug!("Querying xrandr for display info (name + modes)");

        let output = run_command_as_user(self.ctx, "xrandr", &["--query"])?;

        if !output.status.success() {
            return Err(DisplayError::CommandFailed {
                command: "xrandr --query".to_string(),
                stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            });
        }

        let stdout = String::from_utf8_lossy(&output.stdout);

        // First pass: find the built-in display name (eDP-* or LVDS-*)
        let mut display_name = None;
        for line in stdout.lines() {
            if line.contains("connected") && (line.starts_with("eDP") || line.starts_with("LVDS")) {
                if let Some(name) = line.split_whitespace().next() {
                    log::debug!("Found built-in display: {}", name);
                    display_name = Some(name.to_string());
                    break;
                }
            }
        }
        let display_name = display_name.unwrap_or_else(|| {
            log::warn!("Could not detect built-in display name, using default: eDP-1");
            "eDP-1".to_string()
        });

        // Second pass: collect modes for the detected display
        let mut modes = Vec::new();
        let mut in_display_section = false;
        #[allow(unused_assignments)]
        let mut current_resolution = String::new();

        log::debug!("Parsing xrandr output for display '{}'", display_name);

        for line in stdout.lines() {
            if line.starts_with(display_name.as_str()) && line.contains("connected") {
                log::debug!("Found display section: {}", line);
                in_display_section = true;
                continue;
            }

            if in_display_section && !line.starts_with(' ') && !line.starts_with('\t') {
                log::debug!("Exiting display section");
                break;
            }

            if in_display_section {
                let trimmed = line.trim_start();

                if trimmed.len() < line.len() && trimmed.contains('x') {
                    if let Some(first_token) = trimmed.split_whitespace().next() {
                        if first_token.contains('x') {
                            current_resolution = first_token.to_string();

                            if let Some(mode) = parse_xrandr_mode_line(line, &current_resolution) {
                                modes.push(mode);
                            }

                            let parts: Vec<&str> = trimmed.split_whitespace().collect();
                            for part in parts.iter().skip(1) {
                                let rate_str = part.trim_end_matches('+').trim_end_matches('*');
                                if let Ok(rate) = rate_str.parse::<f32>() {
                                    if rate > 10.0 {
                                        let (width_str, height_str) =
                                            current_resolution.split_once('x').unwrap();
                                        if let (Ok(width), Ok(height)) =
                                            (width_str.parse::<u32>(), height_str.parse::<u32>())
                                        {
                                            let mode_string =
                                                format!("{}@{:.2}", current_resolution, rate);
                                            modes.push(DisplayMode {
                                                mode_string,
                                                resolution: (width, height),
                                                refresh_rate: rate,
                                                has_vrr: false,
                                            });
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        log::info!("Found {} available modes for display '{}'", modes.len(), display_name);

        if modes.is_empty() {
            log::warn!("No modes found! xrandr output:\n{}", stdout);
            return Err(DisplayError::ModeNotFound {
                display: display_name,
                spec: "any".to_string(),
            });
        }

        Ok((display_name, modes))
    }

    fn apply_mode(&self, display_name: &str, mode: &DisplayMode) -> Result<(), DisplayError> {
        log::debug!(
            "Executing: sudo -u {} env [vars...] xrandr --output {} --mode {}x{} --rate {:.2}",
            self.ctx.user,
            display_name,
            mode.resolution.0,
            mode.resolution.1,
            mode.refresh_rate
        );
        let output = run_command_as_user(
            self.ctx,
            "xrandr",
            &[
                "--output",
                display_name,
                "--mode",
                &format!("{}x{}", mode.resolution.0, mode.resolution.1),
                "--rate",
                &format!("{:.2}", mode.refresh_rate),
            ],
        )?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            log::error!("xrandr command failed: {}", stderr);
            return Err(DisplayError::CommandFailed {
                command: "xrandr".to_string(),
                stderr: stderr.into_owned(),
            });
        }
        Ok(())
    }
}

/// Construct the correct `DisplayManager` for the active session.
///
/// Returns an error for unsupported display servers (generic Wayland, Unknown).
fn create_manager(ctx: &SessionContext) -> Result<Box<dyn DisplayManager + '_>, DisplayError> {
    let display_server = detect_server_from_env_vars(&ctx.env_vars)
        .unwrap_or_else(|| detect_from_env().unwrap_or(DisplayServer::Unknown));

    log::info!("Detected display server: {:?}", display_server);

    match display_server {
        DisplayServer::GnomeWayland => {
            log::info!("Using GNOME Wayland backend (gnome-randr)");
            Ok(Box::new(GnomeManager { ctx }))
        }
        DisplayServer::X11 => {
            log::info!("Using X11 backend (xrandr)");
            Ok(Box::new(X11Manager { ctx }))
        }
        DisplayServer::Wayland => {
            log::error!("Generic Wayland compositor detected - display control not implemented");
            Err(DisplayError::UnsupportedBackend("Wayland".to_string()))
        }
        DisplayServer::Unknown => {
            log::error!("Could not detect display server");
            Err(DisplayError::SessionNotFound)
        }
    }
}

/// Parse a mode line from gnome-randr output
/// Example input: "    2560x1440@165.001+vrr	2560x1440 	165.00    	[scales...]"
fn parse_gnome_mode_line(line: &str) -> Option<DisplayMode> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }

    // Extract the mode string (first whitespace-separated token)
    let parts: Vec<&str> = trimmed.split_whitespace().collect();
    if parts.is_empty() {
        return None;
    }

    let mode_string = parts[0].to_string();

    // Parse mode string: resolution@rate+flags
    // Examples: "2560x1440@165.001+vrr", "1920x1080@60.002"
    let has_vrr = mode_string.contains("+vrr");
    let clean = mode_string.replace("+vrr", "");

    // Split into resolution and rate parts
    let (res_part, rate_part) = clean.split_once('@')?;
    let (width_str, height_str) = res_part.split_once('x')?;

    // Parse numeric values
    let width = width_str.parse::<u32>().ok()?;
    let height = height_str.parse::<u32>().ok()?;
    let refresh_rate = rate_part.parse::<f32>().ok()?;

    log::debug!(
        "Parsed mode: {} → {}x{} @ {:.2}Hz (VRR: {})",
        mode_string,
        width,
        height,
        refresh_rate,
        has_vrr
    );

    Some(DisplayMode { mode_string, resolution: (width, height), refresh_rate, has_vrr })
}

/// Parse xrandr mode line
/// Example input: "   2560x1440     59.95*+  165.00"
fn parse_xrandr_mode_line(line: &str, resolution: &str) -> Option<DisplayMode> {
    let trimmed = line.trim();

    // Check if line starts with resolution
    if !trimmed.starts_with(resolution) {
        return None;
    }

    // Extract refresh rates from the rest of the line
    let parts: Vec<&str> = trimmed.split_whitespace().collect();
    if parts.len() < 2 {
        return None;
    }

    // Find refresh rates (numbers with optional + or *)
    // The first rate is usually the active one (marked with *)
    for i in 1..parts.len() {
        let rate_str = parts[i].trim_end_matches('+').trim_end_matches('*');
        if let Ok(rate) = rate_str.parse::<f32>() {
            let mode_string = format!("{}@{:.2}", resolution, rate);

            log::debug!("Parsed X11 mode: {} @ {:.2}Hz", resolution, rate);

            let (width_str, height_str) = resolution.split_once('x')?;
            let width = width_str.parse::<u32>().ok()?;
            let height = height_str.parse::<u32>().ok()?;

            return Some(DisplayMode {
                mode_string,
                resolution: (width, height),
                refresh_rate: rate,
                has_vrr: false, // X11 doesn't report VRR in mode strings
            });
        }
    }

    None
}

/// Find the best matching mode for a target refresh rate
/// Prefers VRR modes if available, and selects closest matching rate
fn find_best_mode(
    modes: &[DisplayMode],
    target_rate: u32,
    prefer_vrr: bool,
) -> Option<&DisplayMode> {
    if modes.is_empty() {
        log::warn!("No modes available for matching");
        return None;
    }

    log::info!(
        "Finding best mode for target {}Hz from {} available modes (prefer VRR: {})",
        target_rate,
        modes.len(),
        prefer_vrr
    );

    let target_f32 = target_rate as f32;

    // First, try to find VRR modes if preferred
    let candidates = if prefer_vrr {
        let vrr_modes: Vec<_> = modes.iter().filter(|m| m.has_vrr).collect();
        if !vrr_modes.is_empty() {
            log::debug!("Found {} VRR modes, preferring those", vrr_modes.len());
            vrr_modes
        } else {
            log::debug!("No VRR modes found, using all {} modes", modes.len());
            modes.iter().collect()
        }
    } else {
        modes.iter().collect()
    };

    // Find the closest match by refresh rate
    let best = candidates
        .iter()
        .min_by_key(|mode| {
            let diff = (mode.refresh_rate - target_f32).abs();
            (diff * 1000.0) as u32 // Convert to millihertz for integer comparison
        })
        .copied();

    if let Some(mode) = best {
        log::info!(
            "Selected best mode: {} ({}x{} @ {:.2}Hz, VRR: {}) - diff: {:.2}Hz",
            mode.mode_string,
            mode.resolution.0,
            mode.resolution.1,
            mode.refresh_rate,
            mode.has_vrr,
            (mode.refresh_rate - target_f32).abs()
        );
    }

    best
}

/// Set display refresh rate based on detected display server
///
/// Automatically detects GNOME Wayland, X11, or other Wayland compositors
/// and uses the appropriate method to set the refresh rate.
///
/// # Arguments
///
/// * `rate` - Target refresh rate in Hz (e.g., 60, 120, 144, 165)
///
/// # Errors
///
/// Returns an error if:
/// - Display server cannot be detected
/// - Display user cannot be found
/// - No suitable mode is found for the requested refresh rate
/// - The underlying command (gnome-randr/xrandr) fails
pub fn set_refresh_rate(rate: u32) -> Result<(), DisplayError> {
    log::info!("=== Display Refresh Rate Change Request ===");
    log::info!("Target refresh rate: {}Hz", rate);

    let ctx = get_session_context()?;
    let manager = create_manager(&ctx)?;
    let (display_name, available_modes) = manager.get_display_info()?;

    log::info!("Target display: {}", display_name);

    let best_mode = find_best_mode(&available_modes, rate, true).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!("No suitable mode found for {}Hz on {}", rate, display_name),
        )
    })?;

    log::info!(
        "Applying mode: {} ({}x{} @ {:.2}Hz, VRR: {})",
        best_mode.mode_string,
        best_mode.resolution.0,
        best_mode.resolution.1,
        best_mode.refresh_rate,
        best_mode.has_vrr
    );

    let result = manager.apply_mode(&display_name, best_mode);

    match &result {
        Ok(_) => log::info!("=== Display Refresh Rate Change: SUCCESS ==="),
        Err(e) => log::error!("=== Display Refresh Rate Change: FAILED - {} ===", e),
    }

    result
}

/// Set display mode (resolution and refresh rate) based on ModeSpec
///
/// This function changes both resolution and refresh rate based on the provided mode specification.
/// It can work with either explicit mode strings or resolution + refresh rate combinations.
///
/// # Arguments
///
/// * `mode_spec` - The mode specification (either explicit mode string or resolution+rate)
///
/// # Errors
///
/// Select the best matching display mode from available modes based on specification
///
/// This helper function encapsulates the common mode selection logic used by both
/// GNOME Wayland and X11 backends.
///
/// # Arguments
/// * `available_modes` - Slice of available DisplayMode options from the display
/// * `mode_spec` - The desired mode specification (explicit string or resolution+rate)
/// * `display_name` - Name of the display (for error messages)
/// * `prefer_vrr` - Whether to prefer VRR modes when using ResolutionAndRate
///
/// # Returns
/// The best matching DisplayMode (cloned), or an error if no suitable mode found
fn select_best_mode(
    available_modes: &[DisplayMode],
    mode_spec: &ModeSpec,
    display_name: &str,
    prefer_vrr: bool,
) -> Result<DisplayMode, io::Error> {
    match mode_spec {
        ModeSpec::ModeString(mode_str) => {
            // Try exact mode string match first
            available_modes
                .iter()
                .find(|m| m.mode_string == *mode_str)
                .or_else(|| {
                    log::warn!("Exact mode string '{}' not found, trying fuzzy match", mode_str);
                    // Fuzzy match: compare without VRR suffix
                    let mode_str_no_vrr = mode_str.replace("+vrr", "");
                    available_modes
                        .iter()
                        .find(|m| m.mode_string.replace("+vrr", "") == mode_str_no_vrr)
                })
                .cloned()
                .ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::NotFound,
                        format!("Mode '{}' not found on display {}", mode_str, display_name),
                    )
                })
        }
        ModeSpec::ResolutionAndRate(width, height, hz) => {
            // Filter modes by resolution
            let filtered: Vec<_> = available_modes
                .iter()
                .filter(|m| m.resolution.0 == *width && m.resolution.1 == *height)
                .cloned()
                .collect();

            if filtered.is_empty() {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    format!(
                        "No modes found for resolution {}x{} on display {}",
                        width, height, display_name
                    ),
                ));
            }

            // Find best match by refresh rate (with optional VRR preference)
            find_best_mode(&filtered, *hz, prefer_vrr).cloned().ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    format!(
                        "No suitable mode found for {}x{}@{}Hz on display {}",
                        width, height, hz, display_name
                    ),
                )
            })
        }
    }
}

/// Returns an error if:
/// - Display server cannot be detected
/// - Display user cannot be found
/// - No suitable mode is found for the requested specification
/// - The underlying command (gnome-randr/xrandr) fails
pub fn set_display_mode(mode_spec: &ModeSpec) -> Result<(), DisplayError> {
    log::info!("=== Display Mode Change Request ===");

    match mode_spec {
        ModeSpec::ModeString(mode_str) => {
            log::info!("Target mode: {} (explicit mode string)", mode_str);
        }
        ModeSpec::ResolutionAndRate(width, height, hz) => {
            log::info!("Target mode: {}x{}@{}Hz", width, height, hz);
        }
    }

    let ctx = get_session_context()?;
    let manager = create_manager(&ctx)?;
    let (display_name, available_modes) = manager.get_display_info()?;

    log::info!("Target display: {}", display_name);

    let best_mode = select_best_mode(&available_modes, mode_spec, &display_name, true)?;

    log::info!(
        "Applying mode: {} ({}x{} @ {:.2}Hz, VRR: {})",
        best_mode.mode_string,
        best_mode.resolution.0,
        best_mode.resolution.1,
        best_mode.refresh_rate,
        best_mode.has_vrr
    );

    let result = manager.apply_mode(&display_name, &best_mode);

    match &result {
        Ok(_) => log::info!("=== Display Mode Change: SUCCESS ==="),
        Err(e) => log::error!("=== Display Mode Change: FAILED - {} ===", e),
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_display_server() {
        // Just verify it doesn't crash
        let _ = detect_display_server();
    }

    #[test]
    fn test_refresh_rate_config_default() {
        let config = RefreshRateConfig::default();
        assert_eq!(config.battery, 60);
        assert_eq!(config.performance, 165);
        assert!(config.enabled);
    }
}
