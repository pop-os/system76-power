use std::io;
use sysfs_class::{SysClass, HwMon};

pub struct FanDaemon {
    curve: FanCurve,
    platform: HwMon,
    coretemp: HwMon,
}

impl FanDaemon {
    pub fn new() -> io::Result<FanDaemon> {
        //TODO: Support multiple hwmons
        let mut platform_opt = None;
        let mut coretemp_opt = None;

        for hwmon in HwMon::all()? {
            if let Ok(name) = hwmon.name() {
                info!("hwmon: {}", name);

                match name.as_str() {
                    "system76" => (), //TODO: Support laptops
                    "system76_io" => platform_opt = Some(hwmon),
                    "coretemp" => coretemp_opt = Some(hwmon),
                    _ => ()
                }
            }
        }

        Ok(FanDaemon {
            curve: FanCurve::standard(),
            platform: platform_opt.ok_or_else(|| io::Error::new(
                io::ErrorKind::NotFound,
                "platform hwmon not found"
            ))?,
            coretemp: coretemp_opt.ok_or_else(|| io::Error::new(
                io::ErrorKind::NotFound,
                "coretemp hwmon not found"
            ))?,
        })
    }

    pub fn step(&self) {
        let mut duty_opt = None;
        if let Ok(temp) = self.coretemp.temp(1) {
            if let Ok(input) = temp.input() {
                let c = f64::from(input) / 1000.0;
                duty_opt = self.curve.get_duty((c * 100.0) as i16);
            }
        }

        if let Some(duty) = duty_opt {
            //TODO: Implement in system76-io-dkms
            //let _ = self.platform.write_file("pwm1_enable", "1");

            let duty_str = format!("{}", (u32::from(duty) * 255)/10000);
            let _ = self.platform.write_file("pwm1", &duty_str);
            let _ = self.platform.write_file("pwm2", &duty_str);
        } else {
            //TODO: Implement in system76-io-dkms
            //let _ = self.platform.write_file("pwm1_enable", "2");
        }
    }
}

impl Drop for FanDaemon {
    fn drop(&mut self) {
        let _ = self.platform.write_file("pwm1_enable", "2");
    }
}

pub struct FanPoint {
    // Temperature in hundreths of a degree, 10000 = 100C
    temp: i16,
    // duty in hundredths of a percent, 10000 = 100%
    duty: u16,
}

impl FanPoint {
    pub fn new(temp: i16, duty: u16) -> Self {
        Self {
            temp,
            duty
        }
    }
}

pub struct FanCurve {
    points: Vec<FanPoint>
}

impl FanCurve {
    pub fn new(points: Vec<FanPoint>) -> Self {
        Self {
            points
        }
    }

    pub fn standard() -> Self {
        Self {
            points: vec![
                FanPoint::new(20_00, 30_00),
                FanPoint::new(30_00, 35_00),
                FanPoint::new(40_00, 42_50),
                FanPoint::new(50_00, 52_50),
                FanPoint::new(65_00, 10_000)
            ]
        }
    }

    pub fn get_duty(&self, temp: i16) -> Option<u16> {
        let mut i = 0;

        // If the temp is less than the first point, return the first point duty
        if let Some(first) = self.points.get(i) {
            if temp < first.temp {
                return Some(first.duty);
            }
        }

        while i + 1 < self.points.len() {
            let prev = &self.points[i];
            let next = &self.points[i +  1];

            // If the temp matches the next point, return the next point duty
            if temp == next.temp {
                return Some(next.duty);
            }

            // If the temp matches the previous point, return the previous point duty
            if temp == prev.temp {
                return Some(prev.duty);
            }

            // If the temp is in between the previous and next points, interpolate the duty
            if prev.temp < temp && next.temp > temp {
                let dtemp = next.temp - prev.temp;
                let dduty = next.duty - prev.duty;

                let slope = f32::from(dduty) / f32::from(dtemp);

                let temp_offset = temp - prev.temp;
                let duty_offset = (slope * f32::from(temp_offset)).round();

                return Some(prev.duty + (duty_offset as u16));
            }

            i += 1;
        }

        // If the temp is greater than the last point, return the last point duty
        if let Some(last) = self.points.get(i) {
            if temp > last.temp {
                return Some(last.duty);
            }
        }

        // If there are no points, return None
        None
    }
}
