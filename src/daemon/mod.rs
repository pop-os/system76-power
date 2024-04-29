// Copyright 2018-2021 System76 <info@system76.com>
//
// SPDX-License-Identifier: GPL-3.0-only

use anyhow::Context;
use std::{
    fmt::Display,
    fs,
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::Duration,
};
use tokio::{
    signal::unix::{signal, SignalKind},
    time::sleep,
};

use crate::{
    charge_thresholds::{get_charge_profiles, get_charge_thresholds, set_charge_thresholds},
    errors::ProfileError,
    fan::FanDaemon,
    graphics::{Graphics, GraphicsMode},
    hid_backlight,
    hotplug::{mux, Detect, HotPlugDetect},
    kernel_parameters::{KernelParameter, NmiWatchdog},
    runtime_pm::runtime_pm_quirks,
    DBUS_NAME, DBUS_PATH,
};

mod profiles;
use self::profiles::{balanced, battery, performance};

use system76_power_zbus::ChargeProfile;

const THRESHOLD_POLICY: &str = "com.system76.powerdaemon.set-charge-thresholds";

static CONTINUE: AtomicBool = AtomicBool::new(true);

async fn signal_handling() {
    let mut int = signal(SignalKind::interrupt()).unwrap();
    let mut hup = signal(SignalKind::hangup()).unwrap();
    let mut term = signal(SignalKind::terminate()).unwrap();

    let sig = tokio::select! {
        _ = int.recv() => "SIGINT",
        _ = hup.recv() => "SIGHUP",
        _ = term.recv() => "SIGTERM"
    };

    log::info!("caught signal: {}", sig);
    CONTINUE.store(false, Ordering::SeqCst);
}

// Disabled by default because some systems have quirky ACPI tables that fail to resume from
// suspension.
static PCI_RUNTIME_PM: AtomicBool = AtomicBool::new(false);

// TODO: Whitelist system76 hardware that's known to work with this setting.
pub(crate) fn pci_runtime_pm_support() -> bool { PCI_RUNTIME_PM.load(Ordering::SeqCst) }

struct PowerDaemon {
    initial_set:    bool,
    graphics:       Graphics,
    power_profile:  String,
    profile_errors: Vec<ProfileError>,
    connection:     zbus::Connection,
}

#[zbus::interface(name = "com.sytem76.PowerDaemon")]
impl PowerDaemon {
    async fn battery(
        &mut self,
        #[zbus(signal_context)] context: zbus::SignalContext<'_>,
    ) -> zbus::fdo::Result<()> {
        self.apply_profile(&context, battery, "Battery").await.map_err(zbus_error_from_display)
    }

    async fn balanced(
        &mut self,
        #[zbus(signal_context)] context: zbus::SignalContext<'_>,
    ) -> zbus::fdo::Result<()> {
        self.apply_profile(&context, balanced, "Balanced").await.map_err(zbus_error_from_display)
    }

    async fn performance(
        &mut self,
        #[zbus(signal_context)] context: zbus::SignalContext<'_>,
    ) -> zbus::fdo::Result<()> {
        self.apply_profile(&context, performance, "Performance")
            .await
            .map_err(zbus_error_from_display)
    }

    #[zbus(out_args("profile"))]
    async fn get_profile(&mut self) -> zbus::fdo::Result<String> { Ok(self.power_profile.clone()) }

    #[zbus(out_args("required"))]
    async fn get_external_displays_require_dgpu(&mut self) -> zbus::fdo::Result<bool> {
        self.graphics.get_external_displays_require_dgpu().map_err(zbus_error_from_display)
    }

    #[zbus(out_args("vendor"))]
    async fn get_default_graphics(&mut self) -> zbus::fdo::Result<String> {
        match self.graphics.get_default_graphics().map_err(zbus_error_from_display)? {
            GraphicsMode::Integrated => Ok("integrated".to_string()),
            GraphicsMode::Compute => Ok("compute".to_string()),
            GraphicsMode::Hybrid => Ok("hybrid".to_string()),
            GraphicsMode::Discrete => Ok("nvidia".to_string()),
        }
    }

