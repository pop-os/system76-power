use std::fmt::Display;
use std::fs::{DirEntry, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::Path;
use std::str::FromStr;

pub fn entries<T, F: FnMut(DirEntry) -> T>(path: &Path, mut func: F) -> io::Result<Vec<T>> {
    let mut ret = Vec::new();
    for entry_res in path.read_dir()? {
        ret.push(func(entry_res?));
    }

    Ok(ret)
}

pub fn read_file<P: AsRef<Path>>(path: P) -> io::Result<String> {
    let mut data = String::new();

    {
        let mut file = File::open(path.as_ref())?;
        file.read_to_string(&mut data)?;
    }

    Ok(data)
}

pub fn write_file<P: AsRef<Path>, S: AsRef<[u8]>>(path: P, data: S) -> io::Result<()> {
    {
        let mut file = OpenOptions::new().write(true).open(path)?;
        file.write_all(data.as_ref())?
    }

    Ok(())
}

pub fn parse_file<F: FromStr, P: AsRef<Path>>(path: P) -> io::Result<F> where F::Err: Display {
    read_file(path)?.trim().parse().map_err(|err| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{}", err)
        )
    })
}
