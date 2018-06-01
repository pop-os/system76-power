use std::thread;
use std::time::Duration;
use upower_dbus::UPower;
use pstate::PState;
use super::Power;
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
                    &mut pstate,
                    || if let Err(why) = client.performance() {
                        eprintln!("ac_events failed to set daemon to performance: {}", why);
                    },
                    || upower.on_battery().unwrap_or(false)
                );
            } else if upower.get_percentage().unwrap_or(0f64) < 25f64 {
                set_until(
                    &mut pstate,
                    || if let Err(why) = client.battery() {
                        eprintln!("ac_events failed to set daemon to battery: {}", why);
                    },
                    || upower.get_percentage().unwrap_or(0f64) > 50f64
                );
            }
            thread::sleep(Duration::from_secs(1));
        }
    });
}


fn set_until<A: FnMut(), U: FnMut() -> bool>(
    pstate: &mut PState,
    mut action: A,
    mut until: U
) {
    if let Ok((min, max, no_turbo)) = pstate.get_all_values() {
        action();
        let new_values = pstate.get_all_values().unwrap();

        loop {
            thread::sleep(Duration::from_secs(1));
            if until() {
                if pstate.get_all_values().unwrap() == new_values {
                    let _ = pstate.set_min_perf_pct(min);
                    let _ = pstate.set_max_perf_pct(max);
                    let _ = pstate.set_no_turbo(no_turbo);
                }
                break;
            }
        }
    }
}
