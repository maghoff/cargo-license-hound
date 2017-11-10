#[macro_use] extern crate serde_derive;
extern crate cargo;
extern crate itertools;
extern crate serde;
extern crate serde_json;
extern crate toml;

mod lockfile;

use std::path::PathBuf;

use cargo::core::{Source, SourceId, PackageId};
use cargo::util::Config;
use cargo::sources::SourceConfigMap;

#[derive(Debug, Copy, Clone, Serialize)]
enum LicenseId {
    Bsd3Clause,
    Mit,
    Mpl2,
}

impl LicenseId {
    fn suffixes(&self) -> &'static [&'static str] {
        use LicenseId::*;
        match self {
            &Mit => &[ "-MIT" ],
            &Bsd3Clause => &[ ],
            &Mpl2 => &[ ],
        }
    }

    fn id(&self) -> &'static str {
        use LicenseId::*;
        match self {
            &Mit => "Mit",
            &Bsd3Clause => "Bsd3Clause",
            &Mpl2 => "Mpl2",
        }
    }
}

#[derive(Debug, Serialize)]
enum LicenseSource {
    Crate(String),
    GitHubApi {
        repository: String,
        path: PathBuf,
    }
}

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

        LicenseHound { source_config_map }
    }

    fn hound_license_file(&self, package: &cargo::core::Package, chosen_license: LicenseId) -> Result<(LicenseSource, String), LicenseError> {
        let manifest_path = package.manifest_path();

        let candidate_base_names = [
            "LICENSE",
            "LICENCE", // Typo seen in the wild
        ];

        for suffix in chosen_license.suffixes().into_iter().chain(&[""]) {
            for base_name in &candidate_base_names {
                let candidate_name = format!("{}{}", base_name, suffix);
                if let Ok(license_text) = read_file(manifest_path.with_file_name(&candidate_name)) {
                    return Ok((LicenseSource::Crate(candidate_name), license_text));
                }
            }
        }

        Err(LicenseError::UnableToRecoverLicenseFile(package.manifest_path().with_file_name("").to_owned()))
    }

    fn chase(&self, package: &lockfile::Package) -> Result<LicenseDescription, LicenseError> {
        let source = package.source.as_ref().ok_or(LicenseError::NoSource)?;

        let source_id = SourceId::from_url(&source).unwrap();
        let mut source = self.source_config_map.load(&source_id).unwrap();
        source.update().unwrap();

        let package_id = PackageId::new(&package.name, &package.version, &source_id).unwrap();
        let package = source.download(&package_id).unwrap();
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
