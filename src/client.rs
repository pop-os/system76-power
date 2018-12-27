use dbus::{BusType, Connection, Message};
use dbus::arg::Append;
use std::cmp::Ordering;
use std::io;
use {DBUS_NAME, DBUS_PATH, DBUS_IFACE, Power, err_str};
use clap::ArgMatches;
use pstate::PState;
use sdp::SettingsDaemonPower;
use sysfs_class::{Backlight, Leds, SysClass};

static TIMEOUT: i32 = 60 * 1000;

struct PowerClient {
    bus: Connection,
    sdp: Option<SettingsDaemonPower>,
}

impl PowerClient {
    fn new() -> Result<PowerClient, String> {
        let bus = Connection::get_private(BusType::System).map_err(err_str)?;
        let sdp = SettingsDaemonPower::new().ok();
        Ok(PowerClient { bus, sdp })
    }

    fn call_method<A: Append>(&mut self, method: &str, append: Option<A>) -> Result<Message, String> {
        let mut m = Message::new_method_call(DBUS_NAME, DBUS_PATH, DBUS_IFACE, method)?;
        if let Some(arg) = append {
            m = m.append1(arg);
        }
        let r = self.bus.send_with_reply_and_block(m, TIMEOUT).map_err(err_str)?;
        Ok(r)
    }

    fn get_brightness_keyboard(&mut self) -> Result<i32, String> {
        if let Some(ref mut sdp) = self.sdp {
            return sdp.get_brightness_keyboard().map_err(err_str);
        }

        Ok(0)
    }

    fn get_brightness_screen(&mut self) -> Result<i32, String> {
        if let Some(ref mut sdp) = self.sdp {
            return sdp.get_brightness_screen().map_err(err_str);
        }

        Ok(0)
    }

    fn set_brightness_keyboard(&mut self, new: i32) -> Result<(), String> {
        if let Some(ref mut sdp) = self.sdp {
            sdp.set_brightness_keyboard(new).map_err(err_str)?;
        }

        Ok(())
    }

    fn set_brightness_keyboard_cmp(&mut self, new: i32, ordering: Ordering) -> Result<i32, String> {
        let brightness = self.get_brightness_keyboard()?;
        if new.cmp(&brightness) == ordering {
            self.set_brightness_keyboard(new)?;
            Ok(new)
        } else {
            Ok(brightness)
        }
    }

    fn set_brightness_screen(&mut self, new: i32) -> Result<(), String> {
        if let Some(ref mut sdp) = self.sdp {
            sdp.set_brightness_screen(new).map_err(err_str)?;
        }

        Ok(())
    }

    fn set_brightness_screen_cmp(&mut self, new: i32, ordering: Ordering) -> Result<i32, String> {
        let brightness = self.get_brightness_screen()?;
        if new.cmp(&brightness) == ordering {
            self.set_brightness_screen(new)?;
            Ok(new)
        } else {
            Ok(brightness)
        }
    }
}

impl Power for PowerClient {
    fn custom(&mut self, profile: &str) -> Result<(), String> {
        info!("Setting power profile to performance");
        self.call_method::<&str>("Custom", Some(profile))?;
        Ok(())
    }

    fn performance(&mut self) -> Result<(), String> {
        info!("Setting power profile to performance");
        self.call_method::<bool>("Performance", None)?;
        Ok(())
    }

    fn balanced(&mut self) -> Result<(), String> {
        info!("Setting power profile to balanced");
        self.call_method::<bool>("Balanced", None)?;
        Ok(())
    }

    fn battery(&mut self) -> Result<(), String> {
        info!("Setting power profile to battery");
        self.call_method::<bool>("Battery", None)?;
        Ok(())
    }

    fn get_graphics(&mut self) -> Result<String, String> {
        let m = Message::new_method_call(DBUS_NAME, DBUS_PATH, DBUS_IFACE, "GetGraphics")?;
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
        self.call_method::<&str>("SetGraphics", Some(vendor))?;
        Ok(())
    }

    fn get_graphics_power(&mut self) -> Result<bool, String> {
        let m = Message::new_method_call(DBUS_NAME, DBUS_PATH, DBUS_IFACE, "GetGraphicsPower")?;
        let r = self.bus.send_with_reply_and_block(m, TIMEOUT).map_err(err_str)?;
        r.get1().ok_or_else(|| "return value not found".to_string())
    }

    fn set_graphics_power(&mut self, power: bool) -> Result<(), String> {
        info!("Turning discrete graphics {}", if power { "on" } else { "off "});
        self.call_method::<bool>("SetGraphicsPower", Some(power))?;
        Ok(())
    }

    fn auto_graphics_power(&mut self) -> Result<(), String> {
        info!("Setting discrete graphics to turn off when not in use");
        self.call_method::<bool>("AutoGraphicsPower", None)?;
        Ok(())
    }
}

fn profile() -> io::Result<()> {
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
            Some(other) => client.custom(other),
            None => profile().map_err(err_str)
        },
        // TODO: Implement the brightness feature for clients.
        // "brightness" => match (matches.value_of("brightness"), matches.value_of("value")) {
        //     (Some("keyboard"), Some(value)) => {
        //         let new = value.parse::<i32>().unwrap();
        //         let new = if matches.is_present("min") {
        //             client.set_brightness_keyboard_cmp(new, Ordering::Less)?
        //         } else if matches.is_present("max") {
        //             client.set_brightness_keyboard_cmp(new, Ordering::Greater)?
        //         } else {
        //             client.set_brightness_keyboard(new)?;
        //             new
        //         };

        //         println!("keyboard brightness: {}", new);
        //         Ok(())
        //     },
        //     (Some("keyboard"), None) => {
        //         println!("keyboard brightness: {}", client.get_brightness_keyboard()?);
        //         Ok(())
        //     },
        //     (Some("screen"), Some(value)) => {
        //         let new = value.parse::<i32>().unwrap();
        //         let new = if matches.is_present("min") {
        //             client.set_brightness_screen_cmp(new, Ordering::Less)?
        //         } else if matches.is_present("max") {
        //             client.set_brightness_screen_cmp(new, Ordering::Greater)?
        //         } else {
        //             client.set_brightness_screen(new)?;
        //             new
        //         };

        //         println!("screen brightness: {}", new);
        //         Ok(())
        //     },
        //     (Some("screen"), None) => {
        //         println!("screen brightness: {}", client.get_brightness_screen()?);
        //         Ok(())
        //     },
        //     _ => unimplemented!()
        // }
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
