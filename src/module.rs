use std::{fs::read_to_string, io};

pub struct Module {
    pub name: String,
}

impl Module {
    pub fn all() -> io::Result<Vec<Module>> {
        read_to_string("/proc/modules")?.lines().map(parse).collect()
    }
}

fn parse(line: &str) -> io::Result<Module> {
    let name = line
        .split(' ')
        .next()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "module name not found"))?
        .to_string();

    Ok(Module { name })
}
