//! Apply an update or patch to an installed game.
//!
//! Patches come in two shapes: files to drop into the game folder (an
//! updated exe plus `patch.xp3`, a replacement script), or a patcher
//! program that edits the install itself (Inno Setup, Visual Patch).

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use walkdir::WalkDir;

use crate::detect::{self, InstallerKind};
use crate::ingest;
use crate::launch;
use crate::library::{Game, Runner};
use crate::paths::Paths;
use crate::report::Reporter;

#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "how", rename_all = "snake_case")]
pub enum Applied {
    /// Files copied over the game; replaced originals saved in `backup`.
    Files {
        copied: usize,
        into: PathBuf,
        backup: Option<PathBuf>,
    },
    /// A patcher program ran against the game.
    Program { exe: String, unattended: bool },
}

pub fn apply(
    paths: &Paths,
    game: &mut Game,
    files: &[PathBuf],
    reporter: &dyn Reporter,
) -> Result<Applied> {
    let staged = ingest::unwrap_all(files, &paths.staging(), reporter)?;
    let root = staged.root();
    let game_files = game.files_dir(paths);

    if let Some((exe, kind)) = patcher(&root, &game_files) {
        if game.runner != Runner::Proton {
            bail!(
                "{} is a Windows patcher, but {} runs natively",
                file_name(&exe),
                game.title
            );
        }
        return run_patcher(paths, game, &exe, kind, reporter);
    }
    copy_over(paths, game, &root, &game_files, reporter)
}

/// The patch's program, if the patch is one: an installer framework we
/// recognize, or a lone exe that isn't one of the game's own files.
fn patcher(root: &Path, game_files: &Path) -> Option<(PathBuf, Option<InstallerKind>)> {
    let exes = detect::executables(root);
    if let Some((exe, kind)) = exes.iter().find_map(|e| {
        detect::installer_kind(e)
            .filter(|k| *k != InstallerKind::Unknown)
            .map(|k| (e.clone(), k))
    }) {
        return Some((exe, Some(kind)));
    }
    let game_exes: Vec<String> = detect::executables(game_files)
        .iter()
        .map(|e| lower(&file_name(e)))
        .collect();
    let foreign: Vec<&PathBuf> = exes
        .iter()
        .filter(|e| !game_exes.contains(&lower(&file_name(e))))
        .collect();
    let others = fs::read_dir(root).map(|rd| rd.count()).unwrap_or(0);
    // Just a program (plus maybe a readme): it patches, it isn't patched in.
    match foreign.as_slice() {
        [one] if others <= 3 => Some(((*one).clone(), None)),
        _ => None,
    }
}

fn run_patcher(
    paths: &Paths,
    game: &mut Game,
    exe: &Path,
    kind: Option<InstallerKind>,
    reporter: &dyn Reporter,
) -> Result<Applied> {
    if !crate::desktop::has_display() {
        bail!(
            "this patch is a Windows program, which needs a graphical session; run it from your desktop"
        );
    }
    let files = game.files_dir(paths);
    let target = format!("Z:{}", files.display()).replace('/', "\\");
    let silent = kind.and_then(|k| k.silent_args(&target));
    let mut args: Vec<OsString> = vec![exe.as_os_str().to_owned()];
    match &silent {
        Some(s) => {
            args.extend(s.iter().map(OsString::from));
            reporter.step(format!("running {} unattended", file_name(exe)));
        }
        None => {
            reporter.step(format!(
                "running {}; if it asks where the game is, choose:",
                file_name(exe)
            ));
            reporter.info(target.clone());
        }
    }
    // Patchers often look in their working directory first.
    let spec = launch::proton_spec(
        paths,
        &game.prefix(paths),
        &game.umu_id(),
        args,
        files.clone(),
        reporter,
    )?;
    let log = fs::File::create(game.dir(paths).join("patch.log"))?;
    let status = spec
        .command()
        .stdout(log.try_clone()?)
        .stderr(log)
        .status()?;
    if !status.success() {
        reporter.warn(format!("{} exited with {status}", file_name(exe)));
    }
    Ok(Applied::Program {
        exe: file_name(exe),
        unattended: silent.is_some(),
    })
}

