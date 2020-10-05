use dbus::arg::RefArg;
use dbus::blocking::SyncConnection;
use dbus::channel::Channel;
use dbus::message::SignalArgs;
use dbus::strings::Path as DbusPath;
use dbus::tree::{self, Access, MTSync, MethodErr};
use inotify::{Inotify, WatchDescriptor, WatchMask};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::{DBUS_KEYBOARD_IFACE, DBUS_PATH};

type Factory = dbus::tree::Factory<MTSync<()>, ()>;
type Interface = dbus::tree::Interface<MTSync<()>, ()>;
type Property = dbus::tree::Property<MTSync<()>, ()>;
type Tree = dbus::tree::Tree<MTSync<()>, ()>;

/// Get maximum brightness from a sysfs directory path
fn get_max_brightness(path: &Path) -> Result<i32, MethodErr> {
    let mut path = PathBuf::from(path);
    path.push("max_brightness");
    let brightness = fs::read_to_string(&path).map_err(|e| MethodErr::failed(&e))?;
    i32::from_str_radix(brightness.trim_end(), 10).map_err(|e| MethodErr::failed(&e))
}

/// Get brightness from a sysfs directory path
fn get_brightness(path: &Path) -> Result<i32, MethodErr> {
    let mut path = PathBuf::from(path);
    path.push("brightness");
    let brightness = fs::read_to_string(&path).map_err(|e| MethodErr::failed(&e))?;
    i32::from_str_radix(brightness.trim_end(), 10).map_err(|e| MethodErr::failed(&e))
}

/// Sets brightness with a sysfs directory path
fn set_brightness(path: &Path, brightness: i32) -> Result<(), MethodErr> {
    let mut path = PathBuf::from(path);
    path.push("brightness");
    fs::write(&path, format!("{}\n", brightness)).map_err(|e| MethodErr::failed(&e))
}

/// Gets color from a sysfs directory path
/// Returns "" if it does not support color
fn get_color(path: &Path) -> Result<String, MethodErr> {
    let mut path = PathBuf::from(path);
    path.push("color");
    if !path.exists() {
        path.pop();
        path.push("color_left");
        if !path.exists() {
            return Ok("".to_string());
        }
    }
    let color = fs::read_to_string(&path).map_err(|e| MethodErr::failed(&e))?;
    Ok(color.trim_end().to_string())
}

/// Sets color with a sysfs directory path
fn set_color(path: &Path, color: &str) -> Result<(), MethodErr> {
    let entries = fs::read_dir(path).map_err(|e| MethodErr::failed(&e))?;
    for i in entries {
        let i = i.map_err(|e| MethodErr::failed(&e))?;
        if let Some(filename) = i.file_name().to_str() {
            if filename.starts_with("color") {
                fs::write(i.path(), color).map_err(|e| MethodErr::failed(&e))?;
            }
        }
    }
    Ok(())
}

struct Keyboard {
    brightness_prop: Arc<Property>,
    color_prop: Arc<Property>,
    interface: Arc<Interface>,
    dbus_path: DbusPath<'static>,
    path: PathBuf,
}

impl Keyboard {
    fn new(f: &Factory, path: &Path, dbus_path: DbusPath<'static>) -> Self {
        let path = path.to_owned();
        let path0 = path.clone();
        let path1 = path.clone();
        let path2 = path.clone();
        let path3 = path.clone();
        let path4 = path.clone();
        let max_brightness_prop =
            Arc::new(f.property::<i32, _>("max-brightness", ()).auto_emit_on_set(false).on_get(
                move |iter, _| {
                    iter.append(get_max_brightness(&path0)?);
                    Ok(())
                },
            ));
        let brightness_prop = Arc::new(
            f.property::<i32, _>("brightness", ())
                .auto_emit_on_set(false)
                .access(Access::ReadWrite)
                .on_get(move |iter, _| {
                    iter.append(get_brightness(&path1)?);
                    Ok(())
                })
                .on_set(move |iter, _| set_brightness(&path2, iter.read()?)),
        );
        let color_prop = Arc::new(
            f.property::<&str, _>("color", ())
                .auto_emit_on_set(false)
                .access(Access::ReadWrite)
                .on_get(move |iter, _| {
                    iter.append(get_color(&path3)?);
                    Ok(())
                })
                .on_set(move |iter, _| set_color(&path4, iter.read()?)),
        );
        let name_prop = Arc::new(f.property::<&str, _>("name", ()).on_get(|iter, _| {
            // TODO: Update for Launch keyboard
            iter.append("Built-in Keyboard");
            Ok(())
        }));
        let interface = Arc::new(
            f.interface(DBUS_KEYBOARD_IFACE, ())
                .add_p(max_brightness_prop.clone())
                .add_p(brightness_prop.clone())
                .add_p(color_prop.clone())
                .add_p(name_prop.clone()),
        );
        Self { brightness_prop, color_prop, interface, dbus_path, path }
    }

