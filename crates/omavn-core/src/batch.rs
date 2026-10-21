//! Many downloads at once: sort a folder of loose files (or a list of
//! paths) into games, patches for those games, and leftovers.
//!
//! A real "my visual novels" folder mixes base games, split archives
//! (`x.7z.001` + `x.7z.002` + the `x.exe` that extracts them), installers
//! with their data files, updates, and old copies of the same game.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::archive;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Role {
    Game,
    /// Applies to the game at `base` (an index into the plan), if present.
    Patch {
        base: Option<usize>,
    },
    /// Another download of a game already in the plan (an older copy).
    Duplicate {
        of: usize,
    },
    /// A patch to a version the game's download already has.
    Included {
        base: usize,
    },
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Unit {
    /// Human name derived from the file names.
    pub name: String,
    pub files: Vec<PathBuf>,
    pub role: Role,
    /// Size of all files, in bytes.
    pub size: u64,
}

/// Words that say what a download is, not which game it is.
const NOISE: &[&str] = &[
    "setup",
    "installer",
    "install",
    "patch",
    "update",
    "updater",
    "r18",
    "18",
    "unrated",
    "uncensored",
    "hd",
    "complete",
    "release",
    "dl",
    "final",
    "en",
    "eng",
    "english",
    "jp",
    "old",
    "new",
    "full",
    "win",
    "windows",
    "version",
    "ver",
    "edition",
    "v",
    "the",
    "of",
    "a",
    "no",
    "dx",
    "package",
    "data",
];

/// Words that mark a download as an update to something else.
const PATCH_WORDS: &[&str] = &["patch", "update", "updater", "fix", "hotfix"];

/// Plan an install of everything in `paths`: files are grouped, and each
/// directory counts as one download (`Dies irae/` with its three zips).
pub fn plan(paths: &[PathBuf]) -> Vec<Unit> {
    let mut dirs = Vec::new();
    let mut files = Vec::new();
    for p in paths {
        if p.is_dir() {
            if !is_system_dir(p) && du(p) > 0 {
                dirs.push(p.clone());
            }
        } else if p.is_file() && !is_junk(p) {
            files.push(p.clone());
        }
    }
    files.sort();

    let mut groups: Vec<Vec<PathBuf>> = dirs.into_iter().map(|d| vec![d]).collect();
    let sets = archive::group(&files);
    let mut claimed: BTreeSet<PathBuf> = BTreeSet::new();
    for set in &sets {
        claimed.extend(set.volumes.iter().cloned());
        groups.push(set.volumes.clone());
    }
    // `foo.7z.001` sets need the plain `.002`... volumes too; and the
    // `foo.exe` beside them is the extractor stub.
    let mut loose: Vec<PathBuf> = files.into_iter().filter(|f| !claimed.contains(f)).collect();
    for group in groups.iter_mut() {
        let key = stem_key(&group[0]);
        loose.retain(|f| {
            if stem_key(f) == key && !group.contains(f) {
                group.push(f.clone());
                false
            } else {
                true
            }
        });
    }
    // Remaining loose files: join others with the same stem (`setup.exe` +
    // `setup-1.bin`), otherwise stand alone.
    for f in loose {
        let key = stem_key(&f);
        match groups
            .iter_mut()
            .find(|g| g[0].is_file() && stem_key(&g[0]) == key)
        {
            Some(g) => g.push(f),
            None => groups.push(vec![f]),
        }
    }

    let mut units: Vec<Unit> = groups
        .into_iter()
        .map(|mut files| {
            files.sort();
            let name = display_name(&files[0]);
            let size = files.iter().map(|f| du(f)).sum();
            let role = if is_patch(&files) {
                Role::Patch { base: None }
            } else {
                Role::Game
            };
            Unit {
                name,
                files,
                role,
                size,
            }
        })
        .collect();
    units.sort_by_cached_key(|u| u.name.to_lowercase());

    // Duplicates: two games with the same identity keep the one that
    // isn't marked old (else the bigger, usually more complete, one).
    let ids: Vec<BTreeSet<String>> = units.iter().map(|u| identity(&u.files[0])).collect();
    for i in 0..units.len() {
        for j in 0..units.len() {
            if i == j
                || units[i].role != Role::Game
                || units[j].role != Role::Game
                || ids[i].is_empty()
            {
                continue;
            }
            if ids[i] == ids[j] && worse(&units[i], &units[j]) {
                units[i].role = Role::Duplicate { of: j };
            }
        }
    }
    // Patches attach to the game whose identity they contain.
    for i in 0..units.len() {
        if !matches!(units[i].role, Role::Patch { .. }) {
            continue;
        }
        let base = (0..units.len()).find(|&j| {
            units[j].role == Role::Game && !ids[j].is_empty() && ids[j].is_subset(&ids[i])
        });
        let ver = |k: usize| version(&units[k].files[0]);
        units[i].role = match base {
            Some(b) if ver(i).is_some_and(|v| Some(v) <= ver(b)) => Role::Included { base: b },
            _ => Role::Patch { base },
        };
    }
    units
}

