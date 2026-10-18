//! From "whatever the user downloaded" to an entry in the library.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use walkdir::WalkDir;

use crate::catalog;
use crate::detect::{self, Engine, InstallerKind, Payload};
use crate::ingest;
use crate::launch;
use crate::library::{self, Display, Game, Runner};
use crate::paths::Paths;
use crate::report::Reporter;
use crate::runtime;

#[derive(Debug, Clone, Default)]
pub struct Options {
    pub source: PathBuf,
    pub id: Option<String>,
    pub title: Option<String>,
    /// Force Proton even when a native runner is available.
    pub force_proton: bool,
    /// Skip VNDB metadata and art.
    pub offline: bool,
}

pub fn install(paths: &Paths, opts: &Options, reporter: &dyn Reporter) -> Result<Game> {
    reporter.step(format!("reading {}", opts.source.display()));
    let staged = ingest::unwrap(&opts.source, &paths.staging(), reporter)?;
    let payload = detect::classify(&staged.root())?;

    let source_name = opts
        .source
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "game".into());

    let (id, files_rel) = match &payload {
        Payload::Game { root, .. } => {
            let entry = catalog::lookup(paths, root)?;
            let base = opts.id.clone().or(entry.map(|e| e.id)).unwrap_or_else(|| {
                let dir = root.file_name().map(|n| n.to_string_lossy().into_owned());
                detect::slug(
                    &dir.filter(|_| root != &staged.dir)
                        .unwrap_or(source_name.clone()),
                )
            });
            let id = library::free_id(paths, &base);
            reporter.step(format!("installing to {}", paths.game(&id).display()));
            ingest::materialize(root, &paths.game(&id).join("files"))?;
            (id, PathBuf::from("files"))
        }
        Payload::Installer { exe, kind } => {
            let base = opts
                .id
                .clone()
                .unwrap_or_else(|| detect::slug(&source_name));
            let id = library::free_id(paths, &base);
            let files_rel = run_installer(paths, &id, exe, *kind, reporter)?;
            (id, files_rel)
        }
    };
    drop(staged);

    match finish(paths, opts, &id, files_rel, reporter) {
        Ok(game) => Ok(game),
        Err(e) => {
            reporter.warn(format!(
                "install failed, removing {}",
                paths.game(&id).display()
            ));
            let _ = fs::remove_dir_all(paths.game(&id));
            Err(e)
        }
    }
}

fn finish(
    paths: &Paths,
    opts: &Options,
    id: &str,
    files_rel: PathBuf,
    reporter: &dyn Reporter,
) -> Result<Game> {
    let dir = paths.game(id);
    let files = dir.join(&files_rel);
    let engine = detect::engine(&files);
    let entry = catalog::lookup(paths, &files)?;

    let mut targets = detect::targets(&files, engine);
    if let Some(e) = &entry
        && !e.targets.is_empty()
    {
        targets = e.targets.clone();
    }
    if targets.is_empty() {
        bail!("found no executable to launch in {}", files.display());
    }
    let default_target = entry
        .as_ref()
        .and_then(|e| e.default_target.clone())
        .unwrap_or_else(|| targets[0].id.clone());

    let renpy_version = (engine == Engine::Renpy)
        .then(|| detect::renpy_version(&files))
        .flatten();
    let native = !opts.force_proton
        && renpy_version
            .as_deref()
            .is_some_and(|v| runtime::renpy_platform(v).is_ok());
    let runner = if native {
        Runner::NativeRenpy
    } else {
        Runner::Proton
    };

    // Fetch runtimes now so the first `omavn run` starts instantly (and offline).
    match runner {
        Runner::NativeRenpy => {
            runtime::renpy(paths, renpy_version.as_deref().unwrap(), reporter)?;
        }
        Runner::Proton => {
            runtime::proton(paths, reporter)?;
            runtime::umu(paths, reporter)?;
        }
    }

    let title = opts
        .title
        .clone()
        .or(entry.as_ref().map(|e| e.title.clone()))
        .unwrap_or_else(|| title_from(&files, id));

    let mut game = Game {
        id: id.to_string(),
        title,
        original_title: None,
        vndb: entry.as_ref().and_then(|e| e.vndb.clone()),
        engine: entry.as_ref().and_then(|e| e.engine).unwrap_or(engine),
        runner,
        display: Display::Auto,
        files: files_rel,
        default_target,
        targets,
        renpy_version,
        env: entry
            .as_ref()
            .map(|e| e.env.clone())
            .unwrap_or_else(BTreeMap::new),
        catalog_id: entry.as_ref().map(|e| e.id.clone()),
        notes: entry.and_then(|e| e.notes),
        source: fs::canonicalize(&opts.source).unwrap_or_else(|_| opts.source.clone()),
        installed_at: time::OffsetDateTime::now_utc(),
    };
    game.save(paths)?;
    if !opts.offline {
        reporter.step("fetching metadata and art");
        if let Err(e) = crate::meta::refresh(paths, &mut game, None, reporter) {
            reporter.warn(format!("skipped art: {e}"));
        }
    }
    Ok(game)
}

