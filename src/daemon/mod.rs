use dbus::{
    ffidisp::{Connection, NameFlag},
    tree::{Factory, MethodErr, Signal},
};
use std::{
    cell::RefCell,
    fs,
    rc::Rc,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
};

use crate::{
    err_str,
    errors::ProfileError,
    fan::FanDaemon,
    graphics::Graphics,
    hid_backlight,
    hotplug::HotPlugDetect,
    kernel_parameters::{KernelParameter, NmiWatchdog},
    mux::DisplayPortMux,
    Power, DBUS_IFACE, DBUS_NAME, DBUS_PATH,
};

mod profiles;

use self::profiles::*;

static CONTINUE: AtomicBool = AtomicBool::new(true);

fn signal_handling() {
    extern "C" fn handler(signal: libc::c_int) {
        info!("caught signal: {}", signal);
        CONTINUE.store(false, Ordering::SeqCst);
    }

    unsafe fn signal(signal: libc::c_int) { libc::signal(signal, handler as libc::sighandler_t); }

    unsafe {
        signal(libc::SIGINT);
        signal(libc::SIGHUP);
        signal(libc::SIGTERM);
        signal(libc::SIGKILL);
    }
}

// Disabled by default because some systems have quirky ACPI tables that fail to resume from
// suspension.
static PCI_RUNTIME_PM: AtomicBool = AtomicBool::new(false);

// TODO: Whitelist system76 hardware that's known to work with this setting.
fn pci_runtime_pm_support() -> bool { PCI_RUNTIME_PM.load(Ordering::SeqCst) }

struct PowerDaemon {
    initial_set:         bool,
    graphics:            Graphics,
    power_profile:       String,
    profile_errors:      Vec<ProfileError>,
    dbus_connection:     Arc<Connection>,
    power_switch_signal: Arc<Signal<()>>,
}

impl PowerDaemon {
    fn new(
        power_switch_signal: Arc<Signal<()>>,
        dbus_connection: Arc<Connection>,
    ) -> Result<PowerDaemon, String> {
        let graphics = Graphics::new().map_err(err_str)?;
        Ok(PowerDaemon {
            initial_set: false,
            graphics,
            power_profile: String::new(),
            profile_errors: Vec::new(),
            power_switch_signal,
            dbus_connection,
        })
    }

    fn apply_profile(
        &mut self,
        func: fn(&mut Vec<ProfileError>, bool),
        name: &str,
    ) -> Result<(), String> {
        if &self.power_profile == name {
            info!("profile was already set");
            return Ok(())
        }

        func(&mut self.profile_errors, self.initial_set);

        let message =
            self.power_switch_signal.msg(&DBUS_PATH.into(), &DBUS_NAME.into()).append1(name);

        if let Err(()) = self.dbus_connection.send(message) {
            error!("failed to send power profile switch message");
        }

        self.power_profile = name.into();

        if self.profile_errors.is_empty() {
            Ok(())
        } else {
            let mut error_message = String::from("Errors found when setting profile:");
            for error in self.profile_errors.drain(..) {
                error_message = format!("{}\n    - {}", error_message, error);
            }

            Err(error_message)
        }
    }
}

impl Power for PowerDaemon {
    fn battery(&mut self) -> Result<(), String> {
        self.apply_profile(battery, "Battery").map_err(err_str)
    }

    fn balanced(&mut self) -> Result<(), String> {
        self.apply_profile(balanced, "Balanced").map_err(err_str)
    }

    fn performance(&mut self) -> Result<(), String> {
        self.apply_profile(performance, "Performance").map_err(err_str)
    }

    fn get_graphics(&mut self) -> Result<String, String> {
        self.graphics.get_vendor().map_err(err_str)
    }

    fn get_profile(&mut self) -> Result<String, String> { Ok(self.power_profile.clone()) }

    fn get_switchable(&mut self) -> Result<bool, String> { Ok(self.graphics.can_switch()) }

    fn set_graphics(&mut self, vendor: &str) -> Result<(), String> {
        self.graphics.set_vendor(vendor).map_err(err_str)
    }

    fn get_graphics_power(&mut self) -> Result<bool, String> {
        self.graphics.get_power().map_err(err_str)
    }

    fn set_graphics_power(&mut self, power: bool) -> Result<(), String> {
        self.graphics.set_power(power).map_err(err_str)
    }

    fn auto_graphics_power(&mut self) -> Result<(), String> {
        self.graphics.auto_power().map_err(err_str)
    }
}