/// Whether patch `files` belong to an installed `game`: the words naming
/// the game's download, or its title, all appear in the patch's name.
pub fn patch_fits(files: &[PathBuf], game: &crate::library::Game) -> bool {
    let patch = identity(&files[0]);
    let from_source = identity(&game.source);
    let title: BTreeSet<String> = words(&game.title)
        .into_iter()
        .filter(|w| !NOISE.contains(&w.as_str()))
        .collect();
    [from_source, title]
        .iter()
        .any(|ids| !ids.is_empty() && ids.is_subset(&patch))
}

/// Whether `dir` holds several downloads rather than one (a folder of
/// different games, versus `Dies irae/` with its three parts).
pub fn is_collection(dir: &Path) -> bool {
    let Ok(rd) = fs::read_dir(dir) else {
        return false;
    };
    let entries: Vec<PathBuf> = rd.flatten().map(|e| e.path()).collect();
    plan(&entries)
        .iter()
        .filter(|u| u.role == Role::Game)
        .count()
        > 1
}

/// Loser of two copies of the same game.
fn worse(a: &Unit, b: &Unit) -> bool {
    let old = |u: &Unit| words(&file_name(&u.files[0])).iter().any(|w| w == "old");
    match (old(a), old(b)) {
        (true, false) => true,
        (false, true) => false,
        _ => (a.size, &a.files[0]) < (b.size, &b.files[0]),
    }
}

/// The version in a file name: `muramasa_patch_1.04.exe` -> [1, 4].
fn version(path: &Path) -> Option<Vec<u32>> {
    // The last dotted run of numbers, ignoring the file extension.
    let name = file_name(path).to_lowercase();
    let mut best = None;
    for (i, c) in name.char_indices() {
        if !c.is_ascii_digit() || name[..i].ends_with(|p: char| p.is_ascii_digit() || p == '.') {
            continue;
        }
        let run: String = name[i..]
            .chars()
            .take_while(|c| c.is_ascii_digit() || *c == '.')
            .collect();
        let parts: Vec<u32> = run
            .split('.')
            .filter(|p| !p.is_empty())
            .filter_map(|p| p.parse().ok())
            .collect();
        if parts.len() >= 2 {
            best = Some(parts);
        }
    }
    best
}

fn is_patch(files: &[PathBuf]) -> bool {
    let w = words(&file_name(&files[0]));
    if w.iter().any(|w| PATCH_WORDS.contains(&w.as_str())) {
        return true;
    }
    // A lone file that isn't an archive or a program is game data meant to
    // be dropped into an install (a translated script, say).
    files.len() == 1
        && files[0].is_file()
        && archive::sniff(&files[0]).is_none()
        && !has_ext(&files[0], "exe")
        && !has_ext(&files[0], "msi")
}

