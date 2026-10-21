//! Unwrap a download into a plain directory: peel off zips around rars
//! around SFX volumes, until what is left is a game or an installer.
//!
//! The source is never modified. Staging starts as symlinks to the source,
//! and only archives get materialized (and deleted once extracted).

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::archive;
use crate::detect;
use crate::report::Reporter;

/// Nesting deeper than this is either broken or hostile.
const MAX_ROUNDS: usize = 8;

pub struct Staged {
    /// Temp dir owned by this value; removed on drop.
    pub dir: PathBuf,
}

impl Staged {
    /// The directory inside staging that holds the content.
    pub fn root(&self) -> PathBuf {
        detect::effective_root(&self.dir)
    }
}

impl Drop for Staged {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

pub fn unwrap(source: &Path, staging_root: &Path, reporter: &dyn Reporter) -> Result<Staged> {
    unwrap_all(&[source.to_path_buf()], staging_root, reporter)
}

/// [`unwrap`] for a download made of several files or folders, staged
/// side by side (an extractor stub next to its `.7z.001` and `.7z.002`).
pub fn unwrap_all(
    sources: &[PathBuf],
    staging_root: &Path,
    reporter: &dyn Reporter,
) -> Result<Staged> {
    fs::create_dir_all(staging_root)?;
    let dir = unique_dir(staging_root)?;
    let staged = Staged { dir: dir.clone() };

    for source in sources {
        let source =
            fs::canonicalize(source).with_context(|| format!("{} not found", source.display()))?;
        if source.is_dir() && sources.len() == 1 {
            for entry in fs::read_dir(&source)? {
                let entry = entry?;
                link(&entry.path(), &dir.join(entry.file_name()))?;
            }
        } else {
            link(
                &source,
                &dir.join(source.file_name().context("source has no file name")?),
            )?;
        }
    }

    for _ in 0..MAX_ROUNDS {
        let root = detect::effective_root(&dir);
        unshare(&dir, &root)?;
        let files = top_files(&root);
        let sets = archive::group(&files);
        if sets.is_empty() {
            return Ok(staged);
        }
        // An exe that is not an archive means we reached a game or an
        // installer; archives next to it are its data, not wrappers.
        let stubs: Vec<&PathBuf> = files
            .iter()
            .filter(|f| is_extractor_stub(f, &sets))
            .collect();
        let plain_exe = files.iter().any(|f| {
            f.extension().is_some_and(|e| e.eq_ignore_ascii_case("exe"))
                && !sets.iter().any(|s| s.volumes.contains(f))
                && !stubs.contains(&f)
        });
        if plain_exe {
            return Ok(staged);
        }
        for stub in stubs {
            fs::remove_file(stub)?;
        }
        for set in sets {
            let label = set
                .first()
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            reporter.step(format!("extracting {label}"));
            archive::extract(&set, &root, reporter)?;
            for vol in &set.volumes {
                fs::remove_file(vol)?;
            }
        }
    }
    bail!("archives nested more than {MAX_ROUNDS} levels deep")
}

/// A small exe whose only job is extracting the archive next to it
/// (`game.exe` beside `game.7z.001`), so not the game or an installer.
fn is_extractor_stub(exe: &Path, sets: &[archive::ArchiveSet]) -> bool {
    let stem = |p: &Path| {
        let n = p
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_lowercase();
        n.split('.').next().unwrap_or_default().to_string()
    };
    exe.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("exe"))
        && fs::metadata(exe).is_ok_and(|m| m.len() < 4 << 20)
        && sets
            .iter()
            .any(|s| !s.volumes.iter().any(|v| v == exe) && stem(s.first()) == stem(exe))
}

/// Copy `src` into `dest` (which must not exist), following the staging
/// symlinks. Real files are moved when possible; `copy` reflinks on btrfs/xfs.
pub fn materialize(src: &Path, dest: &Path) -> Result<()> {
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    let meta = fs::symlink_metadata(src)?;
    if !meta.file_type().is_symlink() && fs::rename(src, dest).is_ok() {
        return Ok(());
    }
    let real = fs::canonicalize(src)?;
    if real.is_dir() {
        fs::create_dir_all(dest)?;
        for entry in fs::read_dir(src)? {
            let entry = entry?;
            materialize(&entry.path(), &dest.join(entry.file_name()))?;
        }
    } else {
        fs::copy(&real, dest).with_context(|| format!("copying {}", real.display()))?;
        archive::drop_cache(&real, false);
        archive::drop_cache(dest, true);
    }
    Ok(())
}

/// Make every directory from `base` down to `target` a real directory, so
/// writes there land in staging instead of following a symlink into the
/// user's source folder.
fn unshare(base: &Path, target: &Path) -> Result<()> {
    let rel = target.strip_prefix(base)?;
    let mut cur = base.to_path_buf();
    for comp in rel.components() {
        cur.push(comp);
        if fs::symlink_metadata(&cur)?.file_type().is_symlink() {
            let real = fs::canonicalize(&cur)?;
            fs::remove_file(&cur)?;
            fs::create_dir(&cur)?;
            for entry in fs::read_dir(&real)? {
                let entry = entry?;
                link(&entry.path(), &cur.join(entry.file_name()))?;
            }
        }
    }
    Ok(())
}

fn top_files(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = fs::read_dir(dir)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.is_file())
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v
}

fn link(src: &Path, dest: &Path) -> Result<()> {
    #[cfg(unix)]
    std::os::unix::fs::symlink(src, dest)?;
    #[cfg(not(unix))]
    compile_error!("omavn only supports unix");
    Ok(())
}

fn unique_dir(parent: &Path) -> Result<PathBuf> {
    let base = format!("stage-{}", std::process::id());
    for n in 0.. {
        let p = parent.join(format!("{base}-{n}"));
        if fs::create_dir(&p).is_ok() {
            return Ok(p);
        }
    }
    unreachable!()
}
