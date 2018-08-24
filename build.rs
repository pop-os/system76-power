extern crate vergen;

use vergen::*;

pub fn main() {
    vergen(OutputFns::all()).unwrap();
}
