//! Moving from Windows: find each installed game's saves on a Windows
//! drive (or a backup of one) and copy them to where omavn's copy of the
//! game will look.
//!
//! Games save in three kinds of places: the user's AppData or Documents
//! (under the game's or the developer's name), Ren'Py's shared `RenPy`
//! folder, or next to the game itself (KiriKiri's `savedata`). The first
//! two are mirrored into the game's Wine prefix (or `~/.renpy` for native
//! Ren'Py); the last into the game's own folder.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use walkdir::WalkDir;

use crate::detect;
use crate::library::{Game, Runner};
use crate::paths::Paths;

#[derive(Debug, Clone, serde::Serialize)]
pub struct Found {
    pub game: String,
    pub from: PathBuf,
    pub to: PathBuf,
    /// Why this folder was matched to the game.
    pub why: String,
    pub files: usize,
}

/// Profile folders where games write (relative to `Users/<name>`).
const SAVE_BASES: &[&str] = &[
    "AppData/Roaming",
    "AppData/Local",
    "AppData/LocalLow",
    "Documents",
    "Documents/My Games",
    "Saved Games",
];

/// Not saves, whatever the name.
const IGNORE: &[&str] = &[
    "microsoft",
    "packages",
    "temp",
    "google",
    "mozilla",
    "nvidia",
    "nvidia corporation",
    "amd",
    "intel",
    "steam",
    "discord",
    "programs",
    "crashdumps",
    "d3dscache",
    "connecteddevicesplatform",
    "comms",
    "renpy",
    "my games",
    "unity",
    "low",
    "desktop.ini",
];

/// Everything on `root` that looks like saves for one of `games`.
pub fn scan(paths: &Paths, games: &[Game], root: &Path) -> Result<Vec<Found>> {
    let root = fs::canonicalize(root).with_context(|| format!("{} not found", root.display()))?;
    let profiles = profiles(&root);
    let mut found = Vec::new();

    for profile in &profiles {
        for base in SAVE_BASES {
            let Some(dir) = find_ci(profile, base) else {
                continue;
            };
            for cand in children(&dir) {
                let name = file_name(&cand);
                if IGNORE.contains(&name.to_lowercase().as_str()) {
                    continue;
                }
                if let Some((game, why)) = best_match(paths, games, profile, &cand) {
                    let rel = cand.strip_prefix(profile).unwrap_or(&cand);
                    found.push(Found {
                        game: game.id.clone(),
                        from: cand.clone(),
                        to: prefix_user(paths, game).join(rel),
                        why,
                        files: count_files(&cand),
                    });
                }
            }
        }
        // Ren'Py keeps every game's saves in one RenPy folder.
        if let Some(renpy) = find_ci(profile, "AppData/Roaming/RenPy") {
            for cand in children(&renpy) {
                let Some(game) = games
                    .iter()
                    .find(|g| renpy_dir_matches(g, &file_name(&cand)))
                else {
                    continue;
                };
                let to = match game.runner {
                    Runner::NativeRenpy => home().join(".renpy").join(file_name(&cand)),
                    Runner::Proton => prefix_user(paths, game)
                        .join("AppData/Roaming/RenPy")
                        .join(file_name(&cand)),
                };
                found.push(Found {
                    game: game.id.clone(),
                    from: cand.clone(),
                    to,
                    why: "Ren'Py save folder".into(),
                    files: count_files(&cand),
                });
            }
        }
    }

    // Saves kept beside the game: find the old install by its files.
    for game in games {
        for install in old_installs(&root, &profiles, paths, game) {
            for sub in children(&install) {
                let n = file_name(&sub).to_lowercase();
                if n.starts_with("sav") || n == "userdata" {
                    let to = game.files_dir(paths).join(file_name(&sub));
                    found.push(Found {
                        game: game.id.clone(),
                        from: sub.clone(),
                        to,
                        why: format!("next to the game in {}", install.display()),
                        files: count_files(&sub),
                    });
                }
            }
        }
    }
    found.retain(|f| f.files > 0);
    found.sort_by(|a, b| a.game.cmp(&b.game).then(a.from.cmp(&b.from)));
    found.dedup_by(|a, b| a.from == b.from);
    Ok(found)
}

