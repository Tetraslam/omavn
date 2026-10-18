//! Look at a directory and work out what it is: a playable game (and with
//! which engine), or an installer that has to run first.

use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{Result, bail};
use walkdir::WalkDir;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum InstallerKind {
    Nsis,
    Inno,
    InstallShield,
    Msi,
    WixBurn,
    SetupFactory,
    /// Named like an installer but with no recognizable framework.
    Unknown,
}

impl InstallerKind {
    /// Arguments for an unattended install into `target` (a Windows path).
    pub fn silent_args(self, target: &str) -> Option<Vec<String>> {
        match self {
            // /D must be last and unquoted, even with spaces.
            Self::Nsis => Some(vec!["/S".into(), format!("/D={target}")]),
            Self::Inno => Some(vec![
                "/VERYSILENT".into(),
                "/SUPPRESSMSGBOXES".into(),
                "/NORESTART".into(),
                "/SP-".into(),
                format!("/DIR={target}"),
            ]),
            Self::Msi => Some(vec![
                "/qn".into(),
                format!("TARGETDIR={target}"),
                format!("INSTALLDIR={target}"),
            ]),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Engine {
    Renpy,
    Kirikiri,
    Malie,
    Siglus,
    Bgi,
    CatSystem2,
    Nscripter,
    Artemis,
    Qlie,
    Yuris,
    Unity,
    RpgMaker,
    Tyrano,
    Electron,
    Unknown,
}

impl Engine {
    pub fn name(self) -> &'static str {
        match self {
            Self::Renpy => "Ren'Py",
            Self::Kirikiri => "KiriKiri",
            Self::Malie => "Malie",
            Self::Siglus => "SiglusEngine",
            Self::Bgi => "BGI/Ethornell",
            Self::CatSystem2 => "CatSystem2",
            Self::Nscripter => "NScripter",
            Self::Artemis => "Artemis",
            Self::Qlie => "QLIE",
            Self::Yuris => "YU-RIS",
            Self::Unity => "Unity",
            Self::RpgMaker => "RPG Maker",
            Self::Tyrano => "TyranoScript",
            Self::Electron => "Electron/NW.js",
            Self::Unknown => "unknown",
        }
    }
}

/// What a staged directory turned out to be.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Payload {
    Game { root: PathBuf, engine: Engine },
    Installer { exe: PathBuf, kind: InstallerKind },
}

pub fn classify(dir: &Path) -> Result<Payload> {
    let root = effective_root(dir);
    let engine = engine(&root);
    let exes = executables(&root);

    if engine != Engine::Unknown {
        return Ok(Payload::Game { root, engine });
    }

    let top_level: Vec<&PathBuf> = exes
        .iter()
        .filter(|e| e.parent() == Some(root.as_path()))
        .collect();
    let installer = top_level
        .iter()
        .find_map(|e| installer_kind(e).map(|k| ((*e).clone(), k)));
    if let Some((exe, kind)) = installer {
        // A lone installer next to data files is the common case; a game dir
        // that also happens to ship some other setup.exe is not.
        let others = top_level
            .iter()
            .filter(|e| **e != &exe && !is_helper(e))
            .count();
        if others == 0 {
            return Ok(Payload::Installer { exe, kind });
        }
    }
    if exes.iter().any(|e| !is_helper(e)) {
        return Ok(Payload::Game { root, engine });
    }
    if let Some(msi) = list(&root).into_iter().find(|p| ext_is(p, "msi")) {
        return Ok(Payload::Installer {
            exe: msi,
            kind: InstallerKind::Msi,
        });
    }
    bail!("no game or installer found in {}", root.display())
}

/// Follow single-directory wrappers (`Game_final/Game/...`) down to the
/// directory that actually has content. Readmes and junk do not count.
pub fn effective_root(dir: &Path) -> PathBuf {
    let mut cur = dir.to_path_buf();
    loop {
        let entries = list(&cur);
        let dirs: Vec<&PathBuf> = entries.iter().filter(|p| p.is_dir()).collect();
        let substantive_files = entries.iter().filter(|p| p.is_file() && !is_doc(p)).count();
        if dirs.len() == 1 && substantive_files == 0 {
            cur = dirs[0].clone();
        } else {
            return cur;
        }
    }
}

fn is_doc(p: &Path) -> bool {
    let name = lower_name(p);
    let docs = [
        "txt", "md", "pdf", "html", "htm", "url", "nfo", "rtf", "ini", "jpg", "png", "bmp",
    ];
    crate::archive::is_junk(Path::new(&name))
        || docs.iter().any(|e| name.ends_with(&format!(".{e}")))
}

pub fn engine(root: &Path) -> Engine {
    let files = list(root);
    let has = |name: &str| files.iter().any(|p| lower_name(p) == name);
    let has_ext = |ext: &str| files.iter().any(|p| p.is_file() && ext_is(p, ext));
    let has_dir = |name: &str| files.iter().any(|p| p.is_dir() && lower_name(p) == name);
    let exe_prefix = |prefix: &str| {
        files
            .iter()
            .any(|p| ext_is(p, "exe") && lower_name(p).starts_with(prefix))
    };

    if has_dir("renpy") && has_dir("game") {
        Engine::Renpy
    } else if has("malie.ini") || exe_prefix("malie") {
        Engine::Malie
    } else if has("scene.pck") || exe_prefix("siglusengine") {
        Engine::Siglus
    } else if has_ext("xp3") {
        Engine::Kirikiri
    } else if has("bgi.exe") || has("sysgrp.arc") {
        Engine::Bgi
    } else if has("nscript.dat") || has_ext("nsa") {
        Engine::Nscripter
    } else if has_ext("pfs") {
        Engine::Artemis
    } else if has_ext("ypf") {
        Engine::Yuris
    } else if has("cs2conf.dll") || (has_ext("int") && has("cs2.exe")) {
        Engine::CatSystem2
    } else if has_dir("gamedata")
        && list(&root.join("GameData"))
            .iter()
            .any(|p| ext_is(p, "pack"))
    {
        Engine::Qlie
    } else if has("unityplayer.dll")
        || files
            .iter()
            .any(|p| p.is_dir() && lower_name(p).ends_with("_data"))
    {
        Engine::Unity
    } else if root.join("www/js/rpg_core.js").exists() || root.join("js/rmmz_core.js").exists() {
        Engine::RpgMaker
    } else if has_dir("tyrano") || root.join("resources/app/tyrano").exists() {
        Engine::Tyrano
    } else if has("nw.dll") || root.join("resources/app.asar").exists() {
        Engine::Electron
    } else {
        Engine::Unknown
    }
}

/// Fingerprint the installer framework of a Windows executable, if any.
pub fn installer_kind(exe: &Path) -> Option<InstallerKind> {
    if ext_is(exe, "msi") {
        return Some(InstallerKind::Msi);
    }
    let mut buf = Vec::new();
    File::open(exe)
        .ok()?
        .take(8 << 20)
        .read_to_end(&mut buf)
        .ok()?;
    if !buf.starts_with(b"MZ") {
        return None;
    }
    let find = |needle: &[u8]| memchr::memmem::find(&buf, needle).is_some();
    if find(b"Nullsoft.NSIS.exehead") || find(b"NullsoftInst") {
        Some(InstallerKind::Nsis)
    } else if find(b"JR.Inno.Setup") || find(b"Inno Setup Setup Data") {
        Some(InstallerKind::Inno)
    } else if find(b".wixburn") {
        Some(InstallerKind::WixBurn)
    } else if find(b"InstallShield") {
        Some(InstallerKind::InstallShield)
    } else if find(b"Setup Factory") {
        Some(InstallerKind::SetupFactory)
    } else {
        let name = lower_name(exe);
        (name.starts_with("setup") || name.starts_with("install")).then_some(InstallerKind::Unknown)
    }
}

/// Every `.exe` under `root`, a few levels deep, sorted.
pub fn executables(root: &Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = WalkDir::new(root)
        .max_depth(4)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file() && ext_is(e.path(), "exe"))
        .map(|e| e.into_path())
        .collect();
    out.sort();
    out
}

