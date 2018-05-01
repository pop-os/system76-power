use dbus::{self, BusType, ConnPath, Connection, Message};
use dbus::stdintf::org_freedesktop_dbus::Properties;

macro_rules! device_property {
    ($device:expr, $prop:expr) => {{
        $device.connection_path($device.get_display_device())
            .get("org.freedesktop.UPower.Device", $prop)
    }}
}

pub struct UPower {
    connection: Connection,
    timeout: i32
}

impl UPower {
    pub fn new(timeout: i32) -> Result<UPower, dbus::Error> {
        Connection::get_private(BusType::System)
            .map(|connection| UPower { connection, timeout })
    }

    fn connection_path<'a, P: Into<dbus::Path<'a>>>(&'a self, path: P) -> ConnPath<'a, &'a Connection> {
        self.connection.with_path("org.freedesktop.UPower", path, self.timeout)
    }

    fn get_display_device(&self) -> dbus::Path {
        let reply = self.connection.send_with_reply_and_block(
            Message::new_method_call(
                "org.freedesktop.UPower",
                "/org/freedesktop/UPower",
                "org.freedesktop.UPower",
                "GetDisplayDevice"
            ).expect("failed message"),
            self.timeout
        ).expect("failed reply");

        reply.get1().expect("no value returned")
    }

    pub fn on_battery(&self) -> bool {
        self.connection_path("/org/freedesktop/UPower")
            .get("org.freedesktop.UPower", "OnBattery")
            .unwrap_or(false)
    }

    pub fn get_percentage(&self) -> f64 {
        device_property!(self, "Percentage").unwrap()
    }

    pub fn get_energy(&self) -> f64 {
        device_property!(self, "Energy").unwrap()
    }

    pub fn get_energy_total(&self) -> f64 {
        device_property!(self, "EnergyFull").unwrap()
    }

    pub fn get_online(&self) -> bool {
        device_property!(self, "Online").unwrap()
    }
}
