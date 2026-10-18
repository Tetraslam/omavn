use std::path::PathBuf;

use anyhow::{Context, Result};

/// Where omavn keeps its state. Follows XDG; `OMAVN_HOME` puts everything
/// under one directory instead (handy for tests and portable setups).
#[derive(Debug, Clone, serde::Serialize)]
pub struct Paths {
    pub data: PathBuf,
    pub cache: PathBuf,
    pub config: PathBuf,
}

impl Paths {
    pub fn from_env() -> Result<Self> {
        if let Some(home) = std::env::var_os("OMAVN_HOME") {
            let home = PathBuf::from(home);
            return Ok(Self {
                data: home.join("data"),
                cache: home.join("cache"),
                config: home.join("config"),
            });
        }
        let home = PathBuf::from(std::env::var_os("HOME").context("HOME is not set")?);
        let xdg = |var: &str, fallback: &str| {
            std::env::var_os(var)
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join(fallback))
                .join("omavn")
        };
        Ok(Self {
            data: xdg("XDG_DATA_HOME", ".local/share"),
            cache: xdg("XDG_CACHE_HOME", ".cache"),
            config: xdg("XDG_CONFIG_HOME", ".config"),
        })
    }

    pub fn games(&self) -> PathBuf {
        self.data.join("games")
    }

    pub fn game(&self, id: &str) -> PathBuf {
        self.games().join(id)
    }

    pub fn runtimes(&self) -> PathBuf {
        self.data.join("runtimes")
    }

    /// Scratch space for unpacking. Lives next to `games/` so finished
    /// installs can be moved into place with a rename instead of a copy.
    pub fn staging(&self) -> PathBuf {
        self.data.join("tmp")
    }

    pub fn downloads(&self) -> PathBuf {
        self.cache.join("downloads")
    }

    pub fn config_file(&self) -> PathBuf {
        self.config.join("config.toml")
    }
}
