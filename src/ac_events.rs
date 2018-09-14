use std::thread;
use std::time::Duration;
use upower_dbus::UPower;
use super::Power;
use super::client::PowerClient;

pub fn ac_events() {
    thread::spawn(move || {
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

                    on_ac = false;
                }
            } else {
                if ! upower.on_battery().unwrap_or(false) {
                    // Switch to performance if we were on battery, and are now on AC.
                    if let Err(why) = client.performance() {
                        eprintln!("ac_events failed to set daemon to performance: {}", why);
                    }

                    on_ac = true;
                } else if ! critical && upower.get_percentage().unwrap_or(0f64) < 25f64 {
                    // Switch to battery if the battery has dropped less than 25%.
                    if let Err(why) = client.battery() {
                        eprintln!("ac_events failed to set daemon to battery: {}", why);
                    }

                    critical = true;
                } else if critical && upower.get_percentage().unwrap_or(0f64) > 50f64 {
                    // Switch to balanced once the battery is back to being beyond 50%.
                    if let Err(why) = client.balanced() {
                        eprintln!("ac_events failed to set daemon to balanced: {}", why);
                    }

                    critical = false;
                }
            }

            thread::sleep(Duration::from_secs(1));
        }
    });
}