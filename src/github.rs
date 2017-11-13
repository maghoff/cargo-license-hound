use std::fmt;

use base64;
use cargo;
use regex::Regex;
use reqwest;
use serde_json;

use license::*;

lazy_static! {
    static ref URL_SCHEMA: Regex = Regex::new("^https://github.com/([^/]+)/([^/.]+)(.git)?/?$").unwrap();
    static ref HTTP_CLIENT: reqwest::Client = reqwest::Client::new();
}

const USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"));

#[derive(Deserialize, Debug, PartialEq, Eq)]
enum Encoding {
    #[serde(rename = "base64")]
    Base64,
}

impl Encoding {
    fn decode(&self, input: &str) -> Result<String, ()> {
        match self {
            &Encoding::Base64 =>
                base64::decode_config(input, base64::MIME).map_err(|_| ())
                    .and_then(|x| String::from_utf8(x).map_err(|_| ())),
        }
    }
}

#[derive(Deserialize)]
struct LicenseDescriptor {
    spdx_id: String,
}

#[derive(Deserialize)]
struct LicenseDocument {
    download_url: String,
    content: String,
    encoding: Encoding,
    license: LicenseDescriptor,
}

#[derive(Deserialize)]
struct GitHubError {
    message: String,
    documentation_url: Option<String>,
}

impl fmt::Display for GitHubError {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(f, "{}", self.message)?;

        if let Some(ref documentation_url) = self.documentation_url {
            write!(f, " ({})", documentation_url)?;
        };

        Ok(())
    }
}

const LICENSE_HOUND_GITHUB_USERNAME: &str = "LICENSE_HOUND_GITHUB_USERNAME";
const LICENSE_HOUND_GITHUB_PASSWORD: &str = "LICENSE_HOUND_GITHUB_PASSWORD";

fn get(url: &str) -> reqwest::RequestBuilder {
    use std::env::var;

    let mut builder = HTTP_CLIENT.get(url);

    builder.header(reqwest::header::UserAgent::new(USER_AGENT));

    if let (Ok(username), password) = (var(LICENSE_HOUND_GITHUB_USERNAME), var(LICENSE_HOUND_GITHUB_PASSWORD).ok()) {
        builder.basic_auth(username, password);
    }

    builder
}

fn try_to_print_error(resp: reqwest::Response) {
    if let Ok(err) = serde_json::from_reader::<_, GitHubError>(resp) {
        eprintln!("ERROR github: {}", err);
    }
}

fn license_file_from_license_api(owner: &str, repo: &str, package_name: &str, chosen_license: LicenseId) -> Option<(LicenseSource, String)> {
    let license_url = format!("https://api.github.com/repos/{}/{}/license", owner, repo);

    let resp = try_opt!(get(&license_url).send().ok());

    if resp.status() == reqwest::StatusCode::Forbidden {
        eprintln!("ERROR Request to {} forbidden by GitHub", license_url);
        try_to_print_error(resp);
        eprintln!("HINT Try authenticating with your GitHub user:");
        eprintln!("HINT     {}=... {}=... cargo license-hound", LICENSE_HOUND_GITHUB_USERNAME, LICENSE_HOUND_GITHUB_PASSWORD);
        return None;
    }

    if resp.status() == reqwest::StatusCode::NotFound {
        return None;
    }

    if !resp.status().is_success() {
        eprintln!("ERROR Unexpected status code from GitHub API ({}): {}", license_url, resp.status());
        try_to_print_error(resp);
        return None;
    }

    let license_description: LicenseDocument = try_opt!(serde_json::from_reader(resp).ok());

    if chosen_license.spdx_id() != license_description.license.spdx_id {
        eprintln!(
            "WARN GitHub and license-hound have identified different licenses \
            for package {:?}: {:?} and {:?}, respectively",
            package_name,
            license_description.license.spdx_id,
            chosen_license.spdx_id(),
        );
        return None;
    }

    Some((
        LicenseSource::GitHubApi {
            url: license_description.download_url,
        },
        try_opt!(license_description.encoding.decode(&license_description.content).ok()),
    ))
}

