use dbus::{BusType, Connection, Message};
use std::io;

use {DBUS_NAME, DBUS_PATH, DBUS_IFACE, Power, err_str};
use backlight::Backlight;
use kbd_backlight::KeyboardBacklight;
use pstate::PState;

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
        r.get1().ok_or("return value not found".to_string())
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
        r.get1().ok_or("return value not found".to_string())
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

fn profile() -> io::Result<()> {
    {
        let pstate = PState::new()?;
        let min = pstate.min_perf_pct()?;
        let max = pstate.max_perf_pct()?;
        let no_turbo = pstate.no_turbo()?;
        println!("CPU: {}% - {}%, {}", min, max, if no_turbo { "No Turbo" } else { "Turbo" });
    }

    for backlight in Backlight::all()? {
        let brightness = backlight.actual_brightness()?;
        let max_brightness = backlight.max_brightness()?;
        let ratio = (brightness as f64)/(max_brightness as f64);
        let percent = (ratio * 100.0) as u64;
        println!("Backlight {}: {}/{} = {}%", backlight.name(), brightness, max_brightness, percent);
    }

    for backlight in KeyboardBacklight::all()? {
        let brightness = backlight.brightness()?;
        let max_brightness = backlight.max_brightness()?;
        let ratio = (brightness as f64)/(max_brightness as f64);
        let percent = (ratio * 100.0) as u64;
        println!("Keyboard Backlight {}: {}/{} = {}%", backlight.name(), brightness, max_brightness, percent);
    }

    Ok(())
}

fn usage() {
    eprintln!("system76-power [options] [sub-command] [args...]");
    eprintln!("  --quiet - reduce logging verbosity");
    eprintln!("  --verbose - increase logging verbosity");
    eprintln!("  daemon - run in daemon mode");
    eprintln!("  daemon --experimental - run in daemon mode with experimental features");
    eprintln!("  profile - query current profile");
    eprintln!("  profile performance - set profile to performance");
    eprintln!("  profile balanced - set profile to balanced");
    eprintln!("  profile battery - set profile to battery");
    eprintln!("  graphics - query graphics mode");
    eprintln!("  graphics intel - set graphics mode to intel");
    eprintln!("  graphics nvidia - set graphics mode to nvidia");
    eprintln!("  graphics power - query discrete graphics power state");
    eprintln!("  graphics power auto - turn off discrete graphics if not in use");
    eprintln!("  graphics power off - power off discrete graphics");
    eprintln!("  graphics power on - power on discrete graphics");
}

pub fn client<I: Iterator<Item=String>>(mut args: I) -> Result<(), String> {
    let mut client = PowerClient::new()?;

    if let Some(arg) = args.next() {
        match arg.as_str() {
            "profile" => if let Some(arg) = args.next() {
                match arg.as_str() {
                    "performance" => client.performance(),
                    "balanced" => client.balanced(),
                    "battery" => client.battery(),
                    _ => {
                        usage();
                        Err(format!("unknown profile {}", arg))
                    }
                }
            } else {
                profile().map_err(err_str)
            },
            "graphics" => if let Some(arg) = args.next() {
                match arg.as_str() {
                    "intel" => client.set_graphics("intel"),
                    "nvidia" => client.set_graphics("nvidia"),
                    "power" => if let Some(arg) = args.next() {
                        match arg.as_str() {
                            "auto" => client.auto_graphics_power(),
                            "off" => client.set_graphics_power(false),
                            "on" => client.set_graphics_power(true),
                            _ => {
                                usage();
                                Err(format!("unknown graphics power {}", arg))
                            }
                        }
                    } else {
                        if client.get_graphics_power()? {
                            println!("on");
                        } else {
                            println!("off");
                        }
                        Ok(())
                    },
                    _ => {
                        usage();
                        Err(format!("unknown graphics vendor {}", arg))
                    }
                }
            } else {
                println!("{}", client.get_graphics()?);
                Ok(())
            },
            _ => {
                usage();
                Err(format!("unknown sub-command {}", arg))
            }
        }
    } else {
        usage();
        Err(format!("no sub-command specified"))
    }
}