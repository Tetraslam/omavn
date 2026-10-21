//! Add installed games to Steam as non-Steam games, with library art, so
//! they show up in Big Picture and on a Steam Deck with controller support.
//!
//! Steam keeps these shortcuts in `userdata/<account>/config/shortcuts.vdf`
//! (binary VDF) and rewrites the file when it exits, so changes are only
//! safe while Steam is closed. Shortcuts that omavn didn't make are kept
//! byte for byte.

use std::fs;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use image::imageops::FilterType;
use image::{DynamicImage, GenericImageView};

use crate::art;
use crate::library::Game;
use crate::paths::Paths;

/// One value in a binary VDF document.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Map(Vec<(String, Value)>),
    Str(String),
    Int(i32),
    /// Types we pass through untouched: (type byte, raw bytes).
    Raw(u8, Vec<u8>),
}

impl Value {
    fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Map(m) => m
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(key))
                .map(|(_, v)| v),
            _ => None,
        }
    }
    fn str(&self, key: &str) -> Option<&str> {
        match self.get(key)? {
            Value::Str(s) => Some(s),
            _ => None,
        }
    }
}

pub fn parse(bytes: &[u8]) -> Result<Value> {
    let mut c = Cursor::new(bytes);
    let v = parse_map(&mut c)?;
    Ok(v)
}

fn parse_map(c: &mut Cursor<&[u8]>) -> Result<Value> {
    let mut entries = Vec::new();
    loop {
        let mut t = [0u8];
        if c.read(&mut t)? == 0 {
            return Ok(Value::Map(entries));
        }
        if t[0] == 0x08 {
            return Ok(Value::Map(entries));
        }
        let key = read_cstr(c)?;
        let value = match t[0] {
            0x00 => parse_map(c)?,
            0x01 => Value::Str(read_cstr(c)?),
            0x02 => {
                let mut b = [0u8; 4];
                c.read_exact(&mut b)?;
                Value::Int(i32::from_le_bytes(b))
            }
            0x03 | 0x04 | 0x06 => {
                let mut b = vec![0u8; 4];
                c.read_exact(&mut b)?;
                Value::Raw(t[0], b)
            }
            0x07 | 0x0a => {
                let mut b = vec![0u8; 8];
                c.read_exact(&mut b)?;
                Value::Raw(t[0], b)
            }
            other => bail!("unknown VDF value type 0x{other:02x}; not touching this file"),
        };
        entries.push((key, value));
    }
}

fn read_cstr(c: &mut Cursor<&[u8]>) -> Result<String> {
    let mut out = Vec::new();
    let mut b = [0u8];
    loop {
        c.read_exact(&mut b).context("truncated VDF")?;
        if b[0] == 0 {
            return Ok(String::from_utf8_lossy(&out).into_owned());
        }
        out.push(b[0]);
    }
}

pub fn write(root: &Value) -> Vec<u8> {
    let mut out = Vec::new();
    if let Value::Map(m) = root {
        write_entries(&mut out, m);
    }
    out.push(0x08);
    out
}

fn write_entries(out: &mut Vec<u8>, entries: &[(String, Value)]) {
    for (k, v) in entries {
        let (t, body): (u8, Vec<u8>) = match v {
            Value::Map(m) => {
                let mut b = Vec::new();
                write_entries(&mut b, m);
                b.push(0x08);
                (0x00, b)
            }
            Value::Str(s) => {
                let mut b = s.as_bytes().to_vec();
                b.push(0);
                (0x01, b)
            }
            Value::Int(i) => (0x02, i.to_le_bytes().to_vec()),
            Value::Raw(t, b) => (*t, b.clone()),
        };
        out.push(t);
        out.extend_from_slice(k.as_bytes());
        out.push(0);
        out.extend(body);
    }
}

/// The id Steam derives for a non-Steam shortcut (and names its art by).
pub fn shortcut_id(exe: &str, name: &str) -> u32 {
    crc32(format!("{exe}{name}").as_bytes()) | 0x8000_0000
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &b in data {
        crc ^= b as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

fn steam_root() -> Option<PathBuf> {
    let home = PathBuf::from(std::env::var_os("HOME")?);
    [".local/share/Steam", ".steam/steam", ".steam/root"]
        .iter()
        .map(|d| home.join(d))
        .find(|p| p.join("userdata").is_dir())
}

/// Steam accounts on this machine (their `userdata/<id>/config` dirs).
pub fn accounts() -> Vec<PathBuf> {
    let Some(root) = steam_root() else {
        return Vec::new();
    };
    let Ok(rd) = fs::read_dir(root.join("userdata")) else {
        return Vec::new();
    };
    let mut out: Vec<PathBuf> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name().is_some_and(|n| {
                n.to_string_lossy().bytes().all(|b| b.is_ascii_digit()) && n != "0"
            })
        })
        .map(|p| p.join("config"))
        .collect();
    out.sort();
    out
}

