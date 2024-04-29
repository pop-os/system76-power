// Copyright 2018-2021 System76 <info@system76.com>
//
// SPDX-License-Identifier: GPL-3.0-only

use crate::args::{Args, GraphicsArgs};
use anyhow::Context;
use intel_pstate::PState;
use std::io;
use sysfs_class::{Backlight, Brightness, Leds, SysClass};
use system76_power_zbus::PowerDaemonProxy;

async fn profile(client: &mut PowerDaemonProxy<'_>) -> io::Result<()> {
    let profile = client.get_profile().await.ok();
    let profile = profile.as_ref().map_or("?", |s| s.as_str());
    println!("Power Profile: {}", profile);

    if let Ok(values) = PState::new().and_then(|pstate| pstate.values()) {
        println!(
            "CPU: {}% - {}%, {}",
            values.min_perf_pct,
            values.max_perf_pct,
            if values.no_turbo { "No Turbo" } else { "Turbo" }
        );
    }

    for backlight in Backlight::iter() {
        let backlight = backlight?;
        let brightness = backlight.actual_brightness()?;
        let max_brightness = backlight.max_brightness()?;
        let ratio = (brightness as f64) / (max_brightness as f64);
        let percent = (ratio * 100.0) as u64;
        println!("Backlight {}: {}/{} = {}%", backlight.id(), brightness, max_brightness, percent);
    }

    for backlight in Leds::iter_keyboards() {
        let backlight = backlight?;
        let brightness = backlight.brightness()?;
        let max_brightness = backlight.max_brightness()?;
        let ratio = (brightness as f64) / (max_brightness as f64);
        let percent = (ratio * 100.0) as u64;
        println!(
            "Keyboard Backlight {}: {}/{} = {}%",
            backlight.id(),
            brightness,
            max_brightness,
            percent
        );
    }

    Ok(())
}

#[tokio::main(flavor = "current_thread")]
pub async fn client(args: &Args) -> anyhow::Result<()> {
    let connection =
        zbus::Connection::system().await.context("failed to create zbus system connection")?;

    let mut client = PowerDaemonProxy::new(&connection)
        .await
        .context("failed to connect to system76-power daemon")?;

    match args {
        Args::Profile { profile: name } => match name.as_deref() {
            Some("balanced") => client.balanced().await.map_err(zbus_error),
            Some("battery") => {
                if client.get_desktop().await.map_err(zbus_error)? {
                    return Err(anyhow::anyhow!(
                        r#"
Battery power profile is not supported on desktop computers.
"#,
                    ));
                }
                client.battery().await.map_err(zbus_error)
            }
            Some("performance") => client.performance().await.map_err(zbus_error),
            _ => profile(&mut client).await.context("failed to get power profile"),
        },
        Args::Graphics { cmd } => {
            if !client.get_switchable().await? {
                return Err(anyhow::anyhow!(
                    r#"
Graphics switching is not supported on this device, because
this device is either a desktop or doesn't have both an iGPU and dGPU.
"#,
                ));
            }

            match cmd.as_ref() {
                Some(GraphicsArgs::Compute) => {
                    client.set_graphics("compute").await.map_err(zbus_error)
                }
                Some(GraphicsArgs::Hybrid) => {
                    client.set_graphics("hybrid").await.map_err(zbus_error)
                }
                Some(GraphicsArgs::Integrated) => {
                    client.set_graphics("integrated").await.map_err(zbus_error)
                }
                Some(GraphicsArgs::Nvidia) => {
                    client.set_graphics("nvidia").await.map_err(zbus_error)
                }
                Some(GraphicsArgs::Switchable) => client
                    .get_switchable()
                    .await
                    .map_err(zbus_error)
                    .map(|b| println!("{}", if b { "switchable" } else { "not switchable" })),
                Some(GraphicsArgs::Power { state }) => match state.as_deref() {
                    Some("auto") => client.auto_graphics_power().await.map_err(zbus_error),
                    Some("off") => client.set_graphics_power(false).await.map_err(zbus_error),
                    Some("on") => client.set_graphics_power(true).await.map_err(zbus_error),
                    _ => {
                        if client.get_graphics_power().await.map_err(zbus_error)? {
                            println!("on (discrete)");
                        } else {
                            println!("off (discrete)");
                        }
                        Ok(())
                    }
                },
                None => {
                    println!("{}", client.get_graphics().await.map_err(zbus_error)?);
                    Ok(())
                }
            }
        }
        Args::ChargeThresholds { profile, list_profiles, thresholds } => {
            if client.get_desktop().await.map_err(zbus_error)? {
                return Err(anyhow::anyhow!(
                    r#"
Charge thresholds are not supported on desktop computers.
"#,
                ));
            }

            let profiles = client.get_charge_profiles().await.map_err(zbus_error)?;

            if !thresholds.is_empty() {
                let start = thresholds[0];
                let end = thresholds[1];
                client.set_charge_thresholds(&(start, end)).await.map_err(zbus_error)?;
            } else if let Some(name) = profile {
                if let Some(profile) = profiles.iter().find(|p| &p.id == name) {
                    client
                        .set_charge_thresholds(&(profile.start, profile.end))
                        .await
                        .map_err(zbus_error)?;
                } else {
                    return Err(anyhow::anyhow!("No such profile '{}'", name));
                }
            } else if *list_profiles {
                for profile in &profiles {
                    println!("{}", profile.id);
                    println!("  Title: {}", profile.title);
                    println!("  Description: {}", profile.description);
                    println!("  Start: {}", profile.start);
                    println!("  End: {}", profile.end);
                }
                return Ok(());
            }

            let (start, end) = client.get_charge_thresholds().await.map_err(zbus_error)?;
            if let Some(profile) = profiles.iter().find(|p| p.start == start && p.end == end) {
                println!("Profile: {} ({})", profile.title, profile.id);
            } else {
                println!("Profile: Custom");
            }
            println!("Start: {}", start);
            println!("End: {}", end);

            Ok(())
        }
        Args::Daemon { .. } => unreachable!(),
    }
}

fn zbus_error(why: zbus::Error) -> anyhow::Error { anyhow::anyhow!("{}", why) }
