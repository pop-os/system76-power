// Copyright 2018-2021 System76 <info@system76.com>
//
// SPDX-License-Identifier: GPL-3.0-only

#![allow(clippy::inconsistent_digit_grouping)]

use std::{
    cell::Cell,
    cmp,
    collections::VecDeque,
    fs,
    io,
    process::{Command, Stdio},
};
use sysfs_class::{HwMon, SysClass};

const COOLDOWN_SIZE: usize = from_seconds(2) as usize;
const HEATUP_SIZE: usize = from_seconds(1) as usize;

const fn from_seconds (seconds: u8) -> u8 {
    const INTERVAL: usize = 1000;

    return (1000 * (seconds as usize) / INTERVAL) as u8;
}

#[derive(Debug, thiserror::Error)]
pub enum FanDaemonError {
    #[error("failed to collect hwmon devices: {}", _0)]
    HwmonDevices(io::Error),
    #[error("platform hwmon not found")]
    PlatformHwmonNotFound,
    #[error("cpu hwmon not found")]
    CpuHwmonNotFound,
}

pub struct FanDaemon {
    curve:             FanCurve,
    amdgpus:           Vec<HwMon>,
    platforms:         Vec<HwMon>,
    cpus:              Vec<HwMon>,
    nvidia_exists:     bool,
    displayed_warning: Cell<bool>,
    fan_cooldown:      VecDeque<u8>,
    fan_heatup:        VecDeque<u8>,
    last_duty:         u8,
}

impl FanDaemon {
    pub fn new(nvidia_exists: bool, profile: String) -> Self {
        let model = fs::read_to_string("/sys/class/dmi/id/product_version").unwrap_or_default();
        let mut daemon = FanDaemon {
            curve: match model.trim() {
                "thelio-major-r1" => FanCurve::threadripper2(),
                "thelio-major-r2" | "thelio-major-r2.1" | "thelio-major-b1" | "thelio-major-b2"
                | "thelio-major-b3" | "thelio-mega-r1" | "thelio-mega-r1.1" => FanCurve::hedt(),
                "thelio-massive-b1" => FanCurve::xeon(),
                "galp5" => FanCurve::galp5(profile),
                _ => FanCurve::standard(),
            },
            amdgpus: Vec::new(),
            platforms: Vec::new(),
            cpus: Vec::new(),
            nvidia_exists,
            displayed_warning: Cell::new(false),
            fan_cooldown: VecDeque::with_capacity(COOLDOWN_SIZE),
            fan_heatup: VecDeque::with_capacity(HEATUP_SIZE),
            last_duty: 0,
        };

        if let Err(err) = daemon.discover() {
            log::error!("fan daemon: {}", err);
        }

        daemon
    }

    /// Discover all utilizable hwmon devices
    fn discover(&mut self) -> Result<(), FanDaemonError> {
        self.amdgpus.clear();
        self.platforms.clear();
        self.cpus.clear();

        for hwmon in HwMon::all().map_err(FanDaemonError::HwmonDevices)? {
            if let Ok(name) = hwmon.name() {
                log::debug!("hwmon: {}", name);

                match name.as_str() {
                    "amdgpu" => self.amdgpus.push(hwmon),
                    "system76_acpi" => self.platforms.push(hwmon),
                    "system76_io" => self.platforms.push(hwmon),
                    "coretemp" | "k10temp" => self.cpus.push(hwmon),
                    _ => (),
                }
            }
        }

        if self.platforms.is_empty() {
            return Err(FanDaemonError::PlatformHwmonNotFound);
        }

        if self.cpus.is_empty() {
            return Err(FanDaemonError::CpuHwmonNotFound);
        }

        Ok(())
    }

    /// Get the maximum measured temperature from any CPU / GPU on the system, in
    /// thousandths of a Celsius. Thousandths celsius is the standard Linux hwmon temperature unit.
    pub fn get_temp(&self) -> Option<u32> {
        let mut temp_opt = self
            .cpus
            .iter()
            .chain(self.amdgpus.iter())
            .filter_map(|sensor| sensor.temp(1).ok())
            .filter_map(|temp| temp.input().ok())
            .fold(None, |mut temp_opt, input| {
                // Assume temperatures are always above freezing
                if temp_opt.map_or(true, |x| input as u32 > x) {
                    log::warn!("highest hwmon cpu/gpu temp: {}", input);
                    temp_opt = Some(input as u32);
                }

                temp_opt
            });

        // Fetch NVIDIA temperatures from the `nvidia-smi` tool when it exists.
        if self.nvidia_exists && !self.displayed_warning.get() {
            let mut nv_temp = 0;
            match nvidia_temperatures(|temp| nv_temp = cmp::max(temp, nv_temp)) {
                Ok(()) => {
                    if nv_temp != 0 {
                        log::debug!("highest nvidia temp: {}", nv_temp);
                        temp_opt =
                            Some(temp_opt.map_or(nv_temp, |temp| cmp::max(nv_temp * 1000, temp)));
                    }
                }
                Err(why) => {
                    log::warn!("failed to get temperature of NVIDIA GPUs: {}", why);
                    self.displayed_warning.set(true);
                }
            }
        }

        log::debug!("current temp: {:?}", temp_opt);

        temp_opt
    }

