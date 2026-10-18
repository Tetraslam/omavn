//! Turn an installed game into a concrete command line.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};

use crate::library::{Game, Runner};
use crate::paths::Paths;
use crate::report::Reporter;
use crate::runtime;

/// Everything needed to start a process. Serializable so frontends and
/// agents can inspect (`omavn run --print`) or run it themselves.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Spec {
    pub program: PathBuf,
    /// Overrides argv[0]; Ren'Py finds its game from it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arg0: Option<PathBuf>,
    pub args: Vec<OsString>,
    pub env: BTreeMap<String, String>,
    pub cwd: PathBuf,
}

impl Spec {
    pub fn command(&self) -> Command {
        let mut cmd = Command::new(&self.program);
        #[cfg(unix)]
        if let Some(arg0) = &self.arg0 {
            use std::os::unix::process::CommandExt;
            cmd.arg0(arg0);
        }
        cmd.args(&self.args).envs(&self.env).current_dir(&self.cwd);
        cmd
    }
}

pub fn game(
    paths: &Paths,
    game: &Game,
    target: Option<&str>,
    extra: &[OsString],
    reporter: &dyn Reporter,
) -> Result<Spec> {
    let target = game.target(target)?;
    let files = game.files_dir(paths);
    let exe = files.join(&target.exe);
    let mut args: Vec<OsString> = target.args.iter().map(OsString::from).collect();
    args.extend(extra.iter().cloned());

    let mut spec = match game.runner {
        Runner::NativeRenpy => {
            let version = game
                .renpy_version
                .as_deref()
                .context("manifest is missing renpy_version")?;
            let lib = runtime::renpy(paths, version, reporter)?;
            let platform = lib.file_name().context("bad runtime path")?;
            let base = target.exe.file_stem().context("target has no file name")?;
            Spec {
                program: lib.join("renpy"),
                arg0: Some(files.join("lib").join(platform).join(base)),
                args,
                env: BTreeMap::new(),
                cwd: files.clone(),
            }
        }
        Runner::Proton => {
            let cwd = exe.parent().unwrap_or(&files).to_path_buf();
            let mut full = vec![exe.into_os_string()];
            full.extend(args);
            proton_spec(paths, &game.prefix(paths), full, cwd, reporter)?
        }
    };
    spec.env.extend(game.env.clone());
    Ok(spec)
}

/// Run an arbitrary Windows program (or wine tool like `winecfg`) in `prefix`.
pub fn proton_spec(
    paths: &Paths,
    prefix: &Path,
    args: Vec<OsString>,
    cwd: PathBuf,
    reporter: &dyn Reporter,
) -> Result<Spec> {
    let proton = runtime::proton(paths, reporter)?;
    let umu = runtime::umu(paths, reporter)?;
    std::fs::create_dir_all(prefix)?;
    let env = BTreeMap::from([
        ("WINEPREFIX".into(), prefix.display().to_string()),
        ("PROTONPATH".into(), proton.path.display().to_string()),
        ("GAMEID".into(), "umu-default".into()),
        ("STORE".into(), "none".into()),
    ]);
    Ok(Spec {
        program: umu,
        arg0: None,
        args,
        env,
        cwd,
    })
}