pub fn daemon() -> Result<(), String> {
    signal_handling();
    let pci_runtime_pm = std::env::var("S76_POWER_PCI_RUNTIME_PM").ok().map_or(false, |v| v == "1");

    info!(
        "Starting daemon{}",
        if pci_runtime_pm { " with pci runtime pm support enabled" } else { "" }
    );
    PCI_RUNTIME_PM.store(pci_runtime_pm, Ordering::SeqCst);

    info!("Connecting to dbus system bus");
    let c = Arc::new(Connection::new_system().map_err(err_str)?);

    let f = Factory::new_fn::<()>();
    let hotplug_signal = Arc::new(f.signal("HotPlugDetect", ()).sarg::<u64, _>("port"));
    let power_switch_signal =
        Arc::new(f.signal("PowerProfileSwitch", ()).sarg::<&str, _>("profile"));

    let daemon = PowerDaemon::new(power_switch_signal.clone(), c.clone())?;
    let nvidia_exists = !daemon.graphics.nvidia.is_empty();
    let daemon = Rc::new(RefCell::new(daemon));

    info!("Disabling NMI Watchdog (for kernel debugging only)");
    NmiWatchdog::default().set(b"0");

    // Get the NVIDIA device ID before potentially removing it.
    let nvidia_device_id = if nvidia_exists {
        fs::read_to_string("/sys/bus/pci/devices/0000:01:00.0/device").ok()
    } else {
        None
    };

    info!("Setting automatic graphics power");
    match daemon.borrow_mut().auto_graphics_power() {
        Ok(()) => (),
        Err(err) => {
            warn!("Failed to set automatic graphics power: {}", err);
        }
    }

    {
        info!("Initializing with the balanced profile");
        let mut daemon = daemon.borrow_mut();
        if let Err(why) = daemon.balanced() {
            warn!("Failed to set initial profile: {}", why);
        }

        daemon.initial_set = true;
    }

    info!("Registering dbus name {}", DBUS_NAME);
    c.register_name(DBUS_NAME, NameFlag::ReplaceExisting as u32).map_err(err_str)?;

    // Defines whether the value returned by the method should be appended.
    macro_rules! append {
        (true, $m:ident, $value:ident) => {
            $m.msg.method_return().append1($value)
        };
        (false, $m:ident, $value:ident) => {
            $m.msg.method_return()
        };
    }

    // Programs the message that should be printed.
    macro_rules! get_value {
        (true, $name:expr, $daemon:ident, $m:ident, $method:tt) => {{
            let value = $m.msg.read1()?;
            info!("DBUS Received {}({}) method", $name, value);
            $daemon.borrow_mut().$method(value)
        }};

        (false, $name:expr, $daemon:ident, $m:ident, $method:tt) => {{
            info!("DBUS Received {} method", $name);
            $daemon.borrow_mut().$method()
        }};
    }

    // Creates a new dbus method from an existing method in the daemon.
    macro_rules! method {
        ($method:tt, $name:expr, $append:tt, $print:tt) => {{
            let daemon = daemon.clone();
            f.method($name, (), move |m| {
                let result = get_value!($print, $name, daemon, m, $method);
                match result {
                    Ok(_value) => {
                        let mret = append!($append, m, _value);
                        Ok(vec![mret])
                    }
                    Err(err) => {
                        error!("{}", err);
                        Err(MethodErr::failed(&err))
                    }
                }
            })
        }};
    }

    info!("Adding dbus path {} with interface {}", DBUS_PATH, DBUS_IFACE);
    let tree = f.tree(()).add(
        f.object_path(DBUS_PATH, ()).introspectable().add(
            f.interface(DBUS_IFACE, ())
                .add_m(method!(performance, "Performance", false, false))
                .add_m(method!(balanced, "Balanced", false, false))
                .add_m(method!(battery, "Battery", false, false))
                .add_m(
                    method!(get_graphics, "GetGraphics", true, false).outarg::<&str, _>("vendor"),
                )
                .add_m(method!(get_profile, "GetProfile", true, false).outarg::<&str, _>("vendor"))
                .add_m(method!(set_graphics, "SetGraphics", false, true).inarg::<&str, _>("vendor"))
                .add_m(
                    method!(get_switchable, "GetSwitchable", true, false)
                        .outarg::<bool, _>("switchable"),
                )
                .add_m(
                    method!(get_graphics_power, "GetGraphicsPower", true, false)
                        .outarg::<bool, _>("power"),
                )
                .add_m(
                    method!(set_graphics_power, "SetGraphicsPower", false, true)
                        .inarg::<bool, _>("power"),
                )
                .add_m(method!(auto_graphics_power, "AutoGraphicsPower", false, false))
                .add_s(hotplug_signal.clone())
                .add_s(power_switch_signal.clone()),
        ),
    );

    tree.set_registered(&c, true).map_err(err_str)?;

    c.add_handler(tree);

    // Spawn hid backlight daemon
    let _hid_backlight = thread::spawn(|| hid_backlight::daemon());

    let mut fan_daemon = FanDaemon::new(nvidia_exists);

    let hpd_res = unsafe { HotPlugDetect::new(nvidia_device_id) };

    let mux_res = unsafe { DisplayPortMux::new() };

    let hpd = || -> [bool; 4] {
        if let Ok(ref hpd) = hpd_res {
            unsafe { hpd.detect() }
        } else {
            [false; 4]
        }
    };

    let mut last = hpd();

    info!("Handling dbus requests");
    while CONTINUE.load(Ordering::SeqCst) {
        c.incoming(1000).next();

        fan_daemon.step();

        let hpd = hpd();
        for i in 0..hpd.len() {
            if hpd[i] != last[i] && hpd[i] {
                info!("HotPlugDetect {}", i);
                c.send(hotplug_signal.msg(&DBUS_PATH.into(), &DBUS_NAME.into()).append1(i as u64))
                    .map_err(|()| "failed to send message".to_string())?;
            }
        }

        last = hpd;

        if let Ok(ref mux) = mux_res {
            unsafe {
                mux.step();
            }
        }
    }

    info!("daemon exited from loop");
    Ok(())
}
