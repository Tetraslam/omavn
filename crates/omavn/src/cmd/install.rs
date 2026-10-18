use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result, bail};
use omavn_core::{Paths, install, runtime};

use crate::cli::InstallArgs;
use crate::ui::Ui;

pub fn install(paths: &Paths, ui: &Ui, json: bool, args: InstallArgs) -> Result<ExitCode> {
    let InstallArgs {
        source,
        pick,
        id,
        title,
        proton,
        offline,
    } = args;
    let source = match source {
        Some(s) => s,
        None => pick_download(pick)?,
    };
    let opts = install::Options {
        source,
        id,
        title,
        force_proton: proton,
        offline,
    };
    let game = install::install(paths, &opts, ui)?;
    ui.finish_progress();
    if omavn_core::desktop::has_display() && !json {
        let exe = std::env::current_exe()?.display().to_string();
        omavn_core::omarchy::notify(
            &format!("{} is ready", game.title),
            "Click to play",
            Some(&omavn_core::art::cover(paths, &game)),
            &[&exe, "run", &game.id],
            None,
        );
    }
    ui.result(&game, || {
        ui.success(&format!("installed {} as `{}`", game.title, game.id));
        ui.line(&format!("  play it: omavn run {}", game.id));
        if game.targets.len() > 1 {
            ui.line(&format!(
                "  {} entry points, see: omavn info {}",
                game.targets.len(),
                game.id
            ));
        }
    });
    Ok(ExitCode::SUCCESS)
}

/// Ask for a download with Omarchy's file menu. A file inside a subfolder
/// (e.g. `Dies irae/MG1041_1.zip`) means the whole folder: multi-part
/// downloads and installers with data files come as folders.
fn pick_download(pick: bool) -> Result<PathBuf> {
    if !pick || runtime::which("omarchy-menu-file").is_none() {
        bail!("pass the download to install, e.g. omavn install ~/Downloads/game.zip");
    }
    let home = PathBuf::from(std::env::var_os("HOME").context("HOME is not set")?);
    let root = std::env::var_os("XDG_DOWNLOAD_DIR")
        .map(PathBuf::from)
        .unwrap_or(home.join("Downloads"));
    let out = std::process::Command::new("omarchy-menu-file")
        .arg("Visual novel download")
        .arg(&root)
        .arg("zip rar 7z exe 001 msi iso")
        .output()?;
    let picked = PathBuf::from(String::from_utf8_lossy(&out.stdout).trim());
    if picked.as_os_str().is_empty() {
        bail!("nothing picked");
    }
    match picked.parent() {
        Some(dir) if dir != root => Ok(dir.to_path_buf()),
        _ => Ok(picked),
    }
}