fn get_license_file(url: &str) -> Option<String> {
    let mut resp = try_opt!(get(&url).send().ok());

    if resp.status() == reqwest::StatusCode::Forbidden {
        eprintln!("ERROR Request to {} forbidden by GitHub", url);
        try_to_print_error(resp);
        eprintln!("HINT Try authenticating with your GitHub user:");
        eprintln!("HINT     {}=... {}=... cargo license-hound", LICENSE_HOUND_GITHUB_USERNAME, LICENSE_HOUND_GITHUB_PASSWORD);
        return None;
    }

    if resp.status().is_success() {
        use std::io::prelude::*;
        let mut contents = String::new();
        try_opt!(resp.read_to_string(&mut contents).ok());

        return Some(contents);
    }

    None
}

fn license_file_from_github_repo(owner: &str, repo: &str, _package_name: &str, chosen_license: LicenseId) -> Option<(LicenseSource, String)> {
    for (a, b, c) in chosen_license.guess_filenames() {
        let url = format!("https://raw.githubusercontent.com/{}/{}/master/{}{}{}", owner, repo, a, b, c);
        if let Some(license) = get_license_file(&url) {
            return Some((
                LicenseSource::GitHubRepo { url },
                license,
            ));
        }
    }

    None
}

fn license_file_from_github_core(repo_url: Option<&str>, package_name: &str, chosen_license: LicenseId) -> Option<(LicenseSource, String)> {
    let repo_url = try_opt!(repo_url);
    let re_captures = try_opt!(URL_SCHEMA.captures(repo_url));

    let owner = &re_captures[1];
    let repo = &re_captures[2];

    license_file_from_license_api(owner, repo, package_name, chosen_license)
        .or_else(|| license_file_from_github_repo(owner, repo, package_name, chosen_license))
}

pub fn license_file_from_github(package: &cargo::core::Package, chosen_license: LicenseId) -> Option<(LicenseSource, String)> {
    license_file_from_github_core(
        package.manifest().metadata().repository.as_ref().map(|x| &**x),
        package.name(),
        chosen_license,
    )
}

#[cfg(test)]
mod test {
    use super::*;