/// Executables that ship alongside games but are never the game.
pub fn is_helper(exe: &Path) -> bool {
    let name = lower_name(exe);
    let stem = name.trim_end_matches(".exe");
    const PREFIXES: &[&str] = &[
        "unins",
        "uninst",
        "vcredist",
        "vc_redist",
        "dxsetup",
        "dxwebsetup",
        "directx",
        "dotnet",
        "ndp",
        "oalinst",
        "physx",
        "crashreport",
        "crashhandler",
        "unitycrashhandler",
        "zsync",
        "python",
        "notification_helper",
        "ue4prereq",
        "installer",
        "setup",
        "install",
    ];
    if PREFIXES.iter().any(|p| stem.starts_with(p)) {
        return true;
    }
    // Ren'Py's bundled interpreter lives in lib/, never the entry point.
    exe.components().any(|c| {
        let c = c.as_os_str().to_string_lossy().to_lowercase();
        c == "redist"
            || c == "_commonredist"
            || c == "directx"
            || c.starts_with("py3-")
            || c.starts_with("py2-")
    })
}

/// A way to start the game, e.g. the main exe and a config tool.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Target {
    pub id: String,
    pub name: String,
    /// Relative to the game's files directory.
    pub exe: PathBuf,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
}

/// Rank the candidate entry points of a game directory, best first.
pub fn targets(root: &Path, engine: Engine) -> Vec<Target> {
    let mut scored: Vec<(i64, PathBuf)> = executables(root)
        .into_iter()
        .filter(|e| !is_helper(e))
        .map(|e| (score(root, &e, engine), e))
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));

    let mut seen = std::collections::HashSet::new();
    scored
        .into_iter()
        .filter_map(|(_, exe)| {
            let rel = exe.strip_prefix(root).ok()?.to_path_buf();
            let stem = exe.file_stem()?.to_string_lossy().into_owned();
            let mut id = slug(&stem);
            while !seen.insert(id.clone()) {
                id.push('_');
            }
            Some(Target {
                id,
                name: stem,
                exe: rel,
                args: Vec::new(),
            })
        })
        .collect()
}

