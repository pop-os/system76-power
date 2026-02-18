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
pub fn get_session_context() -> Result<SessionContext, io::Error> {
    log::info!("=== Fetching Session Context (once) ===");

    let user = get_display_user()?;
    let env_vars = get_user_session_env(&user)?;

    log::info!("Session context ready: user={}, env_vars={}", user, env_vars.len());

    Ok(SessionContext { user, env_vars })
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

/// Query available display modes for a specific display using gnome-randr
fn query_gnome_modes(ctx: &SessionContext, display: &str) -> Result<Vec<DisplayMode>, io::Error> {
    log::info!("Querying available modes for display '{}' via gnome-randr", display);

    // Query gnome-randr as the user with their session environment
    let mut cmd = Command::new("sudo");
    cmd.arg("-u").arg(&ctx.user);
    cmd.arg("env");

    // Set environment variables from user session
    for (key, value) in &ctx.env_vars {
        cmd.arg(format!("{}={}", key, value));
    }

    cmd.arg("gnome-randr").arg("query");

    let output = cmd.output()?;

    if !output.status.success() {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            format!("gnome-randr query failed: {}", String::from_utf8_lossy(&output.stderr)),
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut modes = Vec::new();
    let mut in_display_section = false;

    log::debug!("Parsing gnome-randr output for display '{}'", display);

    // Parse the output to find modes for the specified display
    for line in stdout.lines() {
        let trimmed = line.trim_start();

        // Check if we're entering the display section
        // Display lines start without whitespace
        if !line.starts_with(' ') && !line.starts_with('\t') {
            if line.starts_with(display) {
                log::debug!("Found display section: {}", line);
                in_display_section = true;
                continue;
            } else if in_display_section {
                // We've moved to a different display section
                log::debug!("Exiting display section (found different display)");
                break;
            }
        }

        // Parse mode lines (they start with whitespace)
        if in_display_section && trimmed.len() < line.len() {
            // Skip lines that don't look like modes (e.g., "associated physical monitors:")
            if trimmed.contains('@') {
                if let Some(mode) = parse_gnome_mode_line(line) {
                    modes.push(mode);
                }
            }
        }
    }

    log::info!("Found {} available modes for display '{}'", modes.len(), display);

    if modes.is_empty() {
        log::warn!("No modes found! gnome-randr output:\n{}", stdout);
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("No display modes found for {}", display),
        ));
    }

    Ok(modes)
}

