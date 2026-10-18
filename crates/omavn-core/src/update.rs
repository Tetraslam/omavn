//! Self-update from GitHub releases.

use std::fs::{self, File};
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha256};

use crate::download;
use crate::paths::Paths;
use crate::report::Reporter;

pub const REPO: &str = "Tetraslam/omavn";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone, serde::Serialize)]
pub struct Available {
    pub current: String,
    pub latest: String,
    pub newer: bool,
    pub url: String,
}

/// Release asset name for this machine, e.g. `omavn-x86_64-unknown-linux-gnu.tar.gz`.
pub fn asset_name() -> String {
    format!("omavn-{}-unknown-linux-gnu.tar.gz", std::env::consts::ARCH)
}

pub fn check() -> Result<Available> {
    let tag = download::github_latest_tag(REPO)?;
    let latest = tag.trim_start_matches('v').to_string();
    let url = format!(
        "https://github.com/{REPO}/releases/download/{tag}/{}",
        asset_name()
    );
    Ok(Available {
        newer: is_newer(&latest, VERSION),
        current: VERSION.into(),
        latest,
        url,
    })
}

/// Replace the running binary with the latest release. Returns the new
/// version, or `None` if already up to date.
pub fn update(paths: &Paths, force: bool, reporter: &dyn Reporter) -> Result<Option<String>> {
    let exe = std::env::current_exe()?.canonicalize()?;
    let dir = exe.parent().context("binary has no parent directory")?;
    // /usr belongs to the package manager even when we could write there
    // (as root); replacing its files would corrupt the package database.
    if exe.starts_with("/usr") {
        bail!(
            "omavn was installed by your package manager ({}); update it there, e.g. yay -Syu",
            exe.display()
        );
    }
    if fs::metadata(dir)?.permissions().readonly() || !writable(dir) {
        bail!(
            "{} is not writable; omavn was probably installed by a package manager, update it there",
            dir.display()
        );
    }
    let available = check()?;
    if !available.newer && !force {
        return Ok(None);
    }

    reporter.step(format!("downloading omavn {}", available.latest));
    let tarball = paths.downloads().join(asset_name());
    download::fetch(&available.url, &tarball, reporter)?;
    let expected = download::get_string(&format!("{}.sha256", available.url))?;
    verify_sha256(&tarball, expected.split_whitespace().next().unwrap_or(""))?;

    let staged = dir.join(".omavn.new");
    extract_binary(&tarball, &staged)?;
    fs::rename(&staged, &exe).context("replacing the omavn binary")?;
    let _ = fs::remove_file(&tarball);
    Ok(Some(available.latest))
}

fn extract_binary(tarball: &Path, dest: &Path) -> Result<()> {
    let gz = flate2::read::GzDecoder::new(BufReader::new(File::open(tarball)?));
    let mut archive = tar::Archive::new(gz);
    for entry in archive.entries()? {
        let mut entry = entry?;
        if entry.path()?.file_name().is_some_and(|n| n == "omavn") {
            entry.unpack(dest)?;
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(dest, fs::Permissions::from_mode(0o755))?;
            return Ok(());
        }
    }
    bail!("release archive has no omavn binary")
}

fn verify_sha256(file: &Path, expected: &str) -> Result<()> {
    let mut hasher = Sha256::new();
    let mut f = File::open(file)?;
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    let got: String = hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    if !got.eq_ignore_ascii_case(expected.trim()) {
        bail!("checksum mismatch for {}", file.display());
    }
    Ok(())
}

fn writable(dir: &Path) -> bool {
    let probe: PathBuf = dir.join(".omavn-write-test");
    let ok = File::create(&probe).is_ok();
    let _ = fs::remove_file(probe);
    ok
}

fn is_newer(candidate: &str, current: &str) -> bool {
    let parse = |v: &str| -> Vec<u64> {
        v.split(['.', '-'])
            .map(|p| p.parse().unwrap_or(0))
            .collect()
    };
    parse(candidate) > parse(current)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions() {
        assert!(is_newer("0.2.0", "0.1.9"));
        assert!(is_newer("0.10.0", "0.9.0"));
        assert!(!is_newer("0.1.0", "0.1.0"));
    }
}
