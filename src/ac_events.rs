use dbus::{BusType, Connection};
use dbus::tree::Signal;
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use upower_dbus::UPower;
use super::{DBUS_PATH, DBUS_NAME, Power};
use super::client::PowerClient;

pub fn ac_events(sig_critical: Arc<Signal<()>>, sig_normal: Arc<Signal<()>>, sig_ac: Arc<Signal<()>>) {
    thread::spawn(move || {
        let connection = match Connection::get_private(BusType::System) {
            Ok(c) => c,
            Err(why) => {
                eprintln!("ac_events failed to get DBUS connection: {}", why);
                return;
            }
        };

        let upower = match UPower::new(1000) {
            Ok(upower) => upower,
            Err(why) => {
                eprintln!("ac_events failed to connect to upower: {}", why);
                return;
            }
        };

        let client = &mut match PowerClient::new() {
            Ok(client) => client,
            Err(why) => {
                eprintln!("ac_events failed to get client: {}", why);
                return
            }
        };

        let mut on_ac = false;
        let mut critical = false;

        loop {
            if on_ac {
                if upower.on_battery().unwrap_or(false) {
                    // Switch to balanced if we were on AC, and are now on battery.
                    if let Err(why) = client.balanced() {
                        eprintln!("ac_events failed to set daemon to balanced: {}", why);
                    }

                    let _ = connection.send(sig_ac.msg(&DBUS_PATH.into(), &DBUS_NAME.into()).append1(false));
                    on_ac = false;
                }
            } else {
                if ! upower.on_battery().unwrap_or(false) {
                    // Switch to performance if we were on battery, and are now on AC.
                    if let Err(why) = client.performance() {
                        eprintln!("ac_events failed to set daemon to performance: {}", why);
                    }

                    let _ = connection.send(sig_ac.msg(&DBUS_PATH.into(), &DBUS_NAME.into()).append1(true));
                    on_ac = true;
                } else if ! critical && upower.get_percentage().unwrap_or(0f64) < 25f64 {
                    // Switch to battery if the battery has dropped less than 25%.
                    if let Err(why) = client.battery() {
                        eprintln!("ac_events failed to set daemon to battery: {}", why);
                    }

                    let _ = connection.send(sig_critical.msg(&DBUS_PATH.into(), &DBUS_NAME.into()));
                    critical = true;
                } else if critical && upower.get_percentage().unwrap_or(0f64) > 50f64 {
                    // Switch to balanced once the battery is back to being beyond 50%.
                    if let Err(why) = client.balanced() {
                        eprintln!("ac_events failed to set daemon to balanced: {}", why);
                    }

                     let _ = connection.send(sig_normal.msg(&DBUS_PATH.into(), &DBUS_NAME.into()));
                    critical = false;
                }
            }

            thread::sleep(Duration::from_secs(1));
        }
    });
}