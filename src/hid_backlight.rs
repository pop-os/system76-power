use hidapi::{HidApi, HidDevice, HidResult};
use inotify::{Inotify, WatchMask};
use std::{
    fs,
    os::unix::io::AsRawFd,
    path::Path,
};

fn keyboard(device: &HidDevice, brightness: u8, color: u32) -> HidResult<()> {
    //TODO: reset
    let raw_brightness = (((brightness as u16) * 10 + 254) / 255) as u8;
    debug!(
        "keyboard brightness {}/10 color #{:06X}",
        raw_brightness,
        color
    );

    // Set all LED colors
    for led in 0..=255 {
        device.send_feature_report(&[
            0xCC, 0x01,
            led,
            (color >> 16) as u8,
            (color >> 8) as u8,
            color as u8,
        ])?;
    }

    // Set brightness
    device.send_feature_report(&[
        0xCC, 0x09,
        raw_brightness,
    ])?;

    // Override boot effect
    device.send_feature_report(&[
        0xCC, 0x20, 0x01
    ])?;

    Ok(())
}

fn lightguide(device: &HidDevice, brightness: u8, color: u32) -> HidResult<()> {
    //TODO: reset
    let raw_brightness = (((brightness as u16) * 4 + 254) / 255) as u8;
    debug!(
        "lightguide brightness {}/4 color #{:06X}",
        raw_brightness,
        color
    );

    // Set all LED colors
    device.send_feature_report(&[
        0xCC, 0xB0, 0x00, 0x00,
        (color >> 16) as u8,
        (color >> 8) as u8,
        color as u8,
    ])?;

    // Set brightness
    device.send_feature_report(&[
        0xCC, 0xBF,
        raw_brightness,
    ])?;

    Ok(())
}

//TODO: better error handling
pub fn daemon() {
    let api = match HidApi::new() {
        Ok(ok) => ok,
        Err(err) => {
            error!("hid_backlight: failed to access HID API: {}", err);
            return;
        }
    };

    let dir = Path::new("/sys/class/leds/system76_acpi::kbd_backlight");
    if ! dir.is_dir() {
        error!("hid_backlight: no system76_acpi::kbd_backlight led");
        return;
    }

    //TODO: check for existence of files
    let brightness_file = dir.join("brightness");
    let brightness_hw_changed_file = dir.join("brightness_hw_changed");
    let color_file = dir.join("color");

    let mut inotify = Inotify::init().unwrap();
    inotify.add_watch(&brightness_file, WatchMask::MODIFY).unwrap();
    inotify.add_watch(&brightness_hw_changed_file, WatchMask::MODIFY).unwrap();
    inotify.add_watch(&color_file, WatchMask::MODIFY).unwrap();

    let mut buffer = [0; 1024];
    loop {
        let brightness_string = fs::read_to_string(&brightness_file).unwrap();
        let brightness = u8::from_str_radix(brightness_string.trim(), 10).unwrap();

        let color_string = fs::read_to_string(&color_file).unwrap();
        let color = u32::from_str_radix(color_string.trim(), 16).unwrap();

        let mut devices = 0;

        for info in api.device_list() {
            let f = match (info.vendor_id(), info.product_id()) {
                (0x048d, 0x8297) => lightguide,
                (0x048d, 0x8910) => keyboard,
                _ => continue,
            };

            match info.open_device(&api) {
                Ok(device) => match f(&device, brightness, color) {
                    Ok(()) => (),
                    Err(err) => {
                        error!("hid_backlight: failed to set device: {}", err);
                    }
                },
                Err(err) => {
                    error!("hid_backlight: failed to open device: {}", err);
                }
            }

            devices += 1;
        }

        if devices == 0 {
            info!("hid_backlight: no devices found");
            break;
        }

        for event in inotify.read_events_blocking(&mut buffer).unwrap() {
            trace!("{:?}", event);
        }
    }
}
