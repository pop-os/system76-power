extern crate vergen;

use vergen::*;

pub fn main() { generate_version_rs(ConstantsFlags::all()).unwrap(); }
