//! Workarounds for games that refuse to run under Wine, applied at install.

use std::fs::{self, File};
use std::io::Read;
use std::path::Path;

use anyhow::{Context, Result};

use crate::library::Game;
use crate::{Paths, Reporter, download};

/// KirikiriTools' version.dll (MIT), pinned.
const KRKR_TOOLS_URL: &str =
    "https://github.com/arcusmaximus/KirikiriTools/releases/download/1.7/version.dll";
const KRKR_TOOLS_SHA256: &str = "52745037818beb55ef3c3006342658080b652f81fc652020780904ed2cf358d2";

/// Some KiriKiri games guard against a proxy kernel32 by checking
/// `GetModuleFileName(kernel32) == GetSystemWow64Directory() + "\kernel32.dll"`.
/// Wine (and 64-bit Windows) report kernel32 under system32, so the check
/// fails on an untouched game and it quits with "unrecognized kernel32 module".
pub fn needs_kernel32_fix(files: &Path) -> bool {
    let needle: Vec<u8> = "unrecognized kernel32 module"
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect();
    let candidates = fs::read_dir(files)
        .into_iter()
        .chain(fs::read_dir(files.join("plugin")))
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.extension().is_some_and(|x| {
                let x = x.to_string_lossy().to_lowercase();
                x == "exe" || x == "dll" || x == "tpm"
            })
        });
    candidates.into_iter().any(|p| {
        let mut buf = Vec::new();
        File::open(&p)
            .and_then(|f| f.take(32 << 20).read_to_end(&mut buf))
            .is_ok()
            && memchr::memmem::find(&buf, &needle).is_some()
    })
}

/// KirikiriTools' version.dll hides `GetSystemWow64DirectoryA` from the
/// game's plugins, so the check falls back to the matching system dir. It
/// goes next to the game's exe; no game file changes.
pub fn kernel32(
    paths: &Paths,
    game: &mut Game,
    exe_dir: &Path,
    reporter: &dyn Reporter,
) -> Result<()> {
    let cached = paths.downloads().join("kirikiritools-1.7-version.dll");
    if !cached.exists() {
        download::fetch(KRKR_TOOLS_URL, &cached, reporter)?;
    }
    if let Err(e) = crate::update::verify_sha256(&cached, KRKR_TOOLS_SHA256) {
        let _ = fs::remove_file(&cached);
        return Err(e);
    }
    fs::copy(&cached, exe_dir.join("version.dll"))
        .with_context(|| format!("copying version.dll into {}", exe_dir.display()))?;
    let overrides = game.env.entry("WINEDLLOVERRIDES".into()).or_default();
    if !overrides.split(';').any(|o| o.starts_with("version=")) {
        if !overrides.is_empty() {
            overrides.push(';');
        }
        overrides.push_str("version=n,b");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spots_the_kernel32_check() {
        let dir = std::env::temp_dir().join(format!("omavn-fix-{}", std::process::id()));
        fs::create_dir_all(dir.join("plugin")).unwrap();
        fs::write(dir.join("game.exe"), b"MZ").unwrap();
        assert!(!needs_kernel32_fix(&dir));
        let mut tpm = b"MZ ... ".to_vec();
        tpm.extend(
            "Authenticatoin failed: unrecognized kernel32 module"
                .encode_utf16()
                .flat_map(u16::to_le_bytes),
        );
        fs::write(dir.join("plugin/game.tpm"), tpm).unwrap();
        assert!(needs_kernel32_fix(&dir));
        fs::remove_dir_all(dir).unwrap();
    }
}
