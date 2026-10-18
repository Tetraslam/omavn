use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::Path;

use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha512};

use crate::report::Reporter;

const USER_AGENT: &str = concat!(
    "omavn/",
    env!("CARGO_PKG_VERSION"),
    " (+https://github.com/Tetraslam/omavn)"
);

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .user_agent(USER_AGENT)
        .build()
        .into()
}

pub fn get_string(url: &str) -> Result<String> {
    Ok(agent()
        .get(url)
        .call()
        .with_context(|| format!("GET {url}"))?
        .body_mut()
        .read_to_string()?)
}

/// Download `url` to `dest` atomically (via a `.part` file), reporting bytes.
pub fn fetch(url: &str, dest: &Path, reporter: &dyn Reporter) -> Result<()> {
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut resp = agent()
        .get(url)
        .call()
        .with_context(|| format!("GET {url}"))?;
    let total = resp.body().content_length().unwrap_or(0);
    let part = dest.with_extension("part");
    let mut out = File::create(&part)?;
    let mut body = resp.body_mut().with_config().limit(u64::MAX).reader();
    let mut buf = vec![0u8; 1 << 20];
    let mut done = 0u64;
    reporter.progress(0, total);
    loop {
        let n = body.read(&mut buf)?;
        if n == 0 {
            break;
        }
        out.write_all(&buf[..n])?;
        done += n as u64;
        reporter.progress(done, total);
    }
    out.sync_all()?;
    fs::rename(&part, dest)?;
    Ok(())
}

pub fn verify_sha512(file: &Path, expected_hex: &str) -> Result<()> {
    let mut hasher = Sha512::new();
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
    if !got.eq_ignore_ascii_case(expected_hex.trim()) {
        bail!("checksum mismatch for {}", file.display());
    }
    Ok(())
}

#[derive(Debug, serde::Deserialize)]
pub struct Release {
    pub tag_name: String,
    pub assets: Vec<Asset>,
}

#[derive(Debug, serde::Deserialize)]
pub struct Asset {
    pub name: String,
    pub browser_download_url: String,
}

pub fn github_latest(repo: &str) -> Result<Release> {
    let body = get_string(&format!(
        "https://api.github.com/repos/{repo}/releases/latest"
    ))?;
    Ok(serde_json::from_str(&body)?)
}
