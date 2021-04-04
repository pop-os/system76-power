use crate::{charge_thresholds::ChargeProfile, err_str, Power, DBUS_IFACE, DBUS_NAME, DBUS_PATH};
use dbus::{
    arg::Append,
    blocking::{BlockingSender, Connection},
    Message,
};
use pstate::PState;
use std::{io, time::Duration};
use sysfs_class::{Backlight, Brightness, Leds, SysClass};
use std::time::Duration;

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

    fn get_charge_profiles(&mut self) -> Result<Vec<ChargeProfile>, String> {
        let r = self.call_method::<bool>("GetChargeProfiles", None)?;
        r.get1().ok_or_else(|| "return value not found".to_string())
    }
}
