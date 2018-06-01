use std::io;
use std::path::Path;
use util::write_file;

const LAPTOP_MODE: &str = "/proc/sys/vm/laptop_mode";

pub fn set_laptop_mode(secs: u32) -> io::Result<()> {
    let path = Path::new(LAPTOP_MODE);
    if !path.exists() {
        return Ok(());
    }

    write_file(LAPTOP_MODE, secs.to_string().as_bytes())
}