    /// Get the correct duty cycle for a temperature in thousandths Celsius, from 0 to 255
    /// Thousandths celsius is the standard Linux hwmon temperature unit
    /// 0 to 255 is the standard Linux hwmon pwm unit
    pub fn get_duty(&self, temp: u32) -> Option<u8> {
        self.curve
            .get_duty((temp / 10) as i16)
            .map(|duty| (((u32::from(duty)) * 255) / 10_000) as u8)
    }

    /// Set the current duty cycle, from 0 to 255
    /// 0 to 255 is the standard Linux hwmon pwm unit
    pub fn set_duty(&mut self, duty_opt: Option<u8>) {
        if let Some(duty) = duty_opt {
            self.last_duty = duty;
            let duty_str = format!("{}", duty);
            for platform in &self.platforms {
                let _ = platform.write_file("pwm1_enable", "2");
                let _ = platform.write_file("pwm2_enable", "2");
                let _ = platform.write_file("pwm1", &duty_str);
                let _ = platform.write_file("pwm2", &duty_str);
            }
        } else {
            for platform in &self.platforms {
                let _ = platform.write_file("pwm1_enable", "2");
            }
        }
    }

    fn smooth_duty(&mut self, duty_opt: Option<u8>) -> Option<u8> {
        let SMOOTH_FANS = self.curve.SMOOTH_FANS.unwrap_or(0);
        let SMOOTH_FANS_DOWN = self.curve.SMOOTH_FANS_DOWN.unwrap_or(SMOOTH_FANS);
        let SMOOTH_FANS_UP = self.curve.SMOOTH_FANS_UP.unwrap_or(SMOOTH_FANS);
        let SMOOTH_FANS_MIN = self.curve.SMOOTH_FANS_MIN;
        let MAX_JUMP_DOWN = (255 / SMOOTH_FANS_DOWN) as u8;
        let MAX_JUMP_UP = (255 / SMOOTH_FANS_UP) as u8;

        if let Some(duty) = duty_opt {
            let last_duty = self.last_duty;
            let mut next_duty = duty;

            self.fan_heatup.truncate(HEATUP_SIZE - 1);
            self.fan_heatup.push_front(next_duty);
            next_duty = *self.fan_heatup.iter().min().unwrap();

            self.fan_cooldown.truncate(COOLDOWN_SIZE - 1);
            self.fan_cooldown.push_front(next_duty);
            next_duty = *self.fan_cooldown.iter().max().unwrap();

            log::warn!("last_duty:{}, duty:{}, next_duty:{}", last_duty, duty, next_duty);

            // ramping down
            if next_duty < last_duty {
                // out of bounds (lower) safeguard
                let smoothed = last_duty.saturating_sub(MAX_JUMP_DOWN);

                // use smoothed value if above min and if smoothed is closer than raw
                if smoothed > SMOOTH_FANS_MIN {
                    next_duty = cmp::max(smoothed, next_duty);
                }

                log::warn!("ramping down, last_duty:{}, smoothed:{}, next_duty:{}", last_duty, smoothed, next_duty);
            }

            // ramping up
            if next_duty > last_duty {
                // out of bounds (higher) safeguard
                let smoothed = last_duty.saturating_add(MAX_JUMP_UP);

                // use smoothed value if above min and if smoothed is closer than raw
                if smoothed > SMOOTH_FANS_MIN {
                    next_duty = cmp::min(smoothed, next_duty);
                }

                log::warn!("ramping up, last_duty:{}, smoothed:{}, next_duty:{}", last_duty, smoothed, next_duty);
            }

            return Some(next_duty);
        }

        Some(0)
    }

    /// Calculate the correct duty cycle and apply it to all fans
    pub fn step(&mut self) {
        if let Ok(()) = self.discover() {
            let duty_opt: Option<u8> = self.smooth_duty(self.get_temp().and_then(|temp| self.get_duty(temp)));
            self.set_duty(duty_opt);
        }
    }
}