fn title_from(files: &Path, id: &str) -> String {
    files
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .filter(|n| n != "files")
        .unwrap_or_else(|| id.replace('-', " "))
}

/// Run a Windows installer into `games/<id>/files`. Returns the game files
/// directory relative to the game dir (installers like to add a subfolder).
fn run_installer(
    paths: &Paths,
    id: &str,
    exe: &Path,
    kind: InstallerKind,
    reporter: &dyn Reporter,
) -> Result<PathBuf> {
    let dir = paths.game(id);
    let files = dir.join("files");
    fs::create_dir_all(&files)?;
    let prefix = dir.join("prefix");
    let win_target = format!("Z:{}", files.display()).replace('/', "\\");

    let mut args: Vec<OsString> = vec![exe.as_os_str().to_owned()];
    match kind.silent_args(&win_target) {
        Some(silent) => {
            reporter.step(format!("running {kind:?} installer unattended (first run sets up Proton, this can take a minute)"));
            args.extend(silent.into_iter().map(OsString::from));
        }
        None => {
            reporter.step(format!(
                "running {kind:?} installer; click through it, any install location works"
            ));
        }
    }
    let cwd = exe.parent().unwrap_or(Path::new("/")).to_path_buf();
    let spec = launch::proton_spec(paths, &prefix, &library::umu_id(id), args, cwd, reporter)?;
    let started = time::OffsetDateTime::now_utc().unix_timestamp();
    let mut child = spec.command().spawn().context("starting umu-run")?;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        for name in kill_redists(&prefix) {
            reporter.info(format!("skipped {name} (Proton already provides it)"));
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
    };
    if has_game_exe(&files) {
        let root = detect::effective_root(&files);
        return Ok(root.strip_prefix(&dir)?.to_path_buf());
    }
    // The installer ignored our target; look for what it put in the prefix.
    let drive_c = prefix.join("drive_c");
    let found = WalkDir::new(&drive_c)
        .max_depth(5)
        .into_iter()
        .filter_entry(|e| {
            let n = e.file_name().to_string_lossy().to_lowercase();
            n != "windows" && n != "users" && n != "programdata"
        })
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter(|e| {
            e.path()
                .extension()
                .is_some_and(|x| x.eq_ignore_ascii_case("exe"))
        })
        .filter(|e| !detect::is_helper(e.path()))
        // Installers restore archived mtimes, so use ctime to spot new files.
        .filter(|e| e.metadata().is_ok_and(|m| m.ctime() >= started))
        .map(|e| e.into_path())
        .next();
    match found {
        Some(exe) => {
            let root = exe.parent().unwrap();
            let _ = fs::remove_dir(&files);
            Ok(root.strip_prefix(&dir)?.to_path_buf())
        }
        None => bail!("the installer finished ({status}) but no game files appeared"),
    }
}

/// Redistributable installers that installers chain into. They pop up
/// wizards even during "silent" installs, and Proton already ships what
/// they install, so they only block.
const REDISTS: &[&str] = &["dxsetup.exe", "dxwebsetup.exe"];

/// Kill redist installers running in `prefix`. Returns what was killed.
fn kill_redists(prefix: &Path) -> Vec<String> {
    let needle = prefix.as_os_str().as_encoded_bytes();
    let mut killed = Vec::new();
    let Ok(rd) = fs::read_dir("/proc") else {
        return killed;
    };
    for entry in rd.flatten() {
        let Ok(pid) = entry.file_name().to_string_lossy().parse::<i32>() else {
            continue;
        };
        let Ok(cmdline) = fs::read(entry.path().join("cmdline")) else {
            continue;
        };
        let argv0 = cmdline.split(|b| *b == 0).next().unwrap_or_default();
        let argv0 = String::from_utf8_lossy(argv0)
            .replace('\\', "/")
            .to_lowercase();
        let name = argv0.rsplit('/').next().unwrap_or_default().to_string();
        if !REDISTS.contains(&name.as_str()) {
            continue;
        }
        let in_prefix = fs::read(entry.path().join("environ"))
            .is_ok_and(|env| memchr::memmem::find(&env, needle).is_some());
        if in_prefix
            && Command::new("kill")
                .arg("-9")
                .arg(pid.to_string())
                .status()
                .is_ok_and(|s| s.success())
        {
            killed.push(name);
        }
    }
    killed
}

fn has_game_exe(dir: &Path) -> bool {
    detect::executables(dir)
        .iter()
        .any(|e| !detect::is_helper(e))
}
