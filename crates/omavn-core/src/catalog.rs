//! Per-title knowledge that heuristics cannot guess: proper titles, which
//! exe is which edition, env tweaks. Entries ship in `catalog/*.toml`;
//! users can add their own in `~/.config/omavn/catalog/`.

use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use walkdir::WalkDir;

use crate::detect::{Engine, Target};
use crate::paths::Paths;

include!(concat!(env!("OUT_DIR"), "/catalog.rs"));

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    pub id: String,
    pub title: String,
    /// VNDB id, pinned when search would pick the wrong entry.
    #[serde(default)]
    pub vndb: Option<String>,
    #[serde(default)]
    pub engine: Option<Engine>,
    #[serde(rename = "match")]
    pub matcher: Match,
    #[serde(default, rename = "target")]
    pub targets: Vec<Target>,
    #[serde(default)]
    pub default_target: Option<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    /// Winetricks verbs the game needs, applied at install (Proton games).
    #[serde(default)]
    pub winetricks: Vec<String>,
    #[serde(default)]
    pub notes: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct Match {
    /// Paths relative to the game root that must all exist (case-insensitive).
    pub files: Vec<String>,
}

pub fn entries(paths: &Paths) -> Result<Vec<Entry>> {
    let mut out = Vec::new();
    for (name, text) in BUILTIN {
        out.push(toml::from_str(text).with_context(|| format!("builtin catalog entry {name}"))?);
    }
    let user = paths.config.join("catalog");
    if let Ok(rd) = fs::read_dir(&user) {
        for e in rd
            .flatten()
            .filter(|e| e.path().extension().is_some_and(|x| x == "toml"))
        {
            let text = fs::read_to_string(e.path())?;
            out.push(toml::from_str(&text).with_context(|| format!("{}", e.path().display()))?);
        }
    }
    Ok(out)
}

/// Most specific entry whose required files are all present under `root`.
pub fn lookup(paths: &Paths, root: &Path) -> Result<Option<Entry>> {
    let present: HashSet<String> = WalkDir::new(root)
        .max_depth(3)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter_map(|e| e.path().strip_prefix(root).ok().map(normalize))
        .collect();
    Ok(entries(paths)?
        .into_iter()
        .filter(|e| !e.matcher.files.is_empty())
        .filter(|e| {
            e.matcher
                .files
                .iter()
                .all(|f| present.contains(&normalize(Path::new(f))))
        })
        .max_by_key(|e| e.matcher.files.len()))
}

fn normalize(p: &Path) -> String {
    let p: PathBuf = p.components().collect();
    p.to_string_lossy().replace('\\', "/").to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_entries_parse() {
        for (name, text) in BUILTIN {
            let e: Entry = toml::from_str(text).unwrap_or_else(|err| panic!("{name}: {err}"));
            assert_eq!(format!("{}.toml", e.id), *name, "file name must match id");
            if let Some(d) = &e.default_target {
                assert!(
                    e.targets.iter().any(|t| &t.id == d),
                    "{name}: unknown default_target"
                );
            }
        }
    }
}
