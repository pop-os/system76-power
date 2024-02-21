// Copyright 2018-2021 System76 <info@system76.com>
//
// SPDX-License-Identifier: GPL-3.0-only

use crate::{
    args::{Args, GraphicsArgs},
    charge_thresholds::ChargeProfile,
    err_str, Power, DBUS_IFACE, DBUS_NAME, DBUS_PATH,
};
use dbus::{
    arg::Append,
    blocking::{BlockingSender, Connection},
    Message,
};
use intel_pstate::PState;
use std::{io, time::Duration};
use sysfs_class::{Backlight, Brightness, Leds, SysClass};

static TIMEOUT: u64 = 60 * 1000;

pub struct PowerClient {
    bus: Connection,
}

impl PowerClient {
    pub fn new() -> Result<Self, String> {
        let bus = Connection::new_system().map_err(err_str)?;
        Ok(Self { bus })
    }

    fn call_method<A: Append>(
        &mut self,
        method: &str,
        append: Option<A>,
    ) -> Result<Message, String> {
        let mut m = Message::new_method_call(DBUS_NAME, DBUS_PATH, DBUS_IFACE, method)?;
        if let Some(arg) = append {
            m = m.append1(arg);
        }

        let r = self.bus.send_with_reply_and_block(m, Duration::from_millis(TIMEOUT)).map_err(
            |why| {
                format!(
                    "daemon returned an error message: \"{}\"",
                    err_str(why.message().unwrap_or(""))
                )
            },
        )?;

        Ok(r)
    }

    fn set_profile(&mut self, profile: &str) -> Result<(), String> {
        println!("setting power profile to {}", profile);
        self.call_method::<bool>(profile, None)?;
        Ok(())
    }
}

impl Power for PowerClient {
    fn performance(&mut self) -> Result<(), String> { self.set_profile("Performance") }

    fn balanced(&mut self) -> Result<(), String> { self.set_profile("Balanced") }

    fn battery(&mut self) -> Result<(), String> { self.set_profile("Battery") }

    fn get_external_displays_require_dgpu(&mut self) -> Result<bool, String> {
        let r = self.call_method::<bool>("GetExternalDisplaysRequireDGPU", None)?;
        r.get1().ok_or_else(|| "return value not found".to_string())
    }

    fn get_default_graphics(&mut self) -> Result<String, String> {
        let r = self.call_method::<bool>("GetDefaultGraphics", None)?;
        r.get1().ok_or_else(|| "return value not found".to_string())
    }

    fn get_graphics(&mut self) -> Result<String, String> {
        let r = self.call_method::<bool>("GetGraphics", None)?;
        r.get1().ok_or_else(|| "return value not found".to_string())
    }

    fn get_profile(&mut self) -> Result<String, String> {
        let r = self.call_method::<bool>("GetProfile", None)?;
        r.get1().ok_or_else(|| "return value not found".to_string())
    }

    fn get_switchable(&mut self) -> Result<bool, String> {
        let r = self.call_method::<bool>("GetSwitchable", None)?;
        r.get1().ok_or_else(|| "return value not found".to_string())
    }

    fn get_desktop(&mut self) -> Result<bool, String> {
        let r = self.call_method::<bool>("GetDesktop", None)?;
        r.get1().ok_or_else(|| "return value not found".to_string())
    }

    fn set_graphics(&mut self, vendor: &str) -> Result<(), String> {
        println!("setting graphics to {}", vendor);
        let r = self.call_method::<&str>("SetGraphics", Some(vendor)).map(|_| ());
        if r.is_ok() {
            println!("reboot for changes to take effect");
        }
        r
    }

    fn get_graphics_power(&mut self) -> Result<bool, String> {
        let r = self.call_method::<bool>("GetGraphicsPower", None)?;
        r.get1().ok_or_else(|| "return value not found".to_string())
    }

    fn set_graphics_power(&mut self, power: bool) -> Result<(), String> {
        println!("turning discrete graphics {}", if power { "on" } else { "off " });
        self.call_method::<bool>("SetGraphicsPower", Some(power)).map(|_| ())
    }

    fn auto_graphics_power(&mut self) -> Result<(), String> {
        println!("setting discrete graphics to turn off when not in use");
        self.call_method::<bool>("AutoGraphicsPower", None).map(|_| ())
    }

    fn get_charge_thresholds(&mut self) -> Result<(u8, u8), String> {
        let r = self.call_method::<bool>("GetChargeThresholds", None)?;
        r.get1().ok_or_else(|| "return value not found".to_string())
    }

    fn set_charge_thresholds(&mut self, thresholds: (u8, u8)) -> Result<(), String> {
        self.call_method::<(u8, u8)>("SetChargeThresholds", Some(thresholds)).map(|_| ())
    }

    fn get_charge_profiles(&mut self) -> Result<Vec<ChargeProfile>, String> {
        let r = self.call_method::<bool>("GetChargeProfiles", None)?;
        r.get1().ok_or_else(|| "return value not found".to_string())
    }
}

fn profile(client: &mut PowerClient) -> io::Result<()> {
    let profile = client.get_profile().ok();
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

pub fn client(args: &Args) -> Result<(), String> {
    let mut client = PowerClient::new()?;

    match args {
        Args::Profile { profile: name } => {
            if client.get_desktop()? {
                return Err(String::from(
                    r#"
Power profiles are not supported on desktop computers.
"#,
                ));
            }

            match name.as_deref() {
                Some("balanced") => client.balanced(),
                Some("battery") => client.battery(),
                Some("performance") => client.performance(),
                _ => profile(&mut client).map_err(err_str),
            }
        }
        Args::Graphics { cmd } => {
            if !client.get_switchable()? {
                return Err(String::from(
                    r#"
Graphics switching is not supported on this device, because
this device is either a desktop or doesn't have both an iGPU and dGPU.
"#,
                ));
            }

            match cmd.as_ref() {
                Some(GraphicsArgs::Compute) => client.set_graphics("compute"),
                Some(GraphicsArgs::Hybrid) => client.set_graphics("hybrid"),
                Some(GraphicsArgs::Integrated) => client.set_graphics("integrated"),
                Some(GraphicsArgs::Nvidia) => client.set_graphics("nvidia"),
                Some(GraphicsArgs::Switchable) => client
                    .get_switchable()
                    .map(|b| println!("{}", if b { "switchable" } else { "not switchable" })),
                Some(GraphicsArgs::Power { state }) => match state.as_deref() {
                    Some("auto") => client.auto_graphics_power(),
                    Some("off") => client.set_graphics_power(false),
                    Some("on") => client.set_graphics_power(true),
                    _ => {
                        if client.get_graphics_power()? {
                            println!("on (discrete)");
                        } else {
                            println!("off (discrete)");
                        }
                        Ok(())
                    }
                },
                None => {
                    println!("{}", client.get_graphics()?);
                    Ok(())
                }
            }
        }
        Args::ChargeThresholds { profile, list_profiles, thresholds } => {
            if client.get_desktop()? {
                return Err(String::from(
                    r#"
Charge thresholds are not supported on desktop computers.
"#,
                ));
            }

            let profiles = client.get_charge_profiles()?;

            if !thresholds.is_empty() {
                let start = thresholds[0];
                let end = thresholds[1];
                client.set_charge_thresholds((start, end))?;
            } else if let Some(name) = profile {
                if let Some(profile) = profiles.iter().find(|p| &p.id == name) {
                    client.set_charge_thresholds((profile.start, profile.end))?;
                } else {
                    return Err(format!("No such profile '{}'", name));
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

            let (start, end) = client.get_charge_thresholds()?;
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
