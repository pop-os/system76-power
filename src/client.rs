use dbus::{BusType, Connection, Message};
use std::io;
use {DBUS_NAME, DBUS_PATH, DBUS_IFACE, Power, err_str};
use clap::ArgMatches;
use pstate::PState;
use sysfs_class::{Backlight, Leds, SysClass};

static TIMEOUT: i32 = 60 * 1000;

struct PowerClient {
    bus: Connection,
}

impl PowerClient {
    fn new() -> Result<PowerClient, String> {
        let bus = Connection::get_private(BusType::System).map_err(err_str)?;
        Ok(PowerClient {
            bus: bus
        })
    }
}

impl Power for PowerClient {
    fn performance(&mut self) -> Result<(), String> {
        info!("Setting power profile to performance");
        let m = Message::new_method_call(DBUS_NAME, DBUS_PATH, DBUS_IFACE, "Performance")?;
        self.bus.send_with_reply_and_block(m, TIMEOUT).map_err(err_str)?;
        Ok(())
    }

    fn balanced(&mut self) -> Result<(), String> {
        info!("Setting power profile to balanced");
        let m = Message::new_method_call(DBUS_NAME, DBUS_PATH, DBUS_IFACE, "Balanced")?;
        self.bus.send_with_reply_and_block(m, TIMEOUT).map_err(err_str)?;
        Ok(())
    }

    fn battery(&mut self) -> Result<(), String> {
        info!("Setting power profile to battery");
        let m = Message::new_method_call(DBUS_NAME, DBUS_PATH, DBUS_IFACE, "Battery")?;
        self.bus.send_with_reply_and_block(m, TIMEOUT).map_err(err_str)?;
        Ok(())
    }

    fn get_graphics(&mut self) -> Result<String, String> {
        let m = Message::new_method_call(DBUS_NAME, DBUS_PATH, DBUS_IFACE, "GetGraphics")?;
        let r = self.bus.send_with_reply_and_block(m, TIMEOUT).map_err(err_str)?;
        r.get1().ok_or_else(|| "return value not found".to_string())
    }

    fn get_profile(&mut self) -> Result<String, String> {
        let m = Message::new_method_call(DBUS_NAME, DBUS_PATH, DBUS_IFACE, "GetProfile")?;
        let r = self.bus.send_with_reply_and_block(m, TIMEOUT).map_err(err_str)?;
        r.get1().ok_or_else(|| "return value not found".to_string())
    }

    fn get_switchable(&mut self) -> Result<bool, String> {
        let m = Message::new_method_call(DBUS_NAME, DBUS_PATH, DBUS_IFACE, "GetSwitchable")?;
        let r = self.bus.send_with_reply_and_block(m, TIMEOUT).map_err(err_str)?;
        r.get1().ok_or_else(|| "return value not found".to_string())
    }

    fn set_graphics(&mut self, vendor: &str) -> Result<(), String> {
        info!("Setting graphics to {}", vendor);
        let m = Message::new_method_call(DBUS_NAME, DBUS_PATH, DBUS_IFACE, "SetGraphics")?
            .append1(vendor);
        self.bus.send_with_reply_and_block(m, TIMEOUT).map_err(err_str)?;
        Ok(())
    }

    fn get_graphics_power(&mut self) -> Result<bool, String> {
        let m = Message::new_method_call(DBUS_NAME, DBUS_PATH, DBUS_IFACE, "GetGraphicsPower")?;
        let r = self.bus.send_with_reply_and_block(m, TIMEOUT).map_err(err_str)?;
        r.get1().ok_or_else(|| "return value not found".to_string())
    }

    fn set_graphics_power(&mut self, power: bool) -> Result<(), String> {
        info!("Turning discrete graphics {}", if power { "on" } else { "off "});
        let m = Message::new_method_call(DBUS_NAME, DBUS_PATH, DBUS_IFACE, "SetGraphicsPower")?
            .append1(power);
        self.bus.send_with_reply_and_block(m, TIMEOUT).map_err(err_str)?;
        Ok(())
    }

    fn auto_graphics_power(&mut self) -> Result<(), String> {
        info!("Setting discrete graphics to turn off when not in use");
        let m = Message::new_method_call(DBUS_NAME, DBUS_PATH, DBUS_IFACE, "AutoGraphicsPower")?;
        self.bus.send_with_reply_and_block(m, TIMEOUT).map_err(err_str)?;
        Ok(())
    }
}

fn profile(client: &mut PowerClient) -> io::Result<()> {
    let profile = client.get_profile().ok();
    let profile = profile.as_ref().map_or("?", |s| s.as_str());
    println!("Power Profile: {}", profile);

    if let Ok(pstate) = PState::new() {
        let min = pstate.min_perf_pct()?;
        let max = pstate.max_perf_pct()?;
        let no_turbo = pstate.no_turbo()?;
        println!("CPU: {}% - {}%, {}", min, max, if no_turbo { "No Turbo" } else { "Turbo" });
    }

    for backlight in Backlight::iter() {
        let backlight = backlight?;
        let brightness = backlight.actual_brightness()?;
        let max_brightness = backlight.max_brightness()?;
        let ratio = (brightness as f64)/(max_brightness as f64);
        let percent = (ratio * 100.0) as u64;
        println!("Backlight {}: {}/{} = {}%", backlight.id(), brightness, max_brightness, percent);
    }

    for backlight in Leds::keyboard_backlights() {
        let backlight = backlight?;
        let brightness = backlight.brightness()?;
        let max_brightness = backlight.max_brightness()?;
        let ratio = (brightness as f64)/(max_brightness as f64);
        let percent = (ratio * 100.0) as u64;
        println!("Keyboard Backlight {}: {}/{} = {}%", backlight.id(), brightness, max_brightness, percent);
    }

    Ok(())
}

pub fn client(subcommand: &str, matches: &ArgMatches) -> Result<(), String> {
    let mut client = PowerClient::new()?;

    match subcommand {
        "profile" => match matches.value_of("profile") {
            Some("balanced") => client.balanced(),
            Some("battery") => client.battery(),
            Some("performance") => client.performance(),
            _ => profile(&mut client).map_err(err_str)
        },
        "graphics" => match matches.subcommand() {
            ("intel", _) => client.set_graphics("intel"),
            ("nvidia", _) => client.set_graphics("nvidia"),
            ("switchable", _) => {
                if client.get_switchable()? {
                    println!("switchable");
                } else {
                    println!("not switchable");
                }
                Ok(())
            }
            ("power", Some(matches)) => match matches.value_of("state") {
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
            }
            _ => {
                println!("{}", client.get_graphics()?);
                Ok(())
            }
        }
        _ => Err(format!("unknown sub-command {}", subcommand))
    }
}
