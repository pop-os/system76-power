use std::{fs::read_to_string, io};

pub struct Module {
    pub name: String,
}

impl Module {
    fn parse(line: &str) -> io::Result<Module> {
        let mut parts = line.split(' ');

        let name = parts
            .next()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "module name not found"))?;

        Ok(Module { name: name.to_string() })
    }

    pub fn all() -> io::Result<Vec<Module>> {
        let mut modules = Vec::new();

        let data = read_to_string("/proc/modules")?;
        for line in data.lines() {
            let module = Module::parse(line)?;
            modules.push(module);
        }

        Ok(modules)
    }
}
