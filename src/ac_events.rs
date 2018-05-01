use std::thread;
use std::time::Duration;
use upower::UPower;
use pstate::PState;

pub fn ac_events(mut pstate: PState) {
    thread::spawn(move || {
        let upower = match UPower::new() {
            Ok(upower) => upower,
            Err(why) => {
                eprintln!("ac_events loop failed: {}", why);
                return;
            }
        };
        
        // TODO: Use dbus instead
        loop {
            if !upower.on_battery(1000) {
                if let Ok((min, max, no_turbo)) = pstate.get_all_values() {
                    let _ = performance();
                    loop {
                        thread::sleep(Duration::from_secs(1));
                        if upower.on_battery(1000) {
                            let _ = pstate.set_min_perf_pct(min);
                            let _ = pstate.set_max_perf_pct(max);
                            let _ = pstate.set_no_turbo(no_turbo);
                            break;
                        }
                    }
                }
            }
            thread::sleep(Duration::from_secs(1));
        }
    });
}
