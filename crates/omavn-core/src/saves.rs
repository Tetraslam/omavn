//! Find where a game keeps its saves, for backups and syncing.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use anyhow::{Context, Result, bail};
use walkdir::WalkDir;

use crate::library::{Game, Runner};
use crate::paths::Paths;

/// Folders Windows itself (or Wine) writes to, never game saves.
const SYSTEM: &[&str] = &[
    "microsoft",
    "temp",
    "wine",
    "openal",
    "nvidia",
    "nvidia corporation",
    "d3dscache",
    "crashdumps",
    "fontconfig",
    "gstreamer",
    "mesa",
    "dxvk",
];

/// Directories that look like save data, newest first. Saves are found by
/// what changed after install: in the prefix's user folders, in the game
/// folder (`save`, `savedata`), and for native Ren'Py in `~/.renpy`.
pub fn find(paths: &Paths, game: &Game) -> Vec<PathBuf> {
    let installed: SystemTime = game.installed_at.into();
    let mut roots: Vec<PathBuf> = Vec::new();
    let user = game.prefix(paths).join("drive_c/users/steamuser");
    for sub in [
        "AppData/Roaming",
        "AppData/Local",
        "AppData/LocalLow",
        "Documents",
        "Saved Games",
    ] {
        roots.push(user.join(sub));
    }
    if game.runner == Runner::NativeRenpy
        && let Some(home) = std::env::var_os("HOME")
    {
        roots.push(PathBuf::from(home).join(".renpy"));
    }

    let mut found: Vec<(SystemTime, PathBuf)> = Vec::new();
    for root in roots {
        let Ok(rd) = fs::read_dir(&root) else {
            continue;
        };
        for entry in rd.flatten() {
            let dir = entry.path();
            let name = entry.file_name().to_string_lossy().to_lowercase();
            if !dir.is_dir() || SYSTEM.contains(&name.as_str()) {
                continue;
            }
            if let Some(t) = newest_file(&dir).filter(|t| *t > installed) {
                found.push((t, dir));
            }
        }
    }
    let files = game.files_dir(paths);
    for entry in WalkDir::new(&files).max_depth(2).into_iter().flatten() {
        let name = entry.file_name().to_string_lossy().to_lowercase();
        if entry.file_type().is_dir()
            && (name.starts_with("save") || name == "userdata")
            && let Some(t) = newest_file(entry.path())
        {
            found.push((t, entry.into_path()));
        }
    }
    found.sort_by_key(|f| std::cmp::Reverse(f.0));
    found.dedup_by(|a, b| a.1 == b.1);
    found.into_iter().map(|(_, p)| p).collect()
}

fn newest_file(dir: &Path) -> Option<SystemTime> {
    WalkDir::new(dir)
        .max_depth(4)
        .into_iter()
        .flatten()
        .filter(|e| e.file_type().is_file())
        .filter_map(|e| e.metadata().ok()?.modified().ok())
        .max()
}

/// Copy a save folder from a Windows install into the game's prefix. The
/// path below `Users/<name>/` (e.g. `AppData/Roaming/light`) is mirrored
/// under the prefix's user folder, which is where the game will look.
pub fn import(paths: &Paths, game: &Game, src: &Path) -> Result<PathBuf> {
    if game.runner == Runner::NativeRenpy {
        bail!(
            "{} runs natively; copy Ren'Py saves into ~/.renpy/<game> instead",
            game.title
        );
    }
    let src = fs::canonicalize(src).with_context(|| format!("{} not found", src.display()))?;
    let parts: Vec<String> = src
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect();
    let users = parts
        .iter()
        .position(|p| p.eq_ignore_ascii_case("users"))
        .filter(|i| i + 2 < parts.len())
        .with_context(|| {
            format!("expected a path inside a Windows Users folder, like .../Users/<name>/AppData/Roaming/<game>, got {}", src.display())
        })?;
    let mut dest = game.prefix(paths).join("drive_c/users/steamuser");
    for p in &parts[users + 2..] {
        dest.push(p);
    }
    copy_dir(&src, &dest)?;
    Ok(dest)
}

fn copy_dir(src: &Path, dest: &Path) -> Result<()> {
    for entry in WalkDir::new(src) {
        let entry = entry?;
        let target = dest.join(entry.path().strip_prefix(src)?);
        if entry.file_type().is_dir() {
            fs::create_dir_all(&target)?;
        } else {
            fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}
