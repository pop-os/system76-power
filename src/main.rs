use std::io;

use backlight::Backlight;
use kbd_backlight::KeyboardBacklight;

mod backlight;
mod kbd_backlight;
mod util;

fn power() -> io::Result<()> {
    {
        let backlight = Backlight::new("intel_backlight")?;
        let brightness = backlight.actual_brightness()?;
        let max_brightness = backlight.max_brightness()?;
        let ratio = (brightness as f64)/(max_brightness as f64);
        let power = 0.7 + 3.0 * ratio;
        let percent = (ratio * 100.0) as u64;
        println!("Backlight: {}/{} = {}% ~{:.2} W", brightness, max_brightness, percent, power);
    }
    
    {
        let backlight = KeyboardBacklight::new()?;
        let brightness = backlight.brightness()?;
        let max_brightness = backlight.max_brightness()?;
        let ratio = (brightness as f64)/(max_brightness as f64);
        let power = 5.7 * ratio;
        let percent = (ratio * 100.0) as u64;
        println!("Keyboard Backlight: {}/{} = {}% ~{:.2} W", brightness, max_brightness, percent, power);
        println!("  Left: {:06X}", backlight.color_left()?);
        println!("  Center: {:06X}", backlight.color_center()?);
        println!("  Right: {:06X}", backlight.color_right()?);
        println!("  Extra: {:06X}", backlight.color_extra()?);
    }

    Ok(())
}

fn main() {
    power().unwrap();
}