/// Grouping key: the name without archive/volume suffixes and without a
/// trailing part number (`MG1041_2.zip` -> `mg1041`, `x.7z.002` -> `x`).
fn stem_key(path: &Path) -> String {
    let mut name = file_name(path).to_lowercase();
    // Any plain extension (`.hcb`, `.dat`); numeric ones are volume numbers.
    if let Some((s, ext)) = name.rsplit_once('.')
        && (2..=4).contains(&ext.len())
        && ext.bytes().all(|b| b.is_ascii_alphabetic())
    {
        name = s.to_string();
    }
    loop {
        let before = name.len();
        for suffix in [".zip", ".7z", ".rar", ".exe", ".bin", ".iso", ".msi"] {
            if let Some(s) = name.strip_suffix(suffix) {
                name = s.to_string();
            }
        }
        if let Some((s, ext)) = name.rsplit_once('.')
            && !ext.is_empty()
            && ext
                .trim_start_matches("part")
                .trim_start_matches('r')
                .bytes()
                .all(|b| b.is_ascii_digit())
            && ext != "part"
        {
            name = s.to_string();
        }
        if let Some((s, n)) = name.rsplit_once(['_', '-', ' '])
            && !n.is_empty()
            && n.len() <= 2
            && n.bytes().all(|b| b.is_ascii_digit())
        {
            name = s.to_string();
        }
        if name.len() == before {
            return name;
        }
    }
}

/// Which game a download is, as a set of meaningful words.
pub fn identity(path: &Path) -> BTreeSet<String> {
    words(&stem_key(path))
        .into_iter()
        .filter(|w| !NOISE.contains(&w.as_str()) && !w.bytes().all(|b| b.is_ascii_digit()))
        .filter(|w| {
            !(w.starts_with('v') && w[1..].bytes().all(|b| b.is_ascii_digit() || b == b'.'))
        })
        .collect()
}