impl Drop for FanDaemon {
    fn drop(&mut self) { self.set_duty(None); }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FanPoint {
    // Temperature in hundredths of a degree, 10000 = 100C
    temp: i16,
    // duty in hundredths of a percent, 10000 = 100%
    duty: u16,
}

impl FanPoint {
    pub fn new(temp: i16, duty: u16) -> Self { Self { temp, duty } }

    /// Find the duty between two points and a given temperature, if the temperature
    /// lies within this range.
    fn get_duty_between_points(self, next: FanPoint, temp: i16) -> Option<u16> {
        // If the temp matches the next point, return the next point duty
        if temp == next.temp {
            return Some(next.duty);
        }

        // If the temp matches the previous point, return the previous point duty
        if temp == self.temp {
            return Some(self.duty);
        }

        // If the temp is in between the previous and next points, interpolate the duty
        if self.temp < temp && next.temp > temp {
            return Some(self.duty);
            // return Some(self.interpolate_duties(next, temp));
        }

        None
    }

    /// Interpolates the current duty with that of the given next point and temperature.
    fn interpolate_duties(self, next: FanPoint, temp: i16) -> u16 {
        let dtemp = next.temp - self.temp;
        let dduty = next.duty - self.duty;

        let slope = f32::from(dduty) / f32::from(dtemp);

        let temp_offset = temp - self.temp;
        let duty_offset = (slope * f32::from(temp_offset)).round();

        self.duty + (duty_offset as u16)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FanCurve {
    points:             Vec<FanPoint>,
    SMOOTH_FANS:        Option<u8>,
    SMOOTH_FANS_DOWN:   Option<u8>,
    SMOOTH_FANS_MIN:    u8,
    SMOOTH_FANS_UP:     Option<u8>,
}

impl Default for FanCurve {
    fn default() -> FanCurve {
        FanCurve {
            points: Vec::default(),
            SMOOTH_FANS: None,
            SMOOTH_FANS_DOWN: Some(from_seconds(12)),
            SMOOTH_FANS_MIN: 0,
            SMOOTH_FANS_UP: Some(from_seconds(8)),
        }
    }
}

impl FanCurve {
    /// Adds a point to the fan curve
    #[must_use]
    pub fn append(mut self, temp: i16, duty: u16) -> Self {
        self.points.push(FanPoint::new(temp, duty));
        self
    }

    /// The standard fan curve
    pub fn standard() -> Self {
        Self::default()
            .append(44_99, 0_00)
            .append(45_00, 30_00)
            .append(55_00, 35_00)
            .append(65_00, 40_00)
            .append(75_00, 50_00)
            .append(78_00, 60_00)
            .append(81_00, 70_00)
            .append(84_00, 80_00)
            .append(86_00, 90_00)
            .append(88_00, 100_00)
    }

    /// test galp5 curve
    pub fn galp5(profile: String) -> Self {
        let mut curve = Self::default()
            .append(69_00, 0_00)
            .append(70_00, 25_00)
            .append(79_99, 25_00)
            .append(80_00, 40_00)
            .append(87_99, 40_00)
            .append(88_00, 100_00);

        if profile == String::from("performance") {
            curve = Self::default()
                .append(69_00, 0_00)
                .append(70_00, 25_00)
                .append(79_99, 25_00)
                .append(80_00, 100_00);

            curve.SMOOTH_FANS_UP = Some(from_seconds(4));
        }

        return curve;
    }

    /// Fan curve for threadripper 2
    pub fn threadripper2() -> Self {
        Self::default()
            .append(00_00, 30_00)
            .append(40_00, 40_00)
            .append(47_50, 50_00)
            .append(55_00, 65_00)
            .append(62_50, 85_00)
            .append(66_25, 100_00)
    }

    /// Fan curve for HEDT systems
    pub fn hedt() -> Self {
        Self::default()
            .append(00_00, 30_00)
            .append(50_00, 35_00)
            .append(60_00, 45_00)
            .append(70_00, 55_00)
            .append(74_00, 60_00)
            .append(76_00, 70_00)
            .append(78_00, 80_00)
            .append(81_00, 100_00)
    }

    /// Fan curve for xeon
    pub fn xeon() -> Self {
        Self::default()
            .append(00_00, 40_00)
            .append(50_00, 40_00)
            .append(55_00, 45_00)
            .append(60_00, 50_00)
            .append(65_00, 55_00)
            .append(70_00, 60_00)
            .append(72_00, 65_00)
            .append(74_00, 80_00)
            .append(76_00, 85_00)
            .append(77_00, 90_00)
            .append(78_00, 100_00)
    }

    pub fn get_duty(&self, temp: i16) -> Option<u16> {
        // If the temp is less than the first point, return the first point duty
        if let Some(first) = self.points.first() {
            if temp < first.temp {
                return Some(first.duty);
            }
        }

        // Use when we upgrade to 1.28.0
        // for &[prev, next] in self.points.windows(2) {

        for window in self.points.windows(2) {
            let prev = window[0];
            let next = window[1];
            if let Some(duty) = prev.get_duty_between_points(next, temp) {
                return Some(duty);
            }
        }

        // If the temp is greater than the last point, return the last point duty
        if let Some(last) = self.points.last() {
            if temp > last.temp {
                return Some(last.duty);
            }
        }

        // If there are no points, return None
        None
    }
}

pub fn nvidia_temperatures<F: FnMut(u32)>(func: F) -> io::Result<()> {
    let output = Command::new("nvidia-smi")
        .arg("--query-gpu=temperature.gpu")
        .arg("--format=csv,noheader")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .stdin(Stdio::null())
        .output()?;

    let stdout = String::from_utf8(output.stdout)
        .map_err(|_| io::Error::new(io::ErrorKind::Other, "non-utf8 output"))?;

    stdout.lines().filter_map(|line| line.parse::<u32>().ok()).for_each(func);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duty_interpolation() {
        let fan_point = FanPoint::new(20_00, 30_00);
        let next_point = FanPoint::new(30_00, 35_00);

        assert_eq!(fan_point.get_duty_between_points(next_point, 1500), None);
        assert_eq!(fan_point.get_duty_between_points(next_point, 2000), Some(3000));
        assert_eq!(fan_point.get_duty_between_points(next_point, 3000), Some(3500));
        assert_eq!(fan_point.get_duty_between_points(next_point, 3250), None);
        assert_eq!(fan_point.get_duty_between_points(next_point, 3500), None);
    }

    #[test]
    fn standard_points() {
        let standard = FanCurve::standard();

        assert_eq!(standard.get_duty(0), Some(0));
        assert_eq!(standard.get_duty(4499), Some(0));
        assert_eq!(standard.get_duty(4500), Some(3000));
        assert_eq!(standard.get_duty(5500), Some(3500));
        assert_eq!(standard.get_duty(6500), Some(4000));
        assert_eq!(standard.get_duty(7500), Some(5000));
        assert_eq!(standard.get_duty(7800), Some(6000));
        assert_eq!(standard.get_duty(8100), Some(7000));
        assert_eq!(standard.get_duty(8400), Some(8000));
        assert_eq!(standard.get_duty(8600), Some(9000));
        assert_eq!(standard.get_duty(8800), Some(10000));
        assert_eq!(standard.get_duty(10000), Some(10000));
    }

    #[test]
    fn hedt_points() {
        let hedt = FanCurve::hedt();

        assert_eq!(hedt.get_duty(0), Some(3000));
        assert_eq!(hedt.get_duty(5000), Some(3500));
        assert_eq!(hedt.get_duty(6000), Some(4500));
        assert_eq!(hedt.get_duty(7000), Some(5500));
        assert_eq!(hedt.get_duty(7400), Some(6000));
        assert_eq!(hedt.get_duty(7600), Some(7000));
        assert_eq!(hedt.get_duty(7800), Some(8000));
        assert_eq!(hedt.get_duty(8100), Some(10000));
        assert_eq!(hedt.get_duty(10000), Some(10000));
    }

    #[test]
    fn threadripper2_points() {
        let threadripper2 = FanCurve::threadripper2();

        assert_eq!(threadripper2.get_duty(0), Some(3000));
        assert_eq!(threadripper2.get_duty(4000), Some(4000));
        assert_eq!(threadripper2.get_duty(4750), Some(5000));
        assert_eq!(threadripper2.get_duty(5500), Some(6500));
        assert_eq!(threadripper2.get_duty(6250), Some(8500));
        assert_eq!(threadripper2.get_duty(6625), Some(10000));
        assert_eq!(threadripper2.get_duty(10000), Some(10000));
    }

    #[test]
    fn xeon_points() {
        let xeon = FanCurve::xeon();

        assert_eq!(xeon.get_duty(0), Some(4000));
        assert_eq!(xeon.get_duty(5000), Some(4000));
        assert_eq!(xeon.get_duty(5500), Some(4500));
        assert_eq!(xeon.get_duty(6000), Some(5000));
        assert_eq!(xeon.get_duty(6500), Some(5500));
        assert_eq!(xeon.get_duty(7000), Some(6000));
        assert_eq!(xeon.get_duty(7200), Some(6500));
        assert_eq!(xeon.get_duty(7400), Some(8000));
        assert_eq!(xeon.get_duty(7600), Some(8500));
        assert_eq!(xeon.get_duty(7700), Some(9000));
        assert_eq!(xeon.get_duty(7800), Some(10000));
        assert_eq!(xeon.get_duty(10000), Some(10000));
    }
}
