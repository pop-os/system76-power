// Copyright 2018-2021 System76 <info@system76.com>
//
// SPDX-License-Identifier: GPL-3.0-only

use hidapi::{HidApi, HidDevice, HidResult};
use inotify::{Inotify, WatchMask};
use std::{fs, path::Path};

const USB_VID_ITE: u16 = 0x048d;

fn keyboard(device: &HidDevice, brightness: u8, color: u32) -> HidResult<()> {
    // TODO: reset
    let raw_brightness = ((u16::from(brightness) * 10 + 254) / 255) as u8;
    log::debug!("keyboard brightness {}/10 color #{:06X}", raw_brightness, color);

    // Determine color channel values
    let r = (color >> 16) as u8;
    let mut g = (color >> 8) as u8;
    let mut b = color as u8;

    // Color correction based on model
    let dmi_vendor = fs::read_to_string("/sys/class/dmi/id/sys_vendor").unwrap_or_default();
    let dmi_model = fs::read_to_string("/sys/class/dmi/id/product_version").unwrap_or_default();
    match (dmi_vendor.trim(), dmi_model.trim()) {
        ("System76", "bonw15") => {
            g = ((u16::from(g) * 0x65) / 0xFF) as u8;
            b = ((u16::from(b) * 0x60) / 0xFF) as u8;
        }
        _ => {}
    }

    // Set all LED colors
    for led in 0..=255 {
        device.send_feature_report(&[0xCC, 0x01, led, r, g, b])?;
    }

    // Set brightness
    device.send_feature_report(&[0xCC, 0x09, raw_brightness])?;

    // Override boot effect
    device.send_feature_report(&[0xCC, 0x20, 0x01])?;

    Ok(())
}

fn lightguide_8297(device: &HidDevice, brightness: u8, color: u32) -> HidResult<()> {
    // TODO: reset
    let raw_brightness = ((u16::from(brightness) * 4 + 254) / 255) as u8;
    log::debug!("lightguide brightness {}/4 color #{:06X}", raw_brightness, color);

    // Set all LED colors
    device.send_feature_report(&[
        0xCC,
        0xB0,
        0x00,
        0x00,
        (color >> 16) as u8,
        (color >> 8) as u8,
        color as u8,
    ])?;

    // Set brightness
    device.send_feature_report(&[0xCC, 0xBF, raw_brightness])?;

    Ok(())
}

fn lightguide_8911(device: &HidDevice, _brightness: u8, _color: u32) -> HidResult<()> {
    // This light guide cannot set an arbitrary color, so we will instead set it to off
    device.send_feature_report(&[0xCD, 0xE2, 0x02, 0])?;

    // Set effect to static
    device.send_feature_report(&[0xCD, 0xE2, 0x05])?;

    Ok(())
}

// TODO: better error handling
pub fn daemon() {
    let dmi_vendor = fs::read_to_string("/sys/class/dmi/id/sys_vendor").unwrap_or_default();
    let dmi_model = fs::read_to_string("/sys/class/dmi/id/product_version").unwrap_or_default();
    match (dmi_vendor.trim(), dmi_model.trim()) {
        ("System76", "bonw14" | "bonw15" | "bonw15-b" | "bonw16") => {}
        _ => return,
    }

    let api = match HidApi::new() {
        Ok(ok) => ok,
        Err(err) => {
            log::error!("hid_backlight: failed to access HID API: {}", err);
            return;
        }
    };

    let mut dir = Path::new("/sys/class/leds/system76_acpi::kbd_backlight");
    if !dir.is_dir() {
        // Check path from system76-dkms
        dir = Path::new("/sys/class/leds/system76::kbd_backlight");
    }
    if !dir.is_dir() {
        log::error!("hid_backlight: no kbd_backlight control");
        return;
    }

    let mut inotify = Inotify::init().unwrap();
    let mut watches = inotify.watches();

    let brightness_file = dir.join("brightness");
    if brightness_file.exists() {
        watches.add(&brightness_file, WatchMask::MODIFY).unwrap();
    }

    let brightness_hw_changed_file = dir.join("brightness_hw_changed");
    if brightness_hw_changed_file.exists() {
        if let Err(e) = watches.add(&brightness_hw_changed_file, WatchMask::MODIFY) {
            log::warn!("hid_backlight: failed to watch hardware changed file: {}", e);
        }
    }

    let color_file = dir.join("color");
    let color_left_file = dir.join("color_left");

    if color_file.exists() {
        if let Err(e) = watches.add(&color_file, WatchMask::MODIFY) {
            log::warn!("hid_backlight: failed to watch keyboard color: {}", e);
        }
    } else if color_left_file.exists() {
        // XXX: Watch other zones?
        watches.add(&color_left_file, WatchMask::MODIFY).unwrap();
    }

    let mut buffer = [0; 1024];
    loop {
        let brightness = if brightness_file.exists() {
            let brightness_string = fs::read_to_string(&brightness_file).unwrap();
            brightness_string.trim().parse::<u8>().unwrap()
        } else {
            0
        };

        let color_string = if color_file.exists() {
            fs::read_to_string(&color_file).unwrap()
        } else if color_left_file.exists() {
            fs::read_to_string(&color_left_file).unwrap()
        } else {
            String::from("FFFFFF")
        };
        let color = u32::from_str_radix(color_string.trim(), 16).unwrap();

        let mut devices = 0;

        for info in api.device_list() {
            let f = match (info.vendor_id(), info.product_id()) {
                (USB_VID_ITE, 0x8297) => lightguide_8297,
                (USB_VID_ITE, 0x8910) => keyboard,
                (USB_VID_ITE, 0x8911) => lightguide_8911,
                _ => continue,
            };

            match info.open_device(&api) {
                Ok(device) => match f(&device, brightness, color) {
                    Ok(()) => (),
                    Err(err) => {
                        log::error!("hid_backlight: failed to set device: {}", err);
                    }
                },
                Err(err) => {
                    log::error!("hid_backlight: failed to open device: {}", err);
                }
            }

            devices += 1;
        }

        if devices == 0 {
            log::info!("hid_backlight: no devices found");
            break;
        }

        for event in inotify.read_events_blocking(&mut buffer).unwrap() {
            log::trace!("{:?}", event);
        }
    }
}
