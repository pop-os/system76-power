use dbus;
use dbus::stdintf::org_freedesktop_dbus::Properties;

pub struct SettingsDaemonPower {
    conn: dbus::Connection
}

impl SettingsDaemonPower {
    pub fn new() -> Result<Self, dbus::Error> {
        Ok(Self { conn: dbus::Connection::get_private(dbus::BusType::Session)? })
    }

    fn get_path<'a>(&'a self) -> dbus::ConnPath<'a, &'a dbus::Connection> {
        self.conn.with_path("org.gnome.SettingsDaemon.Power", "/org/gnome/SettingsDaemon/Power", 1000)
    }

    pub fn get_brightness_keyboard(&self) -> Result<i32, dbus::Error> {
        self.get_path().get::<i32>(
            "org.gnome.SettingsDaemon.Power.Keyboard",
            "Brightness"
        )
    }

    pub fn get_brightness_screen(&self) -> Result<i32, dbus::Error> {
        self.get_path().get::<i32>(
            "org.gnome.SettingsDaemon.Power.Screen",
            "Brightness"
        )
    }

    pub fn set_brightness_keyboard(&self, value: i32) -> Result<(), dbus::Error> {
        self.get_path().set(
            "org.gnome.SettingsDaemon.Power.Keyboard",
            "Brightness",
            value
        )
    }

    pub fn set_brightness_screen(&self, value: i32) -> Result<(), dbus::Error> {
        self.get_path().set(
            "org.gnome.SettingsDaemon.Power.Screen",
            "Brightness",
            value
        )
    }
}