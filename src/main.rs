use clap::Clap;
use std::process;

mod cli;

fn main() {
    let app = cli::Command::parse();

    match app.run() {
        Ok(()) => (),
        Err(err) => {
            eprintln!("{}", err);
            process::exit(1);
        }
    }
}
