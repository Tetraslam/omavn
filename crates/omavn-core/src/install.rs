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
    /// Further files of the same download (other volumes, data files).
    pub extra: Vec<PathBuf>,
    pub id: Option<String>,
    pub title: Option<String>,
    /// Force Proton even when a native runner is available.
    pub force_proton: bool,
    /// Skip VNDB metadata and art.
    pub offline: bool,
}

pub fn install(paths: &Paths, opts: &Options, reporter: &dyn Reporter) -> Result<Game> {
    reporter.step(format!("reading {}", crate::paths::pretty(&opts.source)));
    let mut sources = vec![opts.source.clone()];
    sources.extend(opts.extra.iter().cloned());
    let staged = ingest::unwrap_all(&sources, &paths.staging(), reporter)?;
    let payload = detect::classify(&staged.root())?;

    let source_name = opts
        .source
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "game".into());

    let mut found: Option<crate::vndb::Meta> = None;
    let (id, files_rel) = match &payload {
        Payload::Game { root, engine } => {
            let entry = catalog::lookup(paths, root)?;
            // Look the game up now, so its id comes from its real name.
            if entry.is_none() && opts.id.is_none() && opts.title.is_none() && !opts.offline {
                // The game's own metadata first, then the download's name.
                let mut tries = vec![
                    detect::guess_title(root, *engine, &source_name),
                    detect::clean_name(&source_name),
                    detect::clean_name(
                        &root
                            .file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_default(),
                    ),
                ];
                tries.retain(|t| t != "files");
                tries.dedup();
                found = tries
                    .iter()
                    .filter(|t| !t.is_empty())
                    .find_map(|t| crate::vndb::search(t).ok().flatten());
            }
            let vndb_id = found
                .as_ref()
                .map(|m| detect::slug(m.english_title.as_deref().unwrap_or(&m.title)))
                .filter(|s| s.is_ascii() && s != "game");
            let base = opts
                .id
                .clone()
                .or(entry.map(|e| e.id))
                .or(vndb_id)
                .unwrap_or_else(|| detect::guess_id(root, *engine, &source_name));
            let id = library::free_id(paths, &base);
            reporter.step(format!(
                "installing to {}",
                crate::paths::pretty(&paths.game(&id))
            ));
            ingest::materialize(root, &paths.game(&id).join("files"))?;
            (id, PathBuf::from("files"))
        }
        Payload::Installer { exe, kind } => {
            let base = opts.id.clone().unwrap_or_else(|| {
                detect::guess_id(Path::new(&source_name), Engine::Unknown, &source_name)
            });
            let id = library::free_id(paths, &base);
            let files_rel = run_installer(paths, &id, exe, *kind, reporter)?;
            (id, files_rel)
        }
    };
    drop(staged);

    match finish(paths, opts, &id, files_rel, found, reporter) {
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
    found: Option<crate::vndb::Meta>,
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
            if targets
                .iter()
                .any(|t| detect::is_32bit_exe(&files.join(&t.exe)))
                && !crate::desktop::lib32_vulkan()
            {
                reporter.warn(format!(
                    "this is a 32-bit game and 32-bit Vulkan drivers are missing, so it may show a black screen; {}",
                    crate::desktop::lib32_vulkan_hint()
                ));
            }
            prepare_prefix(paths, id, reporter)?;
        }
    }

    let title = opts
        .title
        .clone()
        .or(entry.as_ref().map(|e| e.title.clone()))
        .or(found
            .as_ref()
            .map(|m| m.english_title.clone().unwrap_or_else(|| m.title.clone())))
        .unwrap_or_else(|| detect::guess_title(&files, engine, id));

    let mut game = Game {
        id: id.to_string(),
        title,
        original_title: None,
        developers: Vec::new(),
        vndb: entry
            .as_ref()
            .and_then(|e| e.vndb.clone())
            .or(found.as_ref().map(|m| m.id.clone())),
        engine: entry.as_ref().and_then(|e| e.engine).unwrap_or(engine),
        runner,
        display: Display::Auto,
        files: files_rel,
        default_target,
        targets,
        renpy_version,
        winetricks: Vec::new(),
        env: entry
            .as_ref()
            .map(|e| e.env.clone())
            .unwrap_or_else(BTreeMap::new),
        catalog_id: entry.as_ref().map(|e| e.id.clone()),
        notes: entry.as_ref().and_then(|e| e.notes.clone()),
        source: fs::canonicalize(&opts.source).unwrap_or_else(|_| opts.source.clone()),
        installed_at: time::OffsetDateTime::now_utc(),
        last_played: None,
        playtime_secs: 0,
    };
    if runner == Runner::Proton
        && !game.env.contains_key("LANG")
        && detect::is_japanese(&files, engine)
    {
        reporter.info(format!(
            "Japanese game: running it with a Japanese locale (undo: omavn set {} locale)",
            game.id
        ));
        for var in ["LANG", "LC_ALL"] {
            game.env.insert(var.into(), JAPANESE_LOCALE.into());
        }
    }
    game.save(paths)?;
    let verbs = entry
        .as_ref()
        .map(|e| e.winetricks.clone())
        .unwrap_or_default();
    if runner == Runner::Proton && !verbs.is_empty() {
        reporter.step(format!("installing {} with winetricks", verbs.join(", ")));
        launch::winetricks(paths, &game, &verbs, reporter)?;
        game.winetricks = verbs;
        game.save(paths)?;
    }
    if !opts.offline {
        reporter.step("fetching metadata and art");
        if let Err(e) = crate::meta::refresh(
            paths,
            &mut game,
            None,
            opts.title.is_none() && entry.is_none() && found.is_none(),
            reporter,
        ) {
            reporter.warn(format!("skipped art: {e}"));
        }
    }
    Ok(game)
}