/// Copy one match into place. Anything already there is moved to the
/// game's `backup/` folder first, so nothing is lost.
pub fn copy(paths: &Paths, game: &Game, found: &Found) -> Result<Option<PathBuf>> {
    let mut backup = None;
    if found.to.exists() && count_files(&found.to) > 0 {
        let stamp = time::OffsetDateTime::now_utc().unix_timestamp();
        let b = game
            .dir(paths)
            .join("backup")
            .join(format!("saves-{stamp}"))
            .join(file_name(&found.to));
        fs::create_dir_all(b.parent().unwrap())?;
        copy_tree(&found.to, &b)?;
        backup = Some(b);
    }
    copy_tree(&found.from, &found.to)?;
    Ok(backup)
}

fn copy_tree(src: &Path, dest: &Path) -> Result<()> {
    for entry in WalkDir::new(src) {
        let entry = entry?;
        let target = dest.join(entry.path().strip_prefix(src)?);
        if entry.file_type().is_dir() {
            fs::create_dir_all(&target)?;
        } else if entry.file_type().is_file() {
            if let Some(p) = target.parent() {
                fs::create_dir_all(p)?;
            }
            fs::copy(entry.path(), &target)
                .with_context(|| format!("copying {}", entry.path().display()))?;
        }
    }
    Ok(())
}

/// User profiles under `root`: a whole drive, its `Users` folder, or one
/// user's folder (or a backup of one).
fn profiles(root: &Path) -> Vec<PathBuf> {
    if find_ci(root, "AppData").is_some() || find_ci(root, "Documents").is_some() {
        return vec![root.to_path_buf()];
    }
    let users = if file_name(root).eq_ignore_ascii_case("users") {
        Some(root.to_path_buf())
    } else {
        find_ci(root, "Users")
    };
    let Some(users) = users else {
        return Vec::new();
    };
    children(&users)
        .into_iter()
        .filter(|p| {
            let n = file_name(p).to_lowercase();
            ![
                "public",
                "default",
                "default user",
                "all users",
                "defaultapppool",
            ]
            .contains(&n.as_str())
        })
        .collect()
}

fn prefix_user(paths: &Paths, game: &Game) -> PathBuf {
    game.prefix(paths).join("drive_c/users/steamuser")
}

/// Which game a profile folder belongs to, and why.
fn best_match<'a>(
    paths: &Paths,
    games: &'a [Game],
    profile: &Path,
    cand: &Path,
) -> Option<(&'a Game, String)> {
    let rel = cand.strip_prefix(profile).ok()?;
    let c = compact(&file_name(cand));
    if c.len() < 3 {
        return None;
    }
    let mut best: Option<(u8, &Game, String)> = None;
    for g in games {
        let score_why = if find_ci(&prefix_user(paths, g), &rel.to_string_lossy()).is_some() {
            Some((4, "the game already uses this folder".to_string()))
        } else if names(g)
            .iter()
            .any(|n| n == &c || (n.len() >= 5 && (c.contains(n.as_str()) || n.contains(&c))))
        {
            Some((3, "named after the game".to_string()))
        } else if g
            .developers
            .iter()
            .map(|d| compact(d))
            .any(|d| d.len() >= 3 && d == c)
        {
            Some((
                2,
                format!("named after its developer ({})", file_name(cand)),
            ))
        } else {
            None
        };
        if let Some((score, why)) = score_why {
            match &best {
                Some((s, _, _)) if *s >= score => {
                    // Two games claim a developer folder: ambiguous, skip it.
                    if *s == score && score == 2 {
                        best = Some((score, g, "ambiguous".into()));
                    }
                }
                _ => best = Some((score, g, why)),
            }
        }
    }
    best.filter(|(_, _, why)| why != "ambiguous")
        .map(|(_, g, why)| (g, why))
}

