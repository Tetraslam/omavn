//! Look at a directory and work out what it is: a playable game (and with
//! which engine), or an installer that has to run first.

use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{Result, bail};
use walkdir::WalkDir;

use crate::archive;

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
    LiveMaker,
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
            Self::LiveMaker => "LiveMaker",
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
    // Disc-style releases: a setup.exe (and autorun.inf, readmes) next to
    // the finished game folder, which the installer would only copy.
    let subgames: Vec<(PathBuf, Engine)> = list(&root)
        .into_iter()
        .filter(|p| p.is_dir() && !archive::is_junk(Path::new(&lower_name(p))))
        .map(|d| {
            let e = self::engine(&d);
            (d, e)
        })
        .filter(|(_, e)| *e != Engine::Unknown)
        .collect();
    if let [(sub, engine)] = subgames.as_slice() {
        return Ok(Payload::Game {
            root: sub.clone(),
            engine: *engine,
        });
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
    } else if has_ext("xp3") || exe_contains(&files, b"TVP(KIRIKIRI)") {
        // KiriKiri can also carry its data appended to the exe.
        Engine::Kirikiri
    } else if has("bgi.exe") || has("sysgrp.arc") {
        Engine::Bgi
    } else if has("nscript.dat") || has_ext("nsa") {
        Engine::Nscripter
    } else if has_ext("pfs") {
        Engine::Artemis
    } else if has_ext("ypf")
        || has("yscfg.dat")
        || list(&root.join("pac")).iter().any(|p| ext_is(p, "ypf"))
    {
        Engine::Yuris
    } else if has("live.dll") {
        Engine::LiveMaker
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

/// Whether any top-level exe contains `needle` in its first 16 MiB.
fn exe_contains(files: &[PathBuf], needle: &[u8]) -> bool {
    files.iter().filter(|p| ext_is(p, "exe")).any(|exe| {
        let mut buf = Vec::new();
        File::open(exe)
            .and_then(|f| f.take(16 << 20).read_to_end(&mut buf))
            .is_ok()
            && memchr::memmem::find(&buf, needle).is_some()
    })
}

/// Whether file names under `root` contain kana, which means a Japanese
/// game. Those usually need a Japanese locale: their text and paths are
/// Shift-JIS, which other locales turn into mojibake or missing files.
pub fn is_japanese(root: &Path, engine: Engine) -> bool {
    let kana = |c: char| matches!(c, '\u{3040}'..='\u{30ff}' | '\u{ff66}'..='\u{ff9f}');
    let kana_names = WalkDir::new(root)
        .max_depth(3)
        .into_iter()
        .filter_map(|e| e.ok())
        .any(|e| e.file_name().to_string_lossy().chars().any(kana));
    // Kanji-only names don't tell Japanese from Chinese; the exe's own
    // resources do (language 0x0411).
    kana_names
        || targets(root, engine)
            .first()
            .is_some_and(|t| resource_languages(&root.join(&t.exe)).contains(&0x0411))
}

/// Language ids of a PE file's resources.
fn resource_languages(exe: &Path) -> Vec<u32> {
    use pelite::resources::{Entry, Name};
    let Ok(bytes) = fs::read(exe) else {
        return Vec::new();
    };
    let Ok(pe) = pelite::PeFile::from_bytes(&bytes) else {
        return Vec::new();
    };
    let Ok(res) = pe.resources() else {
        return Vec::new();
    };
    let Ok(root) = res.root() else {
        return Vec::new();
    };
    let mut langs = Vec::new();
    // Resource trees go type -> name -> language.
    for ty in root.entries() {
        let Ok(Entry::Directory(names)) = ty.entry() else {
            continue;
        };
        for name in names.entries() {
            let Ok(Entry::Directory(langdir)) = name.entry() else {
                continue;
            };
            for lang in langdir.entries() {
                if let Ok(Name::Id(id)) = lang.name()
                    && !langs.contains(&id)
                {
                    langs.push(id);
                }
            }
        }
    }
    langs
}

/// Names that say nothing about the game.
const GENERIC: &[&str] = &[
    "data",
    "game",
    "games",
    "files",
    "bin",
    "app",
    "release",
    "build",
    "start",
    "launcher",
    "tyranoproject",
    "nw",
    "main",
    "play",
    "setup",
];

/// Drop platform and version noise from a slug: `teadv2-2-01-win` -> `teadv2`.
fn trim_slug(slug: &str) -> String {
    const NOISE: &[&str] = &[
        "win", "w", "windows", "win32", "win64", "x64", "x86", "pc", "final", "freem", "full",
        "ver", "v", "en", "eng", "english",
    ];
    // Words that describe the download, wherever they appear.
    const PACKAGING: &[&str] = &[
        "setup",
        "installer",
        "install",
        "r18",
        "unrated",
        "version",
        "release",
        "dl",
        "complete",
    ];
    let mut parts: Vec<&str> = slug.split('-').collect();
    if parts.iter().any(|p| !PACKAGING.contains(p)) {
        parts.retain(|p| !PACKAGING.contains(p));
    }
    while parts.len() > 1 {
        let last = parts[parts.len() - 1];
        let version = last.trim_start_matches("ver").trim_start_matches('v');
        // `1-0-2en`: a version number with a language tag glued on.
        let lang_version = last.len() > 2
            && ["en", "jp", "cn", "tw"].iter().any(|l| last.ends_with(l))
            && last[..last.len() - 2].bytes().all(|b| b.is_ascii_digit());
        if NOISE.contains(&last)
            || lang_version
            || (!version.is_empty() && version.bytes().all(|b| b.is_ascii_digit()))
        {
            parts.pop();
        } else {
            break;
        }
    }
    parts.join("-")
}

/// A typable library id for a game without a catalog entry: the first
/// ASCII, non-generic candidate among the folder, exe, and download names.
pub fn guess_id(root: &Path, engine: Engine, download: &str) -> String {
    let mut candidates: Vec<String> = Vec::new();
    if let Some(dir) = root.file_name() {
        candidates.push(dir.to_string_lossy().into_owned());
    }
    if let Some(t) = targets(root, engine).first() {
        candidates.push(t.name.clone());
    }
    candidates.push(download.to_string());
    let slugs: Vec<String> = candidates.iter().map(|c| trim_slug(&slug(c))).collect();
    slugs
        .iter()
        .find(|s| s.is_ascii() && !GENERIC.contains(&s.as_str()))
        .or_else(|| slugs.iter().find(|s| !GENERIC.contains(&s.as_str())))
        .cloned()
        .unwrap_or_else(|| "game".into())
}

/// The words of a download's file name, minus packaging noise and version
/// numbers, in order: `eden-of-grisaia-unrated` -> `eden of grisaia`.
pub fn clean_name(download: &str) -> String {
    let mut name = download.to_lowercase();
    loop {
        let before = name.len();
        for ext in [
            ".zip", ".7z", ".rar", ".exe", ".iso", ".001", ".002", ".part1",
        ] {
            if let Some(s) = name.strip_suffix(ext) {
                name = s.to_string();
            }
        }
        if name.len() == before {
            break;
        }
    }
    let slugged = trim_slug(&slug(&name));
    slugged.replace('-', " ")
}

/// A display title for a game without a catalog entry or VNDB match:
/// Ren'Py's build name, the exe's product name, or the exe's own name.
pub fn guess_title(root: &Path, engine: Engine, fallback: &str) -> String {
    let renpy_name = fs::read_to_string(root.join("game/cache/build_info.json"))
        .ok()
        .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
        .and_then(|v| v["name"].as_str().map(str::to_owned));
    let exe = targets(root, engine).into_iter().next();
    let product = exe.as_ref().and_then(|t| product_name(&root.join(&t.exe)));
    let stem = exe
        .map(|t| t.name)
        .filter(|n| !GENERIC.contains(&slug(n).as_str()));
    renpy_name
        .or_else(|| inno_app_name(root))
        .or(product)
        .or(stem)
        .unwrap_or_else(|| fallback.replace(['-', '_'], " "))
}

/// The app name Inno Setup recorded in its uninstall log, for games that
/// came from an Inno installer.
fn inno_app_name(root: &Path) -> Option<String> {
    let mut head = [0u8; 320];
    File::open(root.join("unins000.dat"))
        .ok()?
        .read_exact(&mut head)
        .ok()?;
    if !head.starts_with(b"Inno Setup Uninstall Log") {
        return None;
    }
    let field = &head[192..320];
    let end = field.iter().position(|b| *b == 0).unwrap_or(field.len());
    let name = String::from_utf8_lossy(&field[..end]).trim().to_string();
    (!name.is_empty()).then_some(name)
}

/// `ProductName` from an exe's version resource, unless it just names the engine.
fn product_name(exe: &Path) -> Option<String> {
    let bytes = fs::read(exe).ok()?;
    let pe = pelite::PeFile::from_bytes(&bytes).ok()?;
    let info = pe.resources().ok()?.version_info().ok()?;
    let lang = *info.translation().first()?;
    let name = info.value(lang, "ProductName")?.trim().to_string();
    const ENGINES: &[&str] = &[
        "kirikiri",
        "tvp",
        "ren'py",
        "renpy",
        "unity",
        "electron",
        "nw.js",
        "livemaker",
        "nscripter",
        "yu-ris",
        "artemis",
        "siglus",
        "tyrano",
        "rpg maker",
        "microsoft",
        "cmvs",
    ];
    let lower = name.to_lowercase();
    (!name.is_empty() && !ENGINES.iter().any(|e| lower.contains(e))).then_some(name)
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
        // Staging links to the user's files instead of copying them.
        .follow_links(true)
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
    if PREFIXES.iter().any(|p| stem.starts_with(p)) || stem.contains("updater") {
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
        "config", "setting", "launcher", "update", "patch", "readme", "manual", "tool", "bootmenu",
    ];
    if SIDEKICKS.iter().any(|k| stem.contains(k)) {
        s -= 30;
    }
    // Engine binaries behind a named launcher (`engine.exe` next to
    // `Dracu-Riot!.exe`) start the same game but name nothing.
    const GENERIC_EXES: &[&str] = &[
        "engine",
        "engine32",
        "engine64",
        "game",
        "app",
        "main",
        "nw",
        "bootstrap",
    ];
    if GENERIC_EXES.contains(&stem) {
        s -= 40;
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

/// Whether a Windows executable is 32-bit (i386), from its PE header.
pub fn is_32bit_exe(exe: &Path) -> bool {
    let mut head = [0u8; 4096];
    let Ok(n) = File::open(exe).and_then(|mut f| f.read(&mut head)) else {
        return false;
    };
    let head = &head[..n];
    let Some(pe) = head
        .get(0x3c..0x40)
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize)
    else {
        return false;
    };
    head.get(pe..pe + 6)
        .is_some_and(|h| &h[..4] == b"PE\0\0" && h[4..6] == [0x4c, 0x01])
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
    fn pe_bitness() {
        let dir = std::env::temp_dir().join(format!("omavn-pe-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let mut img = vec![0u8; 0x100];
        img[..2].copy_from_slice(b"MZ");
        img[0x3c] = 0x80;
        img[0x80..0x84].copy_from_slice(b"PE\0\0");
        img[0x84..0x86].copy_from_slice(&[0x4c, 0x01]);
        fs::write(dir.join("x86.exe"), &img).unwrap();
        img[0x84..0x86].copy_from_slice(&[0x64, 0x86]);
        fs::write(dir.join("x64.exe"), &img).unwrap();
        assert!(is_32bit_exe(&dir.join("x86.exe")));
        assert!(!is_32bit_exe(&dir.join("x64.exe")));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn clean_names() {
        assert_eq!(clean_name("eden-of-grisaia-unrated.7z"), "eden of grisaia");
        assert_eq!(clean_name("Dracu Riot R18 v1.0 Release"), "dracu riot");
        assert_eq!(clean_name("Majikoi_Installer"), "majikoi");
        assert_eq!(clean_name("hapymaher_en"), "hapymaher");
        assert_eq!(clean_name("RiddleJokerR18_1.00"), "riddlejokerr18");
    }

    #[test]
    fn trims_slugs() {
        assert_eq!(trim_slug("teadv2-2-01-win"), "teadv2");
        assert_eq!(
            trim_slug("wakarebanashi-failed-freem-1-0-win"),
            "wakarebanashi-failed"
        );
        assert_eq!(trim_slug("4gatsu-8ka-ver2-0-0-4"), "4gatsu-8ka");
        assert_eq!(trim_slug("dies-irae"), "dies-irae");
        assert_eq!(trim_slug("gss-setup-1-0-2en"), "gss");
        assert_eq!(trim_slug("senrenbanka-r18"), "senrenbanka");
        assert_eq!(trim_slug("muramasa-installer-1-04"), "muramasa");
    }

    #[test]
    fn helpers() {
        assert!(is_helper(Path::new("x/unins000.exe")));
        assert!(is_helper(Path::new("x/lib/py3-windows-x86_64/pythonw.exe")));
        assert!(!is_helper(Path::new("x/malie_fabula.exe")));
    }
}