    const EXAMPLE_RESPONSE: &[u8] = br#"
{
  "name": "LICENSE",
  "path": "LICENSE",
  "sha": "401c59dcc4570b954dd6d345e76199e1f4e76266",
  "size": 1077,
  "url": "https://api.github.com/repos/benbalter/gman/contents/LICENSE?ref=master",
  "html_url": "https://github.com/benbalter/gman/blob/master/LICENSE",
  "git_url": "https://api.github.com/repos/benbalter/gman/git/blobs/401c59dcc4570b954dd6d345e76199e1f4e76266",
  "download_url": "https://raw.githubusercontent.com/benbalter/gman/master/LICENSE?lab=true",
  "type": "file",
  "content": "VGhlIE1JVCBMaWNlbnNlIChNSVQpCgpDb3B5cmlnaHQgKGMpIDIwMTMgQmVu\nIEJhbHRlcgoKUGVybWlzc2lvbiBpcyBoZXJlYnkgZ3JhbnRlZCwgZnJlZSBv\nZiBjaGFyZ2UsIHRvIGFueSBwZXJzb24gb2J0YWluaW5nIGEgY29weSBvZgp0\naGlzIHNvZnR3YXJlIGFuZCBhc3NvY2lhdGVkIGRvY3VtZW50YXRpb24gZmls\nZXMgKHRoZSAiU29mdHdhcmUiKSwgdG8gZGVhbCBpbgp0aGUgU29mdHdhcmUg\nd2l0aG91dCByZXN0cmljdGlvbiwgaW5jbHVkaW5nIHdpdGhvdXQgbGltaXRh\ndGlvbiB0aGUgcmlnaHRzIHRvCnVzZSwgY29weSwgbW9kaWZ5LCBtZXJnZSwg\ncHVibGlzaCwgZGlzdHJpYnV0ZSwgc3VibGljZW5zZSwgYW5kL29yIHNlbGwg\nY29waWVzIG9mCnRoZSBTb2Z0d2FyZSwgYW5kIHRvIHBlcm1pdCBwZXJzb25z\nIHRvIHdob20gdGhlIFNvZnR3YXJlIGlzIGZ1cm5pc2hlZCB0byBkbyBzbywK\nc3ViamVjdCB0byB0aGUgZm9sbG93aW5nIGNvbmRpdGlvbnM6CgpUaGUgYWJv\ndmUgY29weXJpZ2h0IG5vdGljZSBhbmQgdGhpcyBwZXJtaXNzaW9uIG5vdGlj\nZSBzaGFsbCBiZSBpbmNsdWRlZCBpbiBhbGwKY29waWVzIG9yIHN1YnN0YW50\naWFsIHBvcnRpb25zIG9mIHRoZSBTb2Z0d2FyZS4KClRIRSBTT0ZUV0FSRSBJ\nUyBQUk9WSURFRCAiQVMgSVMiLCBXSVRIT1VUIFdBUlJBTlRZIE9GIEFOWSBL\nSU5ELCBFWFBSRVNTIE9SCklNUExJRUQsIElOQ0xVRElORyBCVVQgTk9UIExJ\nTUlURUQgVE8gVEhFIFdBUlJBTlRJRVMgT0YgTUVSQ0hBTlRBQklMSVRZLCBG\nSVRORVNTCkZPUiBBIFBBUlRJQ1VMQVIgUFVSUE9TRSBBTkQgTk9OSU5GUklO\nR0VNRU5ULiBJTiBOTyBFVkVOVCBTSEFMTCBUSEUgQVVUSE9SUyBPUgpDT1BZ\nUklHSFQgSE9MREVSUyBCRSBMSUFCTEUgRk9SIEFOWSBDTEFJTSwgREFNQUdF\nUyBPUiBPVEhFUiBMSUFCSUxJVFksIFdIRVRIRVIKSU4gQU4gQUNUSU9OIE9G\nIENPTlRSQUNULCBUT1JUIE9SIE9USEVSV0lTRSwgQVJJU0lORyBGUk9NLCBP\nVVQgT0YgT1IgSU4KQ09OTkVDVElPTiBXSVRIIFRIRSBTT0ZUV0FSRSBPUiBU\nSEUgVVNFIE9SIE9USEVSIERFQUxJTkdTIElOIFRIRSBTT0ZUV0FSRS4K\n",
  "encoding": "base64",
  "_links": {
    "self": "https://api.github.com/repos/benbalter/gman/contents/LICENSE?ref=master",
    "git": "https://api.github.com/repos/benbalter/gman/git/blobs/401c59dcc4570b954dd6d345e76199e1f4e76266",
    "html": "https://github.com/benbalter/gman/blob/master/LICENSE"
  },
  "license": {
    "key": "mit",
    "name": "MIT License",
    "spdx_id": "MIT",
    "url": "https://api.github.com/licenses/mit",
    "featured": true
  }
}
"#;