fn score(root: &Path, exe: &Path, engine: Engine) -> i64 {
    let name = lower_name(exe);
    let depth = exe
        .strip_prefix(root)
        .map(|p| p.components().count())
        .unwrap_or(9) as i64;
    let size = fs::metadata(exe).map(|m| m.len()).unwrap_or(0);
    let mut s = -depth * 20 + (size / (256 << 10)).min(40) as i64;

    let stem = name.trim_end_matches(".exe");
    if engine == Engine::Renpy && root.join(format!("{stem}.py")).exists() {
        s += 100;
    }
    const SIDEKICKS: &[&str] = &[
        "config", "setting", "launcher", "update", "patch", "readme", "manual", "tool",
    ];
    if SIDEKICKS.iter().any(|k| stem.contains(k)) {
        s -= 30;
    }
    let dir = lower_name(root);
    if !stem.is_empty() && (dir.contains(stem) || stem.contains(&dir)) {
        s += 15;
    }
    s
}

/// `Dies irae DX package` -> `dies-irae-dx-package`.
pub fn slug(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        if c.is_alphanumeric() {
            out.extend(c.to_lowercase());
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    let out = out.trim_end_matches('-').to_string();
    if out.is_empty() { "game".into() } else { out }
}

/// Ren'Py version from `renpy/vc_version.py`, e.g. "8.5.2".
pub fn renpy_version(root: &Path) -> Option<String> {
    let text = fs::read_to_string(root.join("renpy/vc_version.py")).ok()?;
    let line = text
        .lines()
        .find(|l| l.trim_start().starts_with("version ="))?;
    let v = line.split(['\'', '"']).nth(1)?;
    let parts: Vec<&str> = v.split('.').take(3).collect();
    (parts.len() == 3).then(|| parts.join("."))
}

fn list(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = fs::read_dir(dir)
        .map(|rd| rd.filter_map(|e| e.ok()).map(|e| e.path()).collect())
        .unwrap_or_default();
    v.sort();
    v
}

fn lower_name(p: &Path) -> String {
    p.file_name()
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

fn ext_is(p: &Path, ext: &str) -> bool {
    p.extension().is_some_and(|e| e.eq_ignore_ascii_case(ext))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs() {
        assert_eq!(slug("Dies irae DX package"), "dies-irae-dx-package");
        assert_eq!(slug("  HimaNatsu!! "), "himanatsu");
        assert_eq!(slug("向日葵"), "向日葵");
    }

    #[test]
    fn helpers() {
        assert!(is_helper(Path::new("x/unins000.exe")));
        assert!(is_helper(Path::new("x/lib/py3-windows-x86_64/pythonw.exe")));
        assert!(!is_helper(Path::new("x/malie_fabula.exe")));
    }
}