fn words(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for c in s.chars() {
        if c.is_alphanumeric() {
            cur.extend(c.to_lowercase());
        } else if !cur.is_empty() {
            out.push(std::mem::take(&mut cur));
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    // Version numbers like 1.13a split into "1", "13a"; drop the pieces.
    out.retain(|w| !(w.len() <= 4 && w.chars().next().is_some_and(|c| c.is_ascii_digit())));
    out
}

fn display_name(path: &Path) -> String {
    let key = stem_key(path);
    let raw = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    // Keep the original casing: cut the file name to the key's length.
    raw.get(..key.len())
        .unwrap_or(&key)
        .replace(['_'], " ")
        .trim()
        .to_string()
}

fn file_name(p: &Path) -> String {
    p.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn has_ext(p: &Path, ext: &str) -> bool {
    p.extension().is_some_and(|e| e.eq_ignore_ascii_case(ext))
}

fn is_junk(p: &Path) -> bool {
    let n = file_name(p);
    n.starts_with('.')
        || n.eq_ignore_ascii_case("desktop.ini")
        || n.eq_ignore_ascii_case("thumbs.db")
}

/// Folders Windows, macOS, and Linux leave on every drive.
fn is_system_dir(p: &Path) -> bool {
    let n = file_name(p).to_lowercase();
    n.starts_with('$')
        || n.starts_with('.')
        || [
            "system volume information",
            "lost+found",
            "__macosx",
            "found.000",
        ]
        .contains(&n.as_str())
}

fn du(p: &Path) -> u64 {
    if p.is_file() {
        return fs::metadata(p).map(|m| m.len()).unwrap_or(0);
    }
    walkdir::WalkDir::new(p)
        .into_iter()
        .flatten()
        .filter_map(|e| e.metadata().ok())
        .filter(|m| m.is_file())
        .map(|m| m.len())
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stems() {
        let k = |s: &str| stem_key(Path::new(s));
        assert_eq!(k("MG1041_2.zip"), "mg1041");
        assert_eq!(
            k("eden-of-grisaia-unrated.7z.002"),
            "eden-of-grisaia-unrated"
        );
        assert_eq!(k("eden-of-grisaia-unrated.exe"), "eden-of-grisaia-unrated");
        assert_eq!(k("SenrenBanka_r18.zip.001"), "senrenbanka_r18");
        assert_eq!(k("setup-1.bin"), "setup");
        assert_eq!(k("Dies Irae.part2.rar"), "dies irae");
        assert_eq!(k("MG1064.zip"), "mg1064");
    }

    #[test]
    fn versions() {
        let v = |s: &str| version(Path::new(s));
        assert_eq!(v("muramasa_patch_1.04.exe"), Some(vec![1, 4]));
        assert_eq!(v("Angelic Chaos update v1.13a.zip"), Some(vec![1, 13]));
        assert_eq!(v("Saya_no_Uta.zip"), None);
    }

    #[test]
    fn identities() {
        let id = |s: &str| {
            identity(Path::new(s))
                .into_iter()
                .collect::<Vec<_>>()
                .join(" ")
        };
        assert_eq!(id("muramasa_installer_1.04.exe"), "muramasa");
        assert_eq!(id("muramasa_patch_1.04.exe"), "muramasa");
        assert_eq!(id("Saya_no_Uta_old.zip"), "saya uta");
        assert_eq!(id("Angelic Chaos update v1.13a.zip"), "angelic chaos");
    }

    fn touch(dir: &Path, name: &str, magic: &[u8]) -> PathBuf {
        let p = dir.join(name);
        fs::write(&p, magic).unwrap();
        p
    }

    #[test]
    fn a_messy_folder() {
        let dir = std::env::temp_dir().join(format!("omavn-batch-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let zip = b"PK\x05\x06\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0";
        let files = vec![
            touch(&dir, "eden-of-grisaia-unrated.exe", b"MZ"),
            touch(
                &dir,
                "eden-of-grisaia-unrated.7z.001",
                b"7z\xbc\xaf\x27\x1c",
            ),
            touch(&dir, "eden-of-grisaia-unrated.7z.002", b"x"),
            touch(&dir, "muramasa_installer_1.04.exe", b"MZ"),
            touch(&dir, "muramasa_patch_1.04.exe", b"MZ"),
            touch(&dir, "Saya_no_Uta_old.zip", zip),
            touch(&dir, "Saya_no_Uta.zip", zip),
            touch(&dir, "Angelic Chaos update v1.13a.zip", zip),
            touch(&dir, "Hoshimemo_HD.hcb", b"s\xdc"),
            touch(&dir, "SenrenBanka_r18.zip.001", zip),
            touch(&dir, "SenrenBanka_r18.zip.002", b"x"),
        ];
        let units = plan(&files);
        let by = |n: &str| {
            units
                .iter()
                .position(|u| u.name == n)
                .unwrap_or_else(|| panic!("{n} in {units:#?}"))
        };
        let eden = &units[by("eden-of-grisaia-unrated")];
        assert_eq!((eden.files.len(), &eden.role), (3, &Role::Game));
        assert_eq!(units[by("SenrenBanka r18")].files.len(), 2);
        let mura = by("muramasa installer");
        assert_eq!(
            units[by("muramasa patch")].role,
            Role::Included { base: mura }
        );
        assert_eq!(
            units[by("Saya no Uta old")].role,
            Role::Duplicate {
                of: by("Saya no Uta")
            }
        );
        assert_eq!(
            units[by("Angelic Chaos update v1.13a")].role,
            Role::Patch { base: None }
        );
        assert_eq!(units[by("Hoshimemo HD")].role, Role::Patch { base: None });
        assert_eq!(units.len(), 8);
        fs::remove_dir_all(dir).unwrap();
    }
}