pub fn is_running() -> bool {
    Command::new("pgrep")
        .args(["-x", "steam"])
        .output()
        .is_ok_and(|o| o.status.success())
}

/// Ask Steam to quit and wait for it.
pub fn shutdown() -> Result<()> {
    let _ = Command::new("steam").arg("-shutdown").status();
    for _ in 0..60 {
        if !is_running() {
            return Ok(());
        }
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
    bail!("Steam didn't quit within a minute; close it and try again")
}

pub fn start() {
    let _ = Command::new("setsid")
        .args(["steam", "-silent"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
}

/// Shortcuts we made: their exe is omavn and they run `run <id>`.
fn ours(shortcut: &Value) -> Option<String> {
    let exe = shortcut.str("Exe").or(shortcut.str("exe"))?;
    let exe = exe.trim_matches('"');
    if Path::new(exe).file_name()? != "omavn" {
        return None;
    }
    let opts = shortcut.str("LaunchOptions")?;
    opts.strip_prefix("run ").map(|id| id.trim().to_string())
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Change {
    pub game: String,
    pub appid: u32,
    pub account: PathBuf,
}

/// Add (or refresh) `games` in every Steam account, with art. Existing
/// omavn shortcuts for these games are replaced; everything else is kept.
pub fn add(paths: &Paths, omavn: &Path, games: &[Game]) -> Result<Vec<Change>> {
    let accounts = accounts();
    if accounts.is_empty() {
        bail!("no Steam account found; start Steam and log in once first");
    }
    if is_running() {
        bail!("Steam is running; it would overwrite the changes when it exits");
    }
    let exe = format!("\"{}\"", omavn.display());
    let mut changes = Vec::new();
    for config in &accounts {
        let file = config.join("shortcuts.vdf");
        let mut list = read_shortcuts(&file)?;
        let ids: Vec<&str> = games.iter().map(|g| g.id.as_str()).collect();
        list.retain(|s| ours(s).is_none_or(|id| !ids.contains(&id.as_str())));
        for g in games {
            let appid = shortcut_id(&exe, &g.title);
            let icon = crate::meta::icon_path(paths, g);
            list.push(Value::Map(vec![
                ("appid".into(), Value::Int(appid as i32)),
                ("AppName".into(), Value::Str(g.title.clone())),
                ("Exe".into(), Value::Str(exe.clone())),
                (
                    "StartDir".into(),
                    Value::Str(format!("\"{}\"", g.dir(paths).display())),
                ),
                (
                    "icon".into(),
                    Value::Str(if icon.exists() {
                        icon.display().to_string()
                    } else {
                        String::new()
                    }),
                ),
                ("ShortcutPath".into(), Value::Str(String::new())),
                ("LaunchOptions".into(), Value::Str(format!("run {}", g.id))),
                ("IsHidden".into(), Value::Int(0)),
                ("AllowDesktopConfig".into(), Value::Int(1)),
                ("AllowOverlay".into(), Value::Int(1)),
                ("OpenVR".into(), Value::Int(0)),
                ("Devkit".into(), Value::Int(0)),
                ("DevkitGameID".into(), Value::Str(String::new())),
                ("DevkitOverrideAppID".into(), Value::Int(0)),
                ("LastPlayTime".into(), Value::Int(0)),
                ("FlatpakAppID".into(), Value::Str(String::new())),
                ("sortas".into(), Value::Str(String::new())),
                (
                    "tags".into(),
                    Value::Map(vec![("0".into(), Value::Str("Visual Novels".into()))]),
                ),
            ]));
            write_art(paths, g, &config.join("grid"), appid);
            changes.push(Change {
                game: g.id.clone(),
                appid,
                account: config.clone(),
            });
        }
        save_shortcuts(&file, list)?;
    }
    Ok(changes)
}

/// Remove omavn's shortcuts for `ids` (all of them when empty).
pub fn remove(ids: &[String]) -> Result<Vec<String>> {
    if is_running() {
        bail!("Steam is running; it would overwrite the changes when it exits");
    }
    let mut removed = Vec::new();
    for config in accounts() {
        let file = config.join("shortcuts.vdf");
        if !file.exists() {
            continue;
        }
        let mut list = read_shortcuts(&file)?;
        list.retain(|s| match ours(s) {
            Some(id) if ids.is_empty() || ids.contains(&id) => {
                if let Some(Value::Int(appid)) = s.get("appid") {
                    remove_art(&config.join("grid"), *appid as u32);
                }
                removed.push(id);
                false
            }
            _ => true,
        });
        save_shortcuts(&file, list)?;
    }
    removed.sort();
    removed.dedup();
    Ok(removed)
}

/// Ids of games that have an omavn shortcut in Steam.
pub fn listed() -> Vec<String> {
    let mut out: Vec<String> = accounts()
        .iter()
        .filter_map(|c| read_shortcuts(&c.join("shortcuts.vdf")).ok())
        .flatten()
        .filter_map(|s| ours(&s))
        .collect();
    out.sort();
    out.dedup();
    out
}

fn read_shortcuts(file: &Path) -> Result<Vec<Value>> {
    let Ok(bytes) = fs::read(file) else {
        return Ok(Vec::new());
    };
    let root = parse(&bytes).with_context(|| format!("reading {}", file.display()))?;
    match root.get("shortcuts") {
        Some(Value::Map(m)) => Ok(m.iter().map(|(_, v)| v.clone()).collect()),
        _ => Ok(Vec::new()),
    }
}

fn save_shortcuts(file: &Path, list: Vec<Value>) -> Result<()> {
    if file.exists() {
        let backup = file.with_extension("vdf.omavn-backup");
        if !backup.exists() {
            fs::copy(file, &backup)?;
        }
    }
    let entries = list
        .into_iter()
        .enumerate()
        .map(|(i, v)| (i.to_string(), v))
        .collect();
    let root = Value::Map(vec![("shortcuts".into(), Value::Map(entries))]);
    fs::create_dir_all(file.parent().unwrap())?;
    let tmp = file.with_extension("vdf.tmp");
    fs::write(&tmp, write(&root))?;
    fs::rename(tmp, file)?;
    Ok(())
}

/// Steam's library art for a shortcut: portrait capsule (`p`), wide
/// capsule, hero banner, and icon.
fn write_art(paths: &Paths, game: &Game, grid: &Path, appid: u32) {
    let _ = fs::create_dir_all(grid);
    let cover = image::open(art::cover(paths, game)).ok();
    let shot = image::open(art::screenshot(paths, game)).ok();
    let card = image::open(art::card(paths, game)).ok();
    let save = |img: DynamicImage, name: String| {
        let _ = img.to_rgb8().save(grid.join(name));
    };
    if let Some(c) = cover.as_ref().or(card.as_ref()) {
        save(fill(c, 600, 900), format!("{appid}p.jpg"));
    }
    if let Some(c) = card.as_ref().or(cover.as_ref()) {
        save(fill(c, 920, 430), format!("{appid}.jpg"));
    }
    if let Some(s) = shot.as_ref().or(card.as_ref()) {
        save(fill(s, 1920, 620), format!("{appid}_hero.jpg"));
    }
}

fn remove_art(grid: &Path, appid: u32) {
    for suffix in ["p.jpg", ".jpg", "_hero.jpg", "_logo.png", "_icon.png"] {
        let _ = fs::remove_file(grid.join(format!("{appid}{suffix}")));
    }
}

/// Scale to cover `w`x`h` and crop the overflow, keeping the top (faces
/// and logos sit there more often than at the bottom).
fn fill(img: &DynamicImage, w: u32, h: u32) -> DynamicImage {
    let (iw, ih) = img.dimensions();
    let scale = (w as f64 / iw as f64).max(h as f64 / ih as f64);
    let (sw, sh) = (
        (iw as f64 * scale).ceil() as u32,
        (ih as f64 * scale).ceil() as u32,
    );
    let scaled = img.resize_exact(sw, sh, FilterType::CatmullRom);
    scaled.crop_imm((sw - w) / 2, (sh - h) / 4, w, h)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc_matches_known_value() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }

    #[test]
    fn vdf_round_trips() {
        let doc = Value::Map(vec![(
            "shortcuts".into(),
            Value::Map(vec![(
                "0".into(),
                Value::Map(vec![
                    ("appid".into(), Value::Int(-123)),
                    ("AppName".into(), Value::Str("Dies irae".into())),
                    (
                        "LastPlayTime".into(),
                        Value::Raw(0x07, vec![1, 2, 3, 4, 5, 6, 7, 8]),
                    ),
                    (
                        "tags".into(),
                        Value::Map(vec![("0".into(), Value::Str("x".into()))]),
                    ),
                ]),
            )]),
        )]);
        let bytes = write(&doc);
        assert_eq!(parse(&bytes).unwrap(), doc);
        assert_eq!(write(&parse(&bytes).unwrap()), bytes);
    }
}
