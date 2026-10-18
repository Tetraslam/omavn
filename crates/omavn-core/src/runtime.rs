//! Runtimes omavn manages on the user's behalf, uv-style: Proton (to run
//! Windows games), umu-launcher (to run Proton outside Steam), and native
//! Linux Ren'Py builds. Existing installs are reused before downloading.

use std::fs::{self, File};
use std::io::BufReader;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};

use crate::download;
use crate::paths::Paths;
use crate::report::Reporter;

#[derive(Debug, Clone, serde::Serialize)]
pub struct Proton {
    pub name: String,
    pub path: PathBuf,
    pub managed: bool,
}

fn home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default()
}

/// Directories where Proton builds commonly live, managed one first.
fn proton_dirs(paths: &Paths) -> Vec<(PathBuf, bool)> {
    let h = home();
    let mut dirs = vec![(paths.runtimes().join("proton"), true)];
    for d in [
        ".local/share/Steam/compatibilitytools.d",
        ".steam/root/compatibilitytools.d",
        ".var/app/com.valvesoftware.Steam/data/Steam/compatibilitytools.d",
        ".local/share/Steam/steamapps/common",
        ".steam/steam/steamapps/common",
    ] {
        dirs.push((h.join(d), false));
    }
    dirs.push((
        PathBuf::from("/usr/share/steam/compatibilitytools.d"),
        false,
    ));
    dirs
}