/// Copy the patch's files over the game, into the folder they belong in
/// (where the patch's top-level names already exist), backing up what
/// gets replaced.
fn copy_over(
    paths: &Paths,
    game: &Game,
    root: &Path,
    game_files: &Path,
    reporter: &dyn Reporter,
) -> Result<Applied> {
    let top: Vec<String> = fs::read_dir(root)?
        .flatten()
        .map(|e| lower(&e.file_name().to_string_lossy()))
        .filter(|n| !is_doc(n))
        .collect();
    if top.is_empty() {
        bail!("the patch has no files to apply");
    }
    let into = WalkDir::new(game_files)
        .max_depth(3)
        .into_iter()
        .flatten()
        .filter(|e| e.file_type().is_dir())
        .map(|e| {
            let names: Vec<String> = fs::read_dir(e.path())
                .map(|rd| {
                    rd.flatten()
                        .map(|f| lower(&f.file_name().to_string_lossy()))
                        .collect()
                })
                .unwrap_or_default();
            (
                top.iter().filter(|t| names.contains(t)).count(),
                e.into_path(),
            )
        })
        .max_by(|a, b| {
            a.0.cmp(&b.0)
                .then(b.1.components().count().cmp(&a.1.components().count()))
        })
        .filter(|(hits, _)| *hits > 0)
        .map(|(_, p)| p)
        .unwrap_or_else(|| detect::effective_root(game_files));

    reporter.step(format!(
        "applying the patch to {}",
        crate::paths::pretty(&into)
    ));
    let stamp = time::OffsetDateTime::now_utc().unix_timestamp();
    let backup_root = game.dir(paths).join("backup").join(stamp.to_string());
    let mut copied = 0;
    let mut backed_up = false;
    for entry in WalkDir::new(root).min_depth(1) {
        let entry = entry?;
        let rel = entry.path().strip_prefix(root)?;
        if rel.components().count() == 1 && is_doc(&lower(&rel.to_string_lossy())) {
            continue;
        }
        let dest = case_insensitive(&into, rel);
        if entry.file_type().is_dir() {
            fs::create_dir_all(&dest)?;
            continue;
        }
        if dest.exists() {
            let b = backup_root.join(dest.strip_prefix(&into).unwrap_or(rel));
            fs::create_dir_all(b.parent().unwrap())?;
            fs::copy(&dest, &b).with_context(|| format!("backing up {}", dest.display()))?;
            backed_up = true;
        }
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(fs::canonicalize(entry.path())?, &dest)
            .with_context(|| format!("writing {}", dest.display()))?;
        copied += 1;
    }
    Ok(Applied::Files {
        copied,
        into,
        backup: backed_up.then_some(backup_root),
    })
}

/// `base/rel`, reusing existing entries whose names differ only in case
/// (Windows games don't care, and patches don't match their casing).
fn case_insensitive(base: &Path, rel: &Path) -> PathBuf {
    let mut cur = base.to_path_buf();
    for comp in rel.components() {
        let want = comp.as_os_str().to_string_lossy();
        let existing = fs::read_dir(&cur).ok().and_then(|rd| {
            rd.flatten()
                .map(|e| e.file_name())
                .find(|n| n.to_string_lossy().eq_ignore_ascii_case(&want))
        });
        cur.push(existing.unwrap_or_else(|| comp.as_os_str().to_owned()));
    }
    cur
}

fn is_doc(name: &str) -> bool {
    ["readme", "license", "changelog", "manual"]
        .iter()
        .any(|d| name.starts_with(d))
        || [".txt", ".md", ".pdf", ".html", ".url", ".rtf"]
            .iter()
            .any(|e| name.ends_with(e))
}

fn file_name(p: &Path) -> String {
    p.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn lower(s: &str) -> String {
    s.to_lowercase()
}
