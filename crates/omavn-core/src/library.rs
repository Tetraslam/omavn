//! Installed games. Each lives in `games/<id>/` with a `game.toml` manifest,
//! the game files in `files/`, and (for Proton games) a wine prefix in `prefix/`.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::detect::{Engine, Target};
use crate::paths::Paths;

pub const MANIFEST: &str = "game.toml";

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Runner {
    /// Windows build through umu + Proton.
    Proton,
    /// Windows Ren'Py build run with the matching Linux Ren'Py runtime.
    NativeRenpy,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Game {
    pub id: String,
    pub title: String,
    pub engine: Engine,
    pub runner: Runner,
    /// Game files. Relative paths are relative to the game's directory.
    pub files: PathBuf,
    pub default_target: String,
    pub targets: Vec<Target>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub renpy_version: Option<String>,
    /// Extra environment for every launch.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub catalog_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    pub source: PathBuf,
    #[serde(with = "time::serde::rfc3339")]
    pub installed_at: time::OffsetDateTime,
}

impl Game {
    pub fn dir(&self, paths: &Paths) -> PathBuf {
        paths.game(&self.id)
    }

    pub fn files_dir(&self, paths: &Paths) -> PathBuf {
        self.dir(paths).join(&self.files)
    }

    pub fn prefix(&self, paths: &Paths) -> PathBuf {
        self.dir(paths).join("prefix")
    }

    pub fn target(&self, id: Option<&str>) -> Result<&Target> {
        let id = id.unwrap_or(&self.default_target);
        self.targets.iter().find(|t| t.id == id).with_context(|| {
            let ids: Vec<&str> = self.targets.iter().map(|t| t.id.as_str()).collect();
            format!(
                "{} has no target `{id}` (available: {})",
                self.id,
                ids.join(", ")
            )
        })
    }

    pub fn save(&self, paths: &Paths) -> Result<()> {
        let dir = self.dir(paths);
        fs::create_dir_all(&dir)?;
        let tmp = dir.join(format!("{MANIFEST}.tmp"));
        fs::write(&tmp, toml::to_string_pretty(self)?)?;
        fs::rename(tmp, dir.join(MANIFEST))?;
        Ok(())
    }
}

pub fn load(paths: &Paths, id: &str) -> Result<Game> {
    read(&paths.game(id).join(MANIFEST))
}

fn read(path: &Path) -> Result<Game> {
    let text = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))
}

pub fn list(paths: &Paths) -> Result<Vec<Game>> {
    let mut games = Vec::new();
    let Ok(rd) = fs::read_dir(paths.games()) else {
        return Ok(games);
    };
    for entry in rd.flatten() {
        let manifest = entry.path().join(MANIFEST);
        if manifest.is_file() {
            games.push(read(&manifest)?);
        }
    }
    games.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()));
    Ok(games)
}

/// Resolve what the user typed to one game: exact id, then unique
/// case-insensitive prefix or title substring.
pub fn find(paths: &Paths, query: &str) -> Result<Game> {
    let games = list(paths)?;
    if let Some(g) = games.iter().find(|g| g.id == query) {
        return Ok(g.clone());
    }
    let q = query.to_lowercase();
    let hits: Vec<&Game> = games
        .iter()
        .filter(|g| g.id.starts_with(&q) || g.title.to_lowercase().contains(&q))
        .collect();
    match hits.as_slice() {
        [one] => Ok((*one).clone()),
        [] => bail!("no installed game matches `{query}` (see `omavn list`)"),
        many => {
            let ids: Vec<&str> = many.iter().map(|g| g.id.as_str()).collect();
            bail!("`{query}` is ambiguous: {}", ids.join(", "))
        }
    }
}

/// A fresh id based on `base` that is not taken yet.
pub fn free_id(paths: &Paths, base: &str) -> String {
    if !paths.game(base).exists() {
        return base.to_string();
    }
    (2..)
        .map(|n| format!("{base}-{n}"))
        .find(|id| !paths.game(id).exists())
        .unwrap()
}