    const BASE64_MIT: &str = "VGhlIE1JVCBMaWNlbnNlIChNSVQpCgpDb3B5cmlnaHQgKGMpIDIwMTMgQmVu\nIEJhbHRlcgoKUGVybWlzc2lvbiBpcyBoZXJlYnkgZ3JhbnRlZCwgZnJlZSBv\nZiBjaGFyZ2UsIHRvIGFueSBwZXJzb24gb2J0YWluaW5nIGEgY29weSBvZgp0\naGlzIHNvZnR3YXJlIGFuZCBhc3NvY2lhdGVkIGRvY3VtZW50YXRpb24gZmls\nZXMgKHRoZSAiU29mdHdhcmUiKSwgdG8gZGVhbCBpbgp0aGUgU29mdHdhcmUg\nd2l0aG91dCByZXN0cmljdGlvbiwgaW5jbHVkaW5nIHdpdGhvdXQgbGltaXRh\ndGlvbiB0aGUgcmlnaHRzIHRvCnVzZSwgY29weSwgbW9kaWZ5LCBtZXJnZSwg\ncHVibGlzaCwgZGlzdHJpYnV0ZSwgc3VibGljZW5zZSwgYW5kL29yIHNlbGwg\nY29waWVzIG9mCnRoZSBTb2Z0d2FyZSwgYW5kIHRvIHBlcm1pdCBwZXJzb25z\nIHRvIHdob20gdGhlIFNvZnR3YXJlIGlzIGZ1cm5pc2hlZCB0byBkbyBzbywK\nc3ViamVjdCB0byB0aGUgZm9sbG93aW5nIGNvbmRpdGlvbnM6CgpUaGUgYWJv\ndmUgY29weXJpZ2h0IG5vdGljZSBhbmQgdGhpcyBwZXJtaXNzaW9uIG5vdGlj\nZSBzaGFsbCBiZSBpbmNsdWRlZCBpbiBhbGwKY29waWVzIG9yIHN1YnN0YW50\naWFsIHBvcnRpb25zIG9mIHRoZSBTb2Z0d2FyZS4KClRIRSBTT0ZUV0FSRSBJ\nUyBQUk9WSURFRCAiQVMgSVMiLCBXSVRIT1VUIFdBUlJBTlRZIE9GIEFOWSBL\nSU5ELCBFWFBSRVNTIE9SCklNUExJRUQsIElOQ0xVRElORyBCVVQgTk9UIExJ\nTUlURUQgVE8gVEhFIFdBUlJBTlRJRVMgT0YgTUVSQ0hBTlRBQklMSVRZLCBG\nSVRORVNTCkZPUiBBIFBBUlRJQ1VMQVIgUFVSUE9TRSBBTkQgTk9OSU5GUklO\nR0VNRU5ULiBJTiBOTyBFVkVOVCBTSEFMTCBUSEUgQVVUSE9SUyBPUgpDT1BZ\nUklHSFQgSE9MREVSUyBCRSBMSUFCTEUgRk9SIEFOWSBDTEFJTSwgREFNQUdF\nUyBPUiBPVEhFUiBMSUFCSUxJVFksIFdIRVRIRVIKSU4gQU4gQUNUSU9OIE9G\nIENPTlRSQUNULCBUT1JUIE9SIE9USEVSV0lTRSwgQVJJU0lORyBGUk9NLCBP\nVVQgT0YgT1IgSU4KQ09OTkVDVElPTiBXSVRIIFRIRSBTT0ZUV0FSRSBPUiBU\nSEUgVVNFIE9SIE9USEVSIERFQUxJTkdTIElOIFRIRSBTT0ZUV0FSRS4K\n";

    const RAW_MIT: &str = "The MIT License (MIT)\n\nCopyright (c) 2013 Ben Balter\n\nPermission is hereby granted, free of charge, to any person obtaining a copy of\nthis software and associated documentation files (the \"Software\"), to deal in\nthe Software without restriction, including without limitation the rights to\nuse, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of\nthe Software, and to permit persons to whom the Software is furnished to do so,\nsubject to the following conditions:\n\nThe above copyright notice and this permission notice shall be included in all\ncopies or substantial portions of the Software.\n\nTHE SOFTWARE IS PROVIDED \"AS IS\", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR\nIMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS\nFOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR\nCOPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER\nIN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN\nCONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.\n";

    #[test]
    fn can_deserialize_response_example() {
        let x: LicenseDocument = serde_json::from_slice(EXAMPLE_RESPONSE).unwrap();
        assert_eq!("https://raw.githubusercontent.com/benbalter/gman/master/LICENSE?lab=true", x.download_url);
        assert_eq!(BASE64_MIT, x.content);
        assert_eq!(Encoding::Base64, x.encoding);
        assert_eq!("MIT", x.license.spdx_id);
    }

    #[test]
    fn can_decode_base64() {
        assert_eq!(Ok(RAW_MIT), Encoding::Base64.decode(BASE64_MIT).as_ref().map(|x| &**x));
    }

    #[test]
    #[ignore] // Integration test, talks with github over the Internet (Use `cargo test --ignored`)
    fn test_with_live_api() {
        let report = license_file_from_license_api(
            "maghoff",
            "cargo-license-hound",
            "cargo-license-hound",
            LicenseId::Mit,
        );

        println!("{:#?}", report);

        assert!(report.is_some());
    }

    #[test]
    #[ignore] // Integration test, talks with github over the Internet (Use `cargo test --ignored`)
    fn test_with_live_repo() {
        let report = license_file_from_github_repo(
            "alexcrichton",
            "futures-rs",
            "futures-cpupool",
            LicenseId::Mit,
        );

        println!("{:#?}", report);

        assert!(report.is_some());
    }
}
