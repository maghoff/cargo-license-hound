use std::collections::HashMap;
use std::io;
use std::path::Path;

use toml;

#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    Deserializing(toml::de::Error),
}

impl From<io::Error> for Error {
    fn from(other: io::Error) -> Error {
        Error::Io(other)
    }
}

impl From<toml::de::Error> for Error {
    fn from(other: toml::de::Error) -> Error {
        Error::Deserializing(other)
    }
}

#[derive(Debug, Deserialize)]
pub struct Package {
    pub name: String,
    pub version: String,
    pub source: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct LockFile {
    pub package: Vec<Package>,
    pub metadata: HashMap<String, String>,
}

impl LockFile {
    pub fn from_file<P: AsRef<Path>>(f: P) -> Result<LockFile, Error> {
        use std::fs::File;
        use std::io::Read;

        let mut reader = io::BufReader::new(File::open(f)?);
        let mut buf = String::new();
        reader.read_to_string(&mut buf)?;

        Ok(toml::from_str(&buf)?)
    }
}