    #[zbus(out_args("vendor"))]
    async fn get_graphics(&mut self) -> zbus::fdo::Result<String> {
        match self.graphics.get_vendor().map_err(zbus_error_from_display)? {
            GraphicsMode::Integrated => Ok("integrated".to_string()),
            GraphicsMode::Compute => Ok("compute".to_string()),
            GraphicsMode::Hybrid => Ok("hybrid".to_string()),
            GraphicsMode::Discrete => Ok("nvidia".to_string()),
        }
    }

    async fn set_graphics(&mut self, vendor: &str) -> zbus::fdo::Result<()> {
        let vendor = match vendor {
            "nvidia" => GraphicsMode::Discrete,
            "hybrid" => GraphicsMode::Hybrid,
            "compute" => GraphicsMode::Compute,
            _ => GraphicsMode::Integrated,
        };

        self.graphics.set_vendor(vendor).map_err(zbus_error_from_display)
    }

    #[zbus(out_args("desktop"))]
    async fn get_desktop(&mut self) -> zbus::fdo::Result<bool> { Ok(self.graphics.is_desktop()) }

    #[zbus(out_args("switchable"))]
    async fn get_switchable(&mut self) -> zbus::fdo::Result<bool> { Ok(self.graphics.can_switch()) }

    #[zbus(out_args("power"))]
    async fn get_graphics_power(&mut self) -> zbus::fdo::Result<bool> {
        self.graphics.get_power().map_err(zbus_error_from_display)
    }

    async fn set_graphics_power(&mut self, power: bool) -> zbus::fdo::Result<()> {
        self.graphics.set_power(power).map_err(zbus_error_from_display)
    }

    async fn auto_graphics_power(&mut self) -> zbus::fdo::Result<()> {
        self.graphics.auto_power().map_err(zbus_error_from_display)
    }

    #[zbus(out_args("start", "end"))]
    async fn get_charge_thresholds(&mut self) -> zbus::fdo::Result<(u8, u8)> {
        get_charge_thresholds().map_err(zbus_error_from_display)
    }

    async fn set_charge_thresholds(&mut self, thresholds: (u8, u8)) -> zbus::fdo::Result<()> {
        let polkit = zbus_polkit::policykit1::AuthorityProxy::new(&self.connection)
            .await
            .context("could not connect to polkit authority daemon")
            .map_err(zbus_error_from_display)?;

        let pid = std::process::id();

        let permitted = if pid == 0 {
            true
        } else {
            let subject = zbus_polkit::policykit1::Subject::new_for_owner(pid, None, None)
                .context("could not create policykit1 subject")
                .map_err(zbus_error_from_display)?;

            polkit
                .check_authorization(
                    &subject,
                    THRESHOLD_POLICY,
                    &std::collections::HashMap::new(),
                    Default::default(),
                    "",
                )
                .await
                .context("could not check policykit authorization")
                .map_err(zbus_error_from_display)?
                .is_authorized
        };

        if permitted {
            set_charge_thresholds(thresholds).map_err(zbus_error_from_display)
        } else {
            Err(zbus_error_from_display("Operation not permitted by Polkit"))
        }
    }

    #[zbus(out_args("profiles"))]
    async fn get_charge_profiles(&mut self) -> zbus::fdo::Result<Vec<ChargeProfile>> {
        Ok(get_charge_profiles())
    }

    #[zbus(signal)]
    async fn hot_plug_detect(context: &zbus::SignalContext<'_>, port: u64) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn power_profile_switch(
        context: &zbus::SignalContext<'_>,
        profile: &str,
    ) -> zbus::Result<()>;
}

impl PowerDaemon {
    fn new(connection: zbus::Connection) -> anyhow::Result<Self> {
        let graphics = Graphics::new()?;

        Ok(Self {
            initial_set: false,
            graphics,
            power_profile: String::new(),
            profile_errors: Vec::new(),
            connection,
        })
    }

