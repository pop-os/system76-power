use crate::{err_str, Power, DBUS_IFACE, DBUS_NAME, DBUS_PATH};
use clap::ArgMatches;
use dbus::{arg::Append, blocking::{BlockingSender, Connection}, Message};
use pstate::PState;
use std::io;
use std::time::Duration;
use sysfs_class::{Backlight, Brightness, Leds, SysClass};

static TIMEOUT: u64 = 60 * 1000;

pub struct PowerClient {
    bus: Connection,
}

impl PowerClient {
    pub fn new() -> Result<PowerClient, String> {
        let bus = Connection::new_system().map_err(err_str)?;
        Ok(PowerClient { bus })
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

        let r = self
            .bus
            .send_with_reply_and_block(m, Duration::from_millis(TIMEOUT))
            .map_err(|why| format!("daemon returned an error message: \"{}\"", err_str(why.message().unwrap_or(""))))?;

        Ok(r)
    }

    fn set_profile(&mut self, profile: &str) -> Result<(), String> {
        println!("setting power profile to {}", profile);
        self.call_method::<bool>(profile, None)?;
        Ok(())
    }
}

impl Power for PowerClient {
    fn performance(&mut self) -> Result<(), String> {
        self.set_profile("Performance")
    }

    fn balanced(&mut self) -> Result<(), String> {
        self.set_profile("Balanced")
    }

    fn battery(&mut self) -> Result<(), String> {
        self.set_profile("Battery")
    }

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

    fn set_graphics(&mut self, vendor: &str) -> Result<(), String> {
        println!("setting graphics to {}", vendor);
        self.call_method::<&str>("SetGraphics", Some(vendor)).map(|_| ())
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

pub fn client(subcommand: &str, matches: &ArgMatches) -> Result<(), String> {
    let mut client = PowerClient::new()?;

    match subcommand {
        "profile" => match matches.value_of("profile") {
            Some("balanced") => client.balanced(),
            Some("battery") => client.battery(),
            Some("performance") => client.performance(),
            _ => profile(&mut client).map_err(err_str),
        },
        "graphics" => match matches.subcommand() {
            ("compute", _) => client.set_graphics("compute"),
            ("hybrid", _) => client.set_graphics("hybrid"),
            ("integrated", _) | ("intel", _) => client.set_graphics("integrated"),
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
            },
            _ => {
                println!("{}", client.get_graphics()?);
                Ok(())
            }
        },
        "charge-thresholds" => match matches.values_of("thresholds") {
            Some(mut thresholds) => {
                assert_eq!(thresholds.len(), 2);
                let start = thresholds.next().unwrap();
                let end = thresholds.next().unwrap();
                let start = u8::from_str_radix(start, 10).map_err(err_str)?;
                let end = u8::from_str_radix(end, 10).map_err(err_str)?;
                client.set_charge_thresholds((start, end))?;
                Ok(())
            }
            None => {
                let (start, end) = client.get_charge_thresholds()?;
                println!("Start: {}", start);
                println!("End: {}", end);
                Ok(())
            }
        },
        _ => Err(format!("unknown sub-command {}", subcommand)),
    }
}