pub fn find_protons(paths: &Paths) -> Vec<Proton> {
    let mut found = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for (dir, managed) in proton_dirs(paths) {
        let Ok(rd) = fs::read_dir(&dir) else { continue };
        for entry in rd.flatten() {
            let path = entry.path();
            if !path.join("proton").is_file() {
                continue;
            }
            let Ok(real) = fs::canonicalize(&path) else {
                continue;
            };
            if !seen.insert(real) {
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            found.push(Proton {
                name,
                path,
                managed,
            });
        }
    }
    found.sort_by_key(|p| std::cmp::Reverse(rank(p)));
    found
}

/// GE-Proton first (it ships the media codecs VN cutscenes need), then by version.
fn rank(p: &Proton) -> (bool, Vec<u64>) {
    let nums = p
        .name
        .split(|c: char| !c.is_ascii_digit())
        .filter(|s| !s.is_empty())
        .filter_map(|s| s.parse().ok())
        .collect();
    (p.name.starts_with("GE-Proton"), nums)
}

/// The Proton to use: `OMAVN_PROTON` (name or path), else the best installed
/// one, else download the latest GE-Proton.
pub fn proton(paths: &Paths, reporter: &dyn Reporter) -> Result<Proton> {
    let all = find_protons(paths);
    if let Ok(want) = std::env::var("OMAVN_PROTON") {
        let p = PathBuf::from(&want);
        if p.join("proton").is_file() {
            return Ok(Proton {
                name: want,
                path: p,
                managed: false,
            });
        }
        return all.into_iter().find(|p| p.name == want).with_context(|| {
            format!("OMAVN_PROTON={want} is not installed (see `omavn runtime list`)")
        });
    }
    match all.into_iter().next() {
        Some(p) => Ok(p),
        None => install_ge_proton(paths, reporter),
    }
}

pub fn install_ge_proton(paths: &Paths, reporter: &dyn Reporter) -> Result<Proton> {
    let release = download::github_latest("GloriousEggroll/proton-ge-custom")?;
    let tag = &release.tag_name;
    let dest_root = paths.runtimes().join("proton");
    let dest = dest_root.join(tag);
    if dest.join("proton").is_file() {
        return Ok(Proton {
            name: tag.clone(),
            path: dest,
            managed: true,
        });
    }
    let asset = |suffix: &str| {
        release
            .assets
            .iter()
            .find(|a| a.name.ends_with(suffix) && a.name.contains("x86_64"))
            .with_context(|| format!("{tag} has no {suffix} asset"))
    };
    let tarball = asset(".tar.gz")?;
    let sums = asset(".sha512sum")?;

    reporter.step(format!("downloading {tag}"));
    let file = paths.downloads().join(&tarball.name);
    download::fetch(&tarball.browser_download_url, &file, reporter)?;
    let expected = download::get_string(&sums.browser_download_url)?;
    download::verify_sha512(&file, expected.split_whitespace().next().unwrap_or(""))?;

    reporter.step(format!("unpacking {tag}"));
    fs::create_dir_all(&dest_root)?;
    let tmp = dest_root.join(format!(".{tag}.tmp"));
    let _ = fs::remove_dir_all(&tmp);
    fs::create_dir_all(&tmp)?;
    let gz = flate2::read::GzDecoder::new(BufReader::new(File::open(&file)?));
    tar::Archive::new(gz).unpack(&tmp)?;
    fs::rename(tmp.join(tag), &dest).or_else(|_| fs::rename(&tmp, &dest))?;
    let _ = fs::remove_dir_all(&tmp);
    let _ = fs::remove_file(&file);
    Ok(Proton {
        name: tag.clone(),
        path: dest,
        managed: true,
    })
}

/// Path to `umu-run`, downloading the zipapp release if it is not on PATH.
pub fn umu(paths: &Paths, reporter: &dyn Reporter) -> Result<PathBuf> {
    if let Some(p) = which("umu-run") {
        return Ok(p);
    }
    let managed = paths.runtimes().join("umu/umu/umu-run");
    if managed.is_file() {
        return Ok(managed);
    }
    if which("python3").is_none() {
        bail!("umu-launcher needs python3; install it with your package manager");
    }
    let release = download::github_latest("Open-Wine-Components/umu-launcher")?;
    let asset = release
        .assets
        .iter()
        .find(|a| a.name.ends_with("-zipapp.tar"))
        .context("umu-launcher release has no zipapp")?;
    reporter.step(format!("downloading umu-launcher {}", release.tag_name));
    let file = paths.downloads().join(&asset.name);
    download::fetch(&asset.browser_download_url, &file, reporter)?;
    let dest = paths.runtimes().join("umu");
    fs::create_dir_all(&dest)?;
    tar::Archive::new(File::open(&file)?).unpack(&dest)?;
    let _ = fs::remove_file(&file);
    if !managed.is_file() {
        bail!("umu-run missing from {}", asset.name);
    }
    Ok(managed)
}

/// Directory with the native Linux Ren'Py runtime for `version`
/// (e.g. `.../renpy/8.5.2/py3-linux-x86_64`), fetched from the official SDK.
pub fn renpy(paths: &Paths, version: &str, reporter: &dyn Reporter) -> Result<PathBuf> {
    let platform = renpy_platform(version)?;
    let root = paths.runtimes().join("renpy").join(version);
    let lib = root.join(platform);
    if lib.join("renpy").is_file() {
        return Ok(lib);
    }
    let url = format!("https://www.renpy.org/dl/{version}/renpy-{version}-sdk.tar.bz2");
    reporter.step(format!("fetching Ren'Py {version} (Linux runtime)"));
    let file = paths
        .downloads()
        .join(format!("renpy-{version}-sdk.tar.bz2"));
    // Downloads land via rename, so an existing file is complete.
    if !file.is_file() {
        download::fetch(&url, &file, reporter)?;
    }

    reporter.step(format!("unpacking Ren'Py {version}"));
    let tmp = root.with_file_name(format!(".{version}.tmp"));
    let _ = fs::remove_dir_all(&tmp);
    fs::create_dir_all(&tmp)?;
    let prefix = format!("renpy-{version}-sdk/lib/{platform}/");
    let bz = bzip2::read::MultiBzDecoder::new(BufReader::new(File::open(&file)?));
    let mut archive = tar::Archive::new(bz);
    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.to_string_lossy().into_owned();
        if let Some(rel) = path.strip_prefix(&prefix)
            && !rel.is_empty()
        {
            let out = tmp.join(platform).join(rel);
            if let Some(parent) = out.parent() {
                fs::create_dir_all(parent)?;
            }
            entry.unpack(out)?;
        }
    }
    if !tmp.join(platform).join("renpy").is_file() {
        bail!("Ren'Py {version} SDK has no {platform} runtime");
    }
    let _ = fs::remove_dir_all(&root);
    fs::rename(&tmp, &root)?;
    let _ = fs::remove_file(&file);
    Ok(lib)
}

/// Ren'Py 8 is Python 3; 7.4+ is Python 2 with a `py2-` prefix.
pub fn renpy_platform(version: &str) -> Result<&'static str> {
    let mut it = version.split('.').filter_map(|s| s.parse::<u32>().ok());
    match (it.next(), it.next()) {
        (Some(8..), _) => Ok("py3-linux-x86_64"),
        (Some(7), Some(4..)) => Ok("py2-linux-x86_64"),
        _ => bail!("Ren'Py {version} is too old for a native runtime"),
    }
}

pub fn which(bin: &str) -> Option<PathBuf> {
    std::env::var_os("PATH")
        .into_iter()
        .flat_map(|p| std::env::split_paths(&p).collect::<Vec<_>>())
        .map(|d| d.join(bin))
        .find(|p| p.is_file())
}
