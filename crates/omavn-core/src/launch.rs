//! Turn an installed game into a concrete command line.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};

use crate::library::{Display, Game, Runner};
use crate::paths::Paths;
use crate::report::Reporter;
use crate::{desktop, runtime};

/// Everything needed to start a process. Serializable so frontends and
/// agents can inspect (`omavn run --print`) or run it themselves.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Spec {
    pub program: PathBuf,
    /// Overrides argv[0]; Ren'Py finds its game from it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arg0: Option<PathBuf>,
    #[serde(serialize_with = "lossy_strings")]
    pub args: Vec<OsString>,
    pub env: BTreeMap<String, String>,
    pub cwd: PathBuf,
}

fn lossy_strings<S: serde::Serializer>(args: &[OsString], s: S) -> Result<S::Ok, S::Error> {
    s.collect_seq(args.iter().map(|a| a.to_string_lossy()))
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

#[derive(Debug, Clone, Default)]
pub struct Options {
    /// Entry point id; the game's default when `None`.
    pub target: Option<String>,
    /// Extra arguments for the game.
    pub args: Vec<OsString>,
    /// Overrides the game's display setting.
    pub display: Option<Display>,
}

pub fn game(paths: &Paths, game: &Game, opts: &Options, reporter: &dyn Reporter) -> Result<Spec> {
    let target = game.target(opts.target.as_deref())?;
    let files = game.files_dir(paths);
    let exe = files.join(&target.exe);
    let mut args: Vec<OsString> = target.args.iter().map(OsString::from).collect();
    args.extend(opts.args.iter().cloned());

    let spec = match game.runner {
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
            proton_spec(
                paths,
                &game.prefix(paths),
                &game.umu_id(),
                full,
                cwd,
                reporter,
            )?
        }
    };
    // Both runners: tiling window managers resize fixed-size game windows,
    // which stretches Windows games and desyncs Ren'Py's drawing from its
    // input. gamescope keeps the game at its own size and scales it.
    let mut spec = match resolve_display(opts.display.unwrap_or(game.display)) {
        Display::Gamescope => gamescope(spec)?,
        _ => spec,
    };
    spec.env.extend(game.env.clone());
    Ok(spec)
}

/// `Auto` becomes gamescope when it is installed and there is a display to
/// nest in. `OMAVN_DISPLAY=window|gamescope` overrides everything.
fn resolve_display(display: Display) -> Display {
    match std::env::var("OMAVN_DISPLAY").as_deref() {
        Ok("window") => return Display::Window,
        Ok("gamescope") => return Display::Gamescope,
        _ => {}
    }
    match display {
        Display::Auto if runtime::which("gamescope").is_some() && desktop::has_display() => {
            Display::Gamescope
        }
        Display::Auto => Display::Window,
        other => other,
    }
}

/// Wrap `inner` in a fullscreen gamescope at the focused monitor's native
/// resolution, scaled to fit with the aspect ratio kept.
fn gamescope(inner: Spec) -> Result<Spec> {
    let program = runtime::which("gamescope").context("gamescope is not installed")?;
    let mut args: Vec<OsString> = Vec::new();
    if let Some((w, h)) = desktop::focused_output_size() {
        for (flag, v) in [("-W", w), ("-H", h)] {
            args.push(flag.into());
            args.push(v.to_string().into());
        }
    }
    // --force-composition: direct scanout skews software-rendered windows
    // whose width isn't aligned (KiriKiri 2 at 964 px draws as diagonal
    // stripes). Compositing every frame costs nothing for a VN.
    for a in ["-f", "-S", "fit", "--force-composition", "--"] {
        args.push(a.into());
    }
    match inner.arg0 {
        // gamescope execs its child with a plain argv; keep the override
        // through bash's `exec -a`.
        Some(arg0) => {
            let bash = runtime::which("bash").context("bash is not installed")?;
            args.push(bash.into_os_string());
            args.push("-c".into());
            args.push("exec -a \"$0\" \"$@\"".into());
            args.push(arg0.into_os_string());
            args.push(inner.program.into_os_string());
        }
        None => args.push(inner.program.into_os_string()),
    }
    args.extend(inner.args);
    Ok(Spec {
        program,
        arg0: None,
        args,
        env: inner.env,
        cwd: inner.cwd,
    })
}

/// Run an arbitrary Windows program (or wine tool like `winecfg`) in `prefix`.
pub fn proton_spec(
    paths: &Paths,
    prefix: &Path,
    umu_id: &str,
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
        ("GAMEID".into(), umu_id.into()),
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

/// Install winetricks verbs (e.g. `quartz`, `wmp9`) into a game's prefix.
pub fn winetricks(
    paths: &Paths,
    game: &Game,
    verbs: &[String],
    reporter: &dyn Reporter,
) -> Result<()> {
    let mut args: Vec<OsString> = vec!["winetricks".into(), "-q".into()];
    args.extend(verbs.iter().map(OsString::from));
    let spec = proton_spec(
        paths,
        &game.prefix(paths),
        &game.umu_id(),
        args,
        game.dir(paths),
        reporter,
    )?;
    let log = std::fs::File::create(game.dir(paths).join("winetricks.log"))?;
    let status = spec
        .command()
        .stdout(log.try_clone()?)
        .stderr(log)
        .status()?;
    if !status.success() {
        anyhow::bail!(
            "winetricks {} failed ({status}); see {}",
            verbs.join(" "),
            game.dir(paths).join("winetricks.log").display()
        );
    }
    Ok(())
}