/// Query available display modes for a specific display using xrandr
fn query_xrandr_modes(ctx: &SessionContext, display: &str) -> Result<Vec<DisplayMode>, io::Error> {
    log::info!("Querying available modes for display '{}' via xrandr", display);

    // Query xrandr as the user with their session environment
    let mut cmd = Command::new("sudo");
    cmd.arg("-u").arg(&ctx.user);
    cmd.arg("env");

    // Set environment variables from user session
    for (key, value) in &ctx.env_vars {
        cmd.arg(format!("{}={}", key, value));
    }

    cmd.arg("xrandr").arg("--query");

    let output = cmd.output()?;

    if !output.status.success() {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            format!("xrandr query failed: {}", String::from_utf8_lossy(&output.stderr)),
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut modes = Vec::new();
    let mut in_display_section = false;
    let mut current_resolution = String::new();

    log::debug!("Parsing xrandr output for display '{}'", display);

    // Parse xrandr output
    for line in stdout.lines() {
        // Check if we're entering the display section
        if line.starts_with(display) && line.contains("connected") {
            log::debug!("Found display section: {}", line);
            in_display_section = true;
            continue;
        }

        // Check if we've moved to another display
        if in_display_section && !line.starts_with(' ') && !line.starts_with('\t') {
            log::debug!("Exiting display section");
            break;
        }

        // Parse mode lines (start with whitespace)
        if in_display_section {
            let trimmed = line.trim_start();

            // Check if this is a resolution line (e.g., "   2560x1440     165.00 +  60.00")
            if trimmed.len() < line.len() && trimmed.contains('x') {
                // Extract resolution (first token)
                if let Some(first_token) = trimmed.split_whitespace().next() {
                    // Check if it looks like a resolution (contains 'x' and can be parsed)
                    if first_token.contains('x') {
                        current_resolution = first_token.to_string();

                        // Parse all refresh rates on this line
                        if let Some(mode) = parse_xrandr_mode_line(line, &current_resolution) {
                            modes.push(mode);
                        }

                        // Also check for additional rates on the same line
                        let parts: Vec<&str> = trimmed.split_whitespace().collect();
                        for part in parts.iter().skip(1) {
                            let rate_str = part.trim_end_matches('+').trim_end_matches('*');
                            if let Ok(rate) = rate_str.parse::<f32>() {
                                if rate > 10.0 {
                                    // Sanity check
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

    log::info!("Found {} available modes for display '{}'", modes.len(), display);

    if modes.is_empty() {
        log::warn!("No modes found! xrandr output:\n{}", stdout);
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("No display modes found for {}", display),
        ));
    }

    Ok(modes)
}

/// Get the built-in display name for GNOME (usually eDP-1)
fn get_builtin_display_name_gnome(ctx: &SessionContext) -> Result<String, io::Error> {
    log::debug!("Querying gnome-randr for built-in display name");

    // Query gnome-randr as the user with their session environment
    let mut cmd = Command::new("sudo");
    cmd.arg("-u").arg(&ctx.user);
    cmd.arg("env");

    // Set environment variables from user session
    for (key, value) in &ctx.env_vars {
        cmd.arg(format!("{}={}", key, value));
    }

    cmd.arg("gnome-randr").arg("query");

    let output = cmd.output()?;

    if !output.status.success() {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            format!("gnome-randr query failed: {}", String::from_utf8_lossy(&output.stderr)),
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Parse output to find built-in display (eDP-*)
    for line in stdout.lines() {
        let line = line.trim();
        // Look for lines like "eDP-1 connected ..." or just "eDP-1"
        if line.starts_with("eDP") && !line.contains("disconnected") {
            if let Some(name) = line.split_whitespace().next() {
                log::debug!("Found built-in display: {}", name);
                return Ok(name.to_string());
            }
        }
    }

    // Fallback to common name
    log::warn!("Could not detect built-in display name, using default: eDP-1");
    Ok("eDP-1".to_string())
}

/// Set refresh rate on GNOME Wayland using gnome-randr
///
/// Queries available modes and selects the best match for the requested refresh rate.
fn set_refresh_rate_gnome_wayland(ctx: &SessionContext, rate: u32) -> Result<(), io::Error> {
    let output_name = get_builtin_display_name_gnome(ctx)?;
    log::info!("Target display: {}", output_name);

    log::info!("Setting display refresh rate to {}Hz on {} via gnome-randr", rate, output_name);

    // Query available modes for this display
    let available_modes = query_gnome_modes(ctx, &output_name)?;

    // Find best matching mode (prefer VRR if available)
    let best_mode = find_best_mode(&available_modes, rate, true).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!("No suitable mode found for {}Hz on {}", rate, output_name),
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

    // Execute gnome-randr command as the user with their session environment
    // We need to pass env vars via the 'env' command since sudo clears them by default
    let mut cmd = Command::new("sudo");
    cmd.arg("-u").arg(&ctx.user);
    cmd.arg("env");

    // Set environment variables from user session
    for (key, value) in &ctx.env_vars {
        cmd.arg(format!("{}={}", key, value));
    }

    cmd.arg("gnome-randr")
        .arg("modify")
        .arg(&output_name)
        .arg("--mode")
        .arg(&best_mode.mode_string);

    log::debug!(
        "Executing: sudo -u {} env [vars...] gnome-randr modify {} --mode {}",
        ctx.user,
        output_name,
        best_mode.mode_string
    );

    let output = cmd.output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        log::error!("gnome-randr modify failed: {}", stderr);
        return Err(io::Error::new(
            io::ErrorKind::Other,
            format!("gnome-randr modify failed: {}", stderr),
        ));
    }

    log::info!("Successfully set display refresh rate to {:.2}Hz", best_mode.refresh_rate);
    Ok(())
}

/// Set refresh rate on X11 using xrandr
///
/// Queries available modes and selects the best match for the requested refresh rate.
fn set_refresh_rate_xrandr(ctx: &SessionContext, rate: u32) -> Result<(), io::Error> {
    log::info!("Setting display refresh rate to {}Hz via xrandr (X11)", rate);

    // Find built-in display (usually eDP-1 or eDP)
    let mut query_cmd = Command::new("sudo");
    query_cmd.arg("-u").arg(&ctx.user);
    query_cmd.arg("env");

    // Set environment variables from user session
    for (key, value) in &ctx.env_vars {
        query_cmd.arg(format!("{}={}", key, value));
    }

    query_cmd.arg("xrandr").arg("--query");

    let output = query_cmd.output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut builtin_output = "eDP-1".to_string();

    // Find the built-in display name
    for line in stdout.lines() {
        if line.contains("connected") && (line.starts_with("eDP") || line.starts_with("LVDS")) {
            if let Some(name) = line.split_whitespace().next() {
                builtin_output = name.to_string();
                log::debug!("Found built-in display: {}", builtin_output);
                break;
            }
        }
    }

    log::info!("Target display: {}", builtin_output);

    // Query available modes for this display
    let available_modes = query_xrandr_modes(ctx, &builtin_output)?;

    // Find best matching mode (X11 doesn't have VRR in mode strings, so prefer_vrr doesn't matter)
    let best_mode = find_best_mode(&available_modes, rate, false).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!("No suitable mode found for {}Hz on {}", rate, builtin_output),
        )
    })?;

    log::info!(
        "Applying mode: {}x{} @ {:.2}Hz",
        best_mode.resolution.0,
        best_mode.resolution.1,
        best_mode.refresh_rate
    );

    // Set the mode using xrandr
    let mut set_cmd = Command::new("sudo");
    set_cmd.arg("-u").arg(&ctx.user);
    set_cmd.arg("env");

    // Set environment variables from user session
    for (key, value) in &ctx.env_vars {
        set_cmd.arg(format!("{}={}", key, value));
    }

    // Use --mode instead of --rate for more precise control
    // Format: resolution@rate (e.g., "2560x1440" with rate "165.00")
    set_cmd
        .arg("xrandr")
        .arg("--output")
        .arg(&builtin_output)
        .arg("--mode")
        .arg(format!("{}x{}", best_mode.resolution.0, best_mode.resolution.1))
        .arg("--rate")
        .arg(format!("{:.2}", best_mode.refresh_rate));

    log::debug!(
        "Executing: sudo -u {} env [vars...] xrandr --output {} --mode {}x{} --rate {:.2}",
        ctx.user,
        builtin_output,
        best_mode.resolution.0,
        best_mode.resolution.1,
        best_mode.refresh_rate
    );

    let output = set_cmd.output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        log::error!("xrandr command failed: {}", stderr);
        return Err(io::Error::new(
            io::ErrorKind::Other,
            format!("xrandr command failed: {}", stderr),
        ));
    }

    log::info!("Successfully set X11 display refresh rate to {:.2}Hz", best_mode.refresh_rate);
    Ok(())
}

/// Set display refresh rate based on detected display server
///
/// Automatically detects GNOME Wayland, X11, or other Wayland compositors
/// and uses the appropriate method to set the refresh rate.
///
/// This function now dynamically queries available display modes and selects
/// the best match, making it work on any display/resolution combination.
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
pub fn set_refresh_rate(rate: u32) -> Result<(), io::Error> {
    log::info!("=== Display Refresh Rate Change Request ===");
    log::info!("Target refresh rate: {}Hz", rate);

    // Fetch session context ONCE - this replaces multiple redundant calls
    let ctx = get_session_context()?;

    // Detect display server using the session context we just fetched
    let display_server = detect_server_from_env_vars(&ctx.env_vars).unwrap_or_else(|| {
        // Fallback to detecting from current environment if session env doesn't help
        detect_from_env().unwrap_or(DisplayServer::Unknown)
    });

    log::info!("Detected display server: {:?}", display_server);

    let result = match display_server {
        DisplayServer::GnomeWayland => {
            log::info!("Using GNOME Wayland backend (gnome-randr)");
            set_refresh_rate_gnome_wayland(&ctx, rate)
        }
        DisplayServer::X11 => {
            log::info!("Using X11 backend (xrandr)");
            set_refresh_rate_xrandr(&ctx, rate)
        }
        DisplayServer::Wayland => {
            log::error!(
                "Generic Wayland compositor detected - refresh rate control not implemented"
            );
            log::info!("Supported: GNOME Wayland. For Sway/Hyprland, use wlr-randr manually.");
            Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "Wayland refresh rate control only supported for GNOME. Try wlr-randr manually.",
            ))
        }
        DisplayServer::Unknown => {
            log::error!(
                "Could not detect display server - no DISPLAY or WAYLAND_DISPLAY environment variables found"
            );
            Err(io::Error::new(
                io::ErrorKind::NotFound,
                "Could not detect display server (no DISPLAY or WAYLAND_DISPLAY)",
            ))
        }
    };

    match &result {
        Ok(_) => log::info!("=== Display Refresh Rate Change: SUCCESS ==="),
        Err(e) => log::error!("=== Display Refresh Rate Change: FAILED - {} ===", e),
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
