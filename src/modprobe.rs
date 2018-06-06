use std::io;
use std::process::Command;

pub fn reload(module: &str, options: &[&str]) -> io::Result<()> {
    unload(module).and_then(|_| load(module, options))
}

pub fn unload(module: &str) -> io::Result<()> {
    Command::new("modprobe")
        .args(&["-r", module])
        .status()
        .and_then(|stat| if stat.success() {
            Ok(())
        } else {
            Err(io::Error::new(io::ErrorKind::Other, format!("failed to unload {}", module)))
        })
}

pub fn load(module: &str, options: &[&str]) -> io::Result<()> {
    Command::new("modprobe")
        .arg(module)
        .args(options)
        .status()
        .and_then(|stat| if stat.success() {
            Ok(())
        } else {
            Err(io::Error::new(io::ErrorKind::Other, format!("failed to load {}", module)))
        })
}
