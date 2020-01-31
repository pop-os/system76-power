use std::{
    cell::Cell,
    cmp, fs, io,
    process::{Command, Stdio},
};
use sysfs_class::{HwMon, SysClass};

#[derive(Debug, Error)]
pub enum FanDaemonError {
    #[error(display = "failed to collect hwmon devices: {}", _0)]
    HwmonDevices(io::Error),
    #[error(display = "platform hwmon not found")]
    PlatformHwmonNotFound,
    #[error(display = "cpu hwmon not found")]
    CpuHwmonNotFound,
}

pub struct FanDaemon {
    curve:             FanCurve,
    amdgpus:           Vec<HwMon>,
    platforms:         Vec<HwMon>,
    cpus:              Vec<HwMon>,
    nvidia_exists:     bool,
    displayed_warning: Cell<bool>,
}

impl FanDaemon {
    pub fn new(nvidia_exists: bool) -> Self {
        let model = fs::read_to_string("/sys/class/dmi/id/product_version")
            .unwrap_or(String::new());
        let mut daemon = FanDaemon {
            curve: match model.trim() {
                "thelio-major-r1" => FanCurve::threadripper(),
                "thelio-major-r2" | 
                "thelio-mega-r1" => FanCurve::threadripper3(),
                "thelio-major-b1" | 
                "thelio-mega-b1" => FanCurve::corex(),
                "thelio-massive-b1" => FanCurve::xeon(),
                _ => FanCurve::standard()
            },
            amdgpus: Vec::new(),
            platforms: Vec::new(),
            cpus: Vec::new(),
            nvidia_exists,
            displayed_warning: Cell::new(false),
        };

        if let Err(err) = daemon.discover() {
            error!("fan daemon: {}", err);
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
                debug!("hwmon: {}", name);

                match name.as_str() {
                    "amdgpu" => self.amdgpus.push(hwmon),
                    "system76" => (), // TODO: Support laptops
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
        let mut temp_opt = self.cpus.iter()
            .chain(self.amdgpus.iter())
            .filter_map(|sensor| sensor.temp(1).ok())
            .filter_map(|temp| temp.input().ok())
            .fold(None, |mut temp_opt, input| {
                if temp_opt.map_or(true, |x| input > x) {
                    debug!("highest hwmon cpu/gpu temp: {}", input);
                    temp_opt = Some(input);
                }

                temp_opt
            });

        // Fetch NVIDIA temperatures from the `nvidia-smi` tool when it exists.
        if self.nvidia_exists && !self.displayed_warning.get() {
            let mut nv_temp = 0;
            match nvidia_temperatures(|temp| nv_temp = cmp::max(temp, nv_temp)) {
                Ok(()) => {
                    if nv_temp != 0 {
                        debug!("highest nvidia temp: {}", nv_temp);
                        temp_opt =
                            Some(temp_opt.map_or(nv_temp, |temp| cmp::max(nv_temp * 1000, temp)));
                    }
                }
                Err(why) => {
                    warn!("failed to get temperature of NVIDIA GPUs: {}", why);
                    self.displayed_warning.set(true);
                }
            }
        }

        debug!("current temp: {:?}", temp_opt);

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
    pub fn set_duty(&self, duty_opt: Option<u8>) {
        if let Some(duty) = duty_opt {
            let duty_str = format!("{}", duty);
            for platform in self.platforms.iter() {
                let _ = platform.write_file("pwm1_enable", "1");
                let _ = platform.write_file("pwm1", &duty_str);
                let _ = platform.write_file("pwm2", &duty_str);
            }
        } else {
            for platform in self.platforms.iter() {
                let _ = platform.write_file("pwm1_enable", "2");
            }
        }
    }

    /// Calculate the correct duty cycle and apply it to all fans
    pub fn step(&mut self) {
        if let Ok(()) = self.discover() {
            self.set_duty(self.get_temp().and_then(|temp| self.get_duty(temp)));
        }
    }
}

impl Drop for FanDaemon {
    fn drop(&mut self) { self.set_duty(None); }
}

#[derive(Clone, Copy, Debug, PartialEq)]
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
            return Some(self.interpolate_duties(next, temp));
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

#[derive(Clone, Debug, Default, PartialEq)]
pub struct FanCurve {
    points: Vec<FanPoint>,
}

impl FanCurve {
    /// Adds a point to the fan curve
    pub fn append(mut self, temp: i16, duty: u16) -> Self {
        self.points.push(FanPoint::new(temp, duty));
        self
    }

    /// The standard fan curve
    pub fn standard() -> Self {
        Self::default()
            .append(44_99,   0_00)
            .append(45_00,  30_00)
            .append(55_00,  35_00)
            .append(65_00,  40_00)
            .append(75_00,  45_00)
            .append(80_00,  50_00)
            .append(90_00, 100_00)
    }

    /// Adjusted fan curve for core-x
    pub fn corex() -> Self {
        Self::default()
            .append(44_99,   0_00)
            .append(45_00,  40_00)
            .append(55_00,  50_00)
            .append(65_00,  65_00)
            .append(75_00,  85_00)
            .append(80_00, 100_00)
    }

    /// Adjusted fan curve for threadripper
    pub fn threadripper() -> Self {
        Self::default()
            .append(39_99,   0_00)
            .append(40_00,  40_00)
            .append(47_50,  50_00)
            .append(55_00,  65_00)
            .append(62_50,  85_00)
            .append(66_25, 100_00)
    }

    /// Adjusted fan curve for threadripper 3
    pub fn threadripper3() -> Self {
        Self::default()
            .append(00_00,  30_00)
            .append(50_00,  35_00)
            .append(60_00,  45_00)
            .append(70_00,  55_00)
            .append(74_00,  60_00)
            .append(76_00,  70_00)
            .append(78_00,  80_00)
            .append(81_00, 100_00)
    }

    /// Adjusted fan curve for xeon
    pub fn xeon() -> Self {
        Self::default()
            .append(00_00,  40_00)
            .append(50_00,  40_00)
            .append(55_00,  45_00)
            .append(60_00,  50_00)
            .append(65_00,  55_00)
            .append(70_00,  60_00)
            .append(75_00,  65_00)
            .append(78_00,  80_00)
            .append(81_00,  85_00)
            .append(83_00,  90_00)
            .append(85_00, 100_00)
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
        assert_eq!(standard.get_duty(7500), Some(4500));
        assert_eq!(standard.get_duty(8000), Some(5000));
        assert_eq!(standard.get_duty(9000), Some(10000));
        assert_eq!(standard.get_duty(10000), Some(10000));
    }

    #[test]
    fn threadripper3_points() {
        let threadripper3 = FanCurve::threadripper3();

        assert_eq!(standard.get_duty(0), Some(3000));
        assert_eq!(standard.get_duty(5000), Some(3500));
        assert_eq!(standard.get_duty(6000), Some(4500));
        assert_eq!(standard.get_duty(7000), Some(5500));
        assert_eq!(standard.get_duty(7400), Some(6000));
        assert_eq!(standard.get_duty(7600), Some(7000));
        assert_eq!(standard.get_duty(7800), Some(8000));
        assert_eq!(standard.get_duty(8100), Some(10000));
        assert_eq!(standard.get_duty(10000), Some(10000));
    }

    #[test]
    fn corex_points() {
        let corex = FanCurve::corex();

        assert_eq!(corex.get_duty(0), Some(0));
        assert_eq!(corex.get_duty(4499), Some(0));
        assert_eq!(corex.get_duty(4500), Some(4000));
        assert_eq!(corex.get_duty(5500), Some(5000));
        assert_eq!(corex.get_duty(6500), Some(6500));
        assert_eq!(corex.get_duty(7500), Some(8500));
        assert_eq!(corex.get_duty(8000), Some(10000));
        assert_eq!(corex.get_duty(10000), Some(10000));
    }

    #[test]
    fn xeon_points() {
        let xeon = FanCurve::xeon();

        assert_eq!(corex.get_duty(0), Some(4000));
        assert_eq!(corex.get_duty(5000), Some(4000));
        assert_eq!(corex.get_duty(5500), Some(4500));
        assert_eq!(corex.get_duty(6000), Some(5000));
        assert_eq!(corex.get_duty(6500), Some(5500));
        assert_eq!(corex.get_duty(7000), Some(6000));
        assert_eq!(corex.get_duty(7500), Some(6500));
        assert_eq!(corex.get_duty(7800), Some(8000));
        assert_eq!(corex.get_duty(8100), Some(8500));
        assert_eq!(corex.get_duty(8300), Some(9000));
        assert_eq!(corex.get_duty(8500), Some(10000));
        assert_eq!(corex.get_duty(10000), Some(10000));
    }
}
