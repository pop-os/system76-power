use std::sync::atomic::Ordering;
use std::thread;
use std::time::Duration;
use upower_dbus::UPower;
use pstate::PState;
use super::Power;
use super::daemon::{battery, balanced, performance, Profile, PROFILE_ACTIVE};
use super::client::PowerClient;

pub fn ac_events(mut pstate: PState) {
    thread::spawn(move || {
        let upower = match UPower::new(1000) {
            Ok(upower) => upower,
            Err(why) => {
                eprintln!("ac_events failed to connect to upower: {}", why);
                return;
            }
        };

        let mut client = match PowerClient::new() {
            Ok(client) => client,
            Err(why) => {
                eprintln!("ac_events failed to get client: {}", why);
                return
            }
        };

        loop {
            if !upower.on_battery().unwrap_or(false) {
                set_until(
                    &mut client,
                    &mut pstate,
                    |client| if let Err(why) = client.performance() {
                        eprintln!("ac_events failed to set daemon to performance: {}", why);
                    },
                    || upower.on_battery().unwrap_or(false)
                );
            } else if upower.get_percentage().unwrap_or(0f64) < 25f64 {
                set_until(
                    &mut client,
                    &mut pstate,
                    |client| if let Err(why) = client.battery() {
                        eprintln!("ac_events failed to set daemon to battery: {}", why);
                    },
                    || upower.get_percentage().unwrap_or(0f64) > 50f64
                );
            }
            thread::sleep(Duration::from_secs(1));
        }
    });
}


fn set_until<A: FnMut(&mut PowerClient), U: FnMut() -> bool>(
    client: &mut PowerClient,
    pstate: &mut PState,
    mut action: A,
    mut until: U
) {
    if let Ok((min, max, no_turbo)) = pstate.get_all_values() {
        // This profile will be restored upon reaching the `until` condition.
        let previous_profile = PROFILE_ACTIVE.load(Ordering::SeqCst);

        action(client);

        // If this profile differs at the time of returning, we will keep the newer profile.
        let set_profile = PROFILE_ACTIVE.load(Ordering::SeqCst);

        loop {
            thread::sleep(Duration::from_secs(1));
            if until() {
                if PROFILE_ACTIVE.load(Ordering::SeqCst) == set_profile {
                    let _ = match previous_profile {
                        Profile::Battery => client.battery(),
                        Profile::Balanced => client.balanced(),
                        Profile::Performance => client.performance(),
                    };
                }
                break;
            }
        }
    }
}