/// Wine takes its ANSI code page from this; Proton's container generates
/// the locale, so the host doesn't need it.
pub const JAPANESE_LOCALE: &str = "ja_JP.UTF-8";

/// Create the game's Wine prefix now, so the first launch is instant. The
/// very first prefix also makes umu download the Steam runtime.
fn prepare_prefix(paths: &Paths, id: &str, reporter: &dyn Reporter) -> Result<()> {
    let prefix = paths.game(id).join("prefix");
    if prefix.join("drive_c").is_dir() {
        return Ok(());
    }
    reporter.step("preparing the Wine prefix (the first one also downloads the Steam runtime)");
    let spec = launch::proton_spec(
        paths,
        &prefix,
        &library::umu_id(id),
        vec!["wineboot".into(), "-u".into()],
        paths.game(id),
        reporter,
    )?;
    let log = fs::File::create(paths.game(id).join("last-run.log"))?;
    let status = spec
        .command()
        .stdout(log.try_clone()?)
        .stderr(log)
        .status()?;
    if !status.success() {
        reporter.warn(format!(
            "preparing the prefix exited with {status}; it will be retried on first launch"
        ));
    }
    Ok(())
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
    if !crate::desktop::has_display() {
        bail!(
            "this download is a Windows installer, which needs a graphical session; run omavn install from your desktop (not over SSH)"
        );
    }
    let dir = paths.game(id);
    let files = dir.join("files");
    fs::create_dir_all(&files)?;
    let prefix = dir.join("prefix");
    let win_target = format!("Z:{}", files.display()).replace('/', "\\");

    let mut args: Vec<OsString> = vec![exe.as_os_str().to_owned()];
    let silent = kind.silent_args(&win_target);
    if let Some(silent) = &silent {
        args.extend(silent.iter().map(OsString::from));
    }
    let cwd = exe.parent().unwrap_or(Path::new("/")).to_path_buf();
    // Fetches Proton and umu first if needed, so progress reads in order.
    let spec = launch::proton_spec(paths, &prefix, &library::umu_id(id), args, cwd, reporter)?;
    if silent.is_some() {
        reporter.step(format!(
            "running {kind:?} installer unattended (the first one also sets up the Steam runtime)"
        ));
    } else {
        reporter.step(format!(
            "running {kind:?} installer; click through it, any install location works"
        ));
    }
    let started = time::OffsetDateTime::now_utc().unix_timestamp();
    // Proton's chatter goes to a log, not the user's terminal.
    let log = fs::File::create(dir.join("install.log"))?;
    let mut child = spec
        .command()
        .stdout(log.try_clone()?)
        .stderr(log)
        .spawn()
        .context("starting umu-run")?;
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
