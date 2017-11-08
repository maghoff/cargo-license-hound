#[macro_use] extern crate serde_derive;
extern crate serde;
extern crate toml;

mod lockfile;

fn main() {
    println!("{:#?}", lockfile::LockFile::from_file("Cargo.lock").unwrap());
}