    async fn apply_profile(
        &mut self,
        context: &zbus::SignalContext<'_>,
        func: fn(&mut Vec<ProfileError>, bool),
        name: &str,
    ) -> Result<(), String> {
        if self.power_profile == name {
            log::info!("profile was already set");
            return Ok(());
        }

        let _res = Self::power_profile_switch(context, name).await;

        func(&mut self.profile_errors, self.initial_set);

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

#[tokio::main(flavor = "current_thread")]
#[allow(clippy::too_many_lines)]
pub async fn daemon() -> anyhow::Result<()> {
    let signal_handling_fut = signal_handling();

    let pci_runtime_pm = std::env::var("S76_POWER_PCI_RUNTIME_PM").ok().map_or(false, |v| v == "1");

    PCI_RUNTIME_PM.store(pci_runtime_pm, Ordering::SeqCst);

    let connection =
        zbus::Connection::system().await.context("failed to create zbus connection")?;

    let context = zbus::SignalContext::new(&connection, DBUS_PATH)
        .context("unable to create signal context")?;

    let mut daemon = PowerDaemon::new(connection)?;

    let nvidia_exists = !daemon.graphics.nvidia.is_empty();

    NmiWatchdog.set(b"0");

    // Get the NVIDIA device ID before potentially removing it.
    let nvidia_device_id = if nvidia_exists {
        fs::read_to_string("/sys/bus/pci/devices/0000:01:00.0/device").ok()
    } else {
        None
    };

    match daemon.auto_graphics_power().await {
        Ok(()) => (),
        Err(err) => {
            log::warn!("Failed to set automatic graphics power: {}", err);
        }
    }

    match runtime_pm_quirks() {
        Ok(()) => (),
        Err(err) => {
            log::warn!("Failed to set runtime power management quirks: {}", err);
        }
    }

    if let Err(why) = daemon.balanced(context.clone()).await {
        log::warn!("Failed to set initial profile: {}", why);
    }

    daemon.initial_set = true;

    let _connection = zbus::ConnectionBuilder::system()
        .context("failed to create zbus connection builder")?
        .name(DBUS_NAME)
        .context("unable to register name")?
        .serve_at(DBUS_PATH, daemon)
        .context("unable to serve")?
        .build()
        .await
        .context("unable to create system service for com.system76.PowerDaemon")?;

    // Spawn hid backlight daemon
    let _hid_backlight = thread::spawn(hid_backlight::daemon);
    let mut fan_daemon = FanDaemon::new(nvidia_exists);
    let mut hpd_res = unsafe { HotPlugDetect::new(nvidia_device_id) };
    let mux_res = unsafe { mux::DisplayPortMux::new() };
    let mut hpd = || -> [bool; 4] {
        if let Ok(ref mut hpd) = hpd_res {
            unsafe { hpd.detect() }
        } else {
            [false; 4]
        }
    };

    let main_loop = async move {
        let mut last = hpd();

        while CONTINUE.load(Ordering::SeqCst) {
            sleep(Duration::from_millis(1000)).await;

            fan_daemon.step();

            let hpd = hpd();
            for i in 0..hpd.len() {
                if hpd[i] != last[i] && hpd[i] {
                    log::info!("HotPlugDetect {}", i);
                    let _res = PowerDaemon::hot_plug_detect(&context, i as u64).await;
                }
            }

            last = hpd;

            if let Ok(ref mux) = mux_res {
                unsafe {
                    mux.step();
                }
            }
        }
    };

    log::info!("Handling dbus requests");
    futures_lite::future::zip(signal_handling_fut, main_loop).await;

    log::info!("daemon exited from loop");
    Ok(())
}

fn zbus_error_from_display<E: Display>(why: E) -> zbus::fdo::Error {
    zbus::fdo::Error::Failed(format!("{}", why))
}
