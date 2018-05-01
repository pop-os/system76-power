use dbus::{self, BusType, Connection};
use dbus::stdintf::org_freedesktop_dbus::Properties;

pub struct UPower(Connection);

impl UPower {
    pub fn new() -> Result<UPower, dbus::Error> {
        Connection::get_private(BusType::System)
            .map(|conn| UPower(conn))
    }

    pub fn on_battery(&self, timeout_ms: i32) -> bool {
        let p = self.0.with_path("org.freedesktop.UPower", "/org/freedesktop/UPower", timeout_ms);
        p.get("org.freedesktop.UPower", "OnBattery").unwrap_or(false)
    }
}