/// Ways a game is called, squashed to letters and digits.
fn names(g: &Game) -> Vec<String> {
    let mut v = vec![compact(&g.title), compact(&g.id)];
    if let Some(o) = &g.original_title {
        v.push(compact(o));
    }
    v.extend(
        g.targets
            .iter()
            .filter_map(|t| t.exe.file_stem().map(|s| compact(&s.to_string_lossy()))),
    );
    v.retain(|n| n.len() >= 3 && !["engine", "game", "launcher"].contains(&n.as_str()));
    v
}

/// Ren'Py names its folder `<save_directory>`, usually `<Game>-<number>`.
fn renpy_dir_matches(g: &Game, dir: &str) -> bool {
    if g.engine != detect::Engine::Renpy {
        return false;
    }
    let base = compact(dir.rsplit_once('-').map(|(a, _)| a).unwrap_or(dir));
    base.len() >= 3
        && names(g)
            .iter()
            .any(|n| n == &base || n.starts_with(&base) || base.starts_with(n.as_str()))
}

/// Folders on the Windows drive holding an earlier install of `game`:
/// they have the game's exe and most of its other top-level files.
fn old_installs(root: &Path, profiles: &[PathBuf], paths: &Paths, game: &Game) -> Vec<PathBuf> {
    let files = game.files_dir(paths);
    let ours: Vec<String> = children(&files)
        .iter()
        .map(|p| file_name(p).to_lowercase())
        .collect();
    let exes: Vec<String> = game
        .targets
        .iter()
        .filter_map(|t| {
            t.exe
                .file_name()
                .map(|n| n.to_string_lossy().to_lowercase())
        })
        .collect();
    let mut search: Vec<PathBuf> = vec![root.to_path_buf()];
    for p in profiles {
        for sub in ["Desktop", "Downloads", "Documents", "Games"] {
            search.extend(find_ci(p, sub));
        }
    }
    let mut hits = Vec::new();
    for base in search {
        let walker = WalkDir::new(&base)
            .max_depth(5)
            .into_iter()
            .filter_entry(|e| {
                let n = e.file_name().to_string_lossy().to_lowercase();
                e.depth() == 0
                    || !(n == "windows"
                        || n == "appdata"
                        || n == "users"
                        || n.starts_with('$')
                        || n == "programdata")
            });
        for e in walker.flatten() {
            if !e.file_type().is_file()
                || !exes.contains(&e.file_name().to_string_lossy().to_lowercase())
            {
                continue;
            }
            let dir = e.path().parent().unwrap().to_path_buf();
            let theirs: Vec<String> = children(&dir)
                .iter()
                .map(|p| file_name(p).to_lowercase())
                .collect();
            let shared = ours.iter().filter(|n| theirs.contains(n)).count();
            if shared * 2 >= ours.len().min(theirs.len()).max(2) && !hits.contains(&dir) {
                hits.push(dir);
            }
        }
    }
    hits
}

/// `base/rel`, matching each component case-insensitively.
fn find_ci(base: &Path, rel: &str) -> Option<PathBuf> {
    let mut cur = base.to_path_buf();
    for comp in rel.split(['/', '\\']).filter(|c| !c.is_empty()) {
        let next = fs::read_dir(&cur)
            .ok()?
            .flatten()
            .map(|e| e.path())
            .find(|p| file_name(p).eq_ignore_ascii_case(comp))?;
        cur = next;
    }
    cur.is_dir().then_some(cur)
}

fn children(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = fs::read_dir(dir)
        .map(|rd| {
            rd.flatten()
                .map(|e| e.path())
                .filter(|p| p.is_dir())
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v
}

fn count_files(dir: &Path) -> usize {
    WalkDir::new(dir)
        .into_iter()
        .flatten()
        .filter(|e| e.file_type().is_file())
        .count()
}

fn compact(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

fn file_name(p: &Path) -> String {
    p.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default()
}