    fn notify_prop<T: RefArg + 'static>(&self, c: &Channel, p: &Property, value: T) {
        let mut v = Vec::new();
        p.add_propertieschanged(&mut v, &DBUS_KEYBOARD_IFACE.into(), || Box::new(value));
        for i in v {
            let _ = c.send(i.to_emit_message(&self.dbus_path));
        }
    }

    fn notify_color(&self, c: &Channel) {
        match get_color(&self.path) {
            Ok(value) => self.notify_prop(c, &self.color_prop, value),
            Err(err) => error!("{:?}", err),
        }
    }

    fn notify_brightness(&self, c: &Channel) {
        match get_brightness(&self.path) {
            Ok(value) => self.notify_prop(c, &self.brightness_prop, value),
            Err(err) => error!("{:?}", err),
        }
    }
}

struct Daemon {
    c: Arc<SyncConnection>,
    tree: Arc<Mutex<Tree>>,
    keyboards: HashMap<DbusPath<'static>, Keyboard>,
    number: u64,
    inotify: Inotify,
    watches: HashMap<WatchDescriptor, (DbusPath<'static>, &'static str)>,
}

impl Daemon {
    fn new(c: Arc<SyncConnection>, tree: Arc<Mutex<Tree>>) -> Self {
        let keyboards = HashMap::new();
        let inotify = Inotify::init().unwrap();
        let number = 0;
        let watches = HashMap::new();

        Self { c, tree, keyboards, number, inotify, watches }
    }

    fn add_inotify_watches(&mut self, path: &Path, dbus_path: &DbusPath<'static>) {
        let mut brightness_path = path.to_owned();
        brightness_path.push("brightness");
        match self.inotify.add_watch(&brightness_path, WatchMask::MODIFY) {
            Ok(wd) => {
                self.watches.insert(wd, (dbus_path.clone(), "brightness"));
            }
            Err(err) => error!("{}", err),
        }

        let mut color_path = path.to_owned();
        color_path.push("color");
        if !color_path.exists() {
            color_path.pop();
            color_path.push("color_left");
        };
        if color_path.exists() {
            match self.inotify.add_watch(&color_path, WatchMask::MODIFY) {
                Ok(wd) => {
                    self.watches.insert(wd, (dbus_path.clone(), "color"));
                }
                Err(err) => error!("{}", err),
            }
        }

        let mut brightness_hw_path = path.to_owned();
        brightness_hw_path.push("brightness_hw_changed");
        match self.inotify.add_watch(&brightness_hw_path, WatchMask::MODIFY) {
            Ok(wd) => {
                self.watches.insert(wd, (dbus_path.clone(), ""));
            }
            Err(err) => error!("{}", err),
        }
    }

    fn load(&mut self) {
        let f = tree::Factory::new_sync::<()>();

        let entries = match fs::read_dir("/sys/class/leds") {
            Ok(entries) => entries,
            Err(err) => {
                error!("{}", err);
                return;
            }
        };

        for i in entries {
            let i = match i {
                Ok(i) => i,
                Err(err) => {
                    error!("{}", err);
                    continue;
                }
            };

            if let Some(filename) = i.file_name().to_str() {
                if filename.ends_with(":kbd_backlight") {
                    let path = i.path();

                    let dbus_path =
                        DbusPath::from(format!("{}/keyboard{}", DBUS_PATH, self.number));
                    self.number += 1;

                    self.add_inotify_watches(&path, &dbus_path);

                    let keyboard = Keyboard::new(&f, &path, dbus_path.clone());
                    let interface = keyboard.interface.clone();
                    self.keyboards.insert(dbus_path.clone(), keyboard);

                    info!("Adding dbus path {} with interface {}", dbus_path, DBUS_KEYBOARD_IFACE);
                    let mut tree = self.tree.lock().unwrap();
                    tree.insert(f.object_path(dbus_path, ()).introspectable().add(interface));
                }
            }
        }
    }

    fn run(&mut self) {
        // TODO: Watch for added/removed devices

        self.load();

        let mut buffer = [0; 1024];

        loop {
            for event in self.inotify.read_events_blocking(&mut buffer).unwrap() {
                trace!("{:?}", event);

                if let Some((dbus_path, property)) = self.watches.get(&event.wd) {
                    if let Some(keyboard) = self.keyboards.get(dbus_path) {
                        if property == &"brightness" {
                            keyboard.notify_brightness(self.c.channel());
                        } else if property == &"color" {
                            keyboard.notify_color(self.c.channel());
                        } else if property == &"" {
                            keyboard.notify_brightness(self.c.channel());
                            keyboard.notify_color(self.c.channel());
                        }
                    }
                }
            }
        }
    }
}

pub fn daemon(c: Arc<SyncConnection>, tree: Arc<Mutex<Tree>>) {
    Daemon::new(c, tree).run();
}