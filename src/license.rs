#[derive(Debug, Copy, Clone, Serialize)]
pub enum LicenseId {
    Bsd3Clause,
    Mit,
    Mpl2,
}

impl LicenseId {
    pub fn suffixes(&self) -> &'static [&'static str] {
        use LicenseId::*;
        match self {
            &Mit => &[ "-MIT" ],
            &Bsd3Clause => &[ ],
            &Mpl2 => &[ ],
        }
    }

    pub fn spdx_id(&self) -> &'static str {
        use LicenseId::*;
        match self {
            &Mit => "MIT",
            &Bsd3Clause => "BSD-3-Clause",
            &Mpl2 => "MPL-2.0",
        }
    }
}

#[derive(Debug, Serialize)]
pub enum LicenseSource {
    Crate(String),
    GitHubApi {
        url: String,
    }
}
