use std::thread;
use std::time::Duration;
use upower::UPower;
use pstate::PState;
use super::{battery, performance};

pub fn ac_events(mut pstate: PState) {
    thread::spawn(move || {
        let upower = match UPower::new(1000) {
            Ok(upower) => upower,
            Err(why) => {
                eprintln!("ac_events loop failed: {}", why);
                return;
            }
        };

        loop {
            if !upower.on_battery() {
                set_until(
                    &mut pstate,
                    || {
                        // TODO: Use dbus instead?
                        let _ = performance();
                    },
                    || {
                        upower.on_battery()
                    }
                );
            } else if upower.get_percentage() < 25f64 {
                set_until(
                    &mut pstate,
                    || {
                        // TODO: Use dbus instead?
                        let _ = battery();
                    },
                    || upower.get_percentage() > 50f64
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
