#[macro_use] extern crate itertools;
#[macro_use] extern crate lazy_static;
#[macro_use] extern crate serde_derive;
extern crate base64;
extern crate cargo;
extern crate regex;
extern crate reqwest;
extern crate serde_json;
extern crate serde;
extern crate toml;

mod github;
mod license;
mod lockfile;

use std::path::PathBuf;
use std::collections::HashSet;

use cargo::core::{Source, SourceId, PackageId};
use cargo::core::source::MaybePackage;
use cargo::util::Config;
use cargo::sources::SourceConfigMap;

use license::*;

#[derive(Debug, Serialize)]
struct LicenseDescription {
    chosen_license: LicenseId,
    copyright_notice: String,
    full_spdx_license: String,
    full_license_document: String,
    license_source: LicenseSource,
    link: Option<String>,
}

#[derive(Debug, Serialize)]
enum LicenseError {
    NoSource,
    LicenseNotDeclared(PathBuf),
    UnableToRecoverLicenseFile(PathBuf),
    UnableToRecoverAttribution(String),
    UnacceptableLicense(String),
}

#[derive(Debug, Serialize)]
struct LicenseReport {
    package_name: String,
    version: String,
    conclusion: Result<LicenseDescription, LicenseError>,
}

struct LicenseHound<'a> {
    source_config_map: SourceConfigMap<'a>,
    yanked_whitelist: HashSet<PackageId>,
}

fn read_file<P: AsRef<std::path::Path>>(path: P) -> Result<String, std::io::Error> {
    use std::fs::File;
    use std::io::prelude::*;

    let mut file = File::open(path)?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;

    Ok(contents)
}

fn recover_copyright_notice(license_text: &str) -> Result<String, LicenseError> {
    use itertools::Itertools;

    Ok(
        license_text
            .lines()
            .map(|x| if x.starts_with("//") { &x[2..] } else { x })
            .map(|x| x.trim())
            .map(|x| x.to_string())
            .coalesce(|a, b| {
                if b.len() == 0 {
                    Err((a, b))
                } else {
                    if a.len() == 0 {
                        Ok(b)
                    } else {
                        Ok(format!("{} {}", a, b))
                    }
                }
            })
            .filter(|x| x.to_lowercase().find("copyright").is_some())
            .next()
            .ok_or_else(|| LicenseError::UnableToRecoverAttribution(license_text.to_string()))?
    )
}

impl<'a> LicenseHound<'a> {
    fn new(config: &'a Config) -> LicenseHound<'a> {
        let source_config_map = SourceConfigMap::new(&config).unwrap();
        let yanked_whitelist = HashSet::new();

        LicenseHound { source_config_map, yanked_whitelist }
    }

    fn license_file_from_package(&self, package: &cargo::core::Package, chosen_license: LicenseId) -> Option<(LicenseSource, String)> {
        let manifest_path = package.manifest_path();

        for (a, b, c) in chosen_license.guess_filenames() {
            let candidate_name = format!("{}{}{}", a, b, c);

            if let Ok(license_text) = read_file(manifest_path.with_file_name(&candidate_name)) {
                return Some((LicenseSource::Crate(candidate_name), license_text));
            }
        }

        None
    }

    fn hound_license_file(&self, package: &cargo::core::Package, chosen_license: LicenseId) -> Result<(LicenseSource, String), LicenseError> {
        self.license_file_from_package(package, chosen_license)
            .or_else(|| github::license_file_from_github(package, chosen_license))
            .ok_or_else(|| LicenseError::UnableToRecoverLicenseFile(package.manifest_path().with_file_name("").to_owned()))
    }

    fn chase(&self, package: &lockfile::Package) -> Result<LicenseDescription, LicenseError> {
        let source = package.source.as_ref().ok_or(LicenseError::NoSource)?;

        let source_id = SourceId::from_url(&source).unwrap();
        let mut source = self.source_config_map.load(source_id, &self.yanked_whitelist).unwrap();
        source.update().unwrap();

        let package_id = PackageId::new(&package.name, &package.version, source_id).unwrap();
        let package = match source.download(package_id).unwrap() {
            MaybePackage::Ready(package) => package,
            MaybePackage::Download { .. } => unreachable!(),
        };
        let metadata = package.manifest().metadata();

        let spdx_license = metadata.license.as_ref().ok_or(LicenseError::LicenseNotDeclared(package.manifest_path().to_owned()))?;

        // YOLO! This will give legally wrong results for descriptors such as "MIT AND GPL3",
        // which I have never seen in the wild. The more robust solution here is to implement
        // a proper parser for the spdx syntax and implement boolean logic for it.
        let chosen_license =
            if spdx_license.find("MIT").is_some() {
                Ok(LicenseId::Mit)
            } else if spdx_license.find("MPL-2.0").is_some() {
                Ok(LicenseId::Mpl2)
            } else if spdx_license.find("BSD-3-Clause").is_some() {
                Ok(LicenseId::Bsd3Clause)
            } else {
                Err(LicenseError::UnacceptableLicense(spdx_license.clone()))
            }?;

        let (license_source, full_license_document) = self.hound_license_file(&package, chosen_license)?;

        let copyright_notice = recover_copyright_notice(&full_license_document)?;

        Ok(LicenseDescription {
            chosen_license: chosen_license,
            copyright_notice: copyright_notice,
            full_spdx_license: spdx_license.clone(),
            full_license_document: full_license_document,
            license_source: license_source,
            link:
                metadata.homepage.as_ref()
                .or(metadata.repository.as_ref())
                .or(metadata.documentation.as_ref())
                .map(|x| x.to_string()),
        })
    }
}

fn main() {
    let config = Config::default().unwrap();
    let _lock = config.acquire_package_cache_lock().unwrap();
    let license_hound = LicenseHound::new(&config);

    let packages = lockfile::LockFile::from_file("Cargo.lock").unwrap().package;

    let license_reports =
        packages.into_iter().map(|x| {
            let conclusion = license_hound.chase(&x);
            LicenseReport { package_name: x.name, version: x.version, conclusion }
        })
        .collect::<Vec<_>>();

    serde_json::to_writer(std::io::stdout(), &license_reports).unwrap();
}
