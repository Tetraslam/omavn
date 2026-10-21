use std::io::IsTerminal;
use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result, bail};
use omavn_core::batch::{self, Role, Unit};
use omavn_core::library::{self, Game};
use omavn_core::paths::pretty;
use omavn_core::{Paths, install, patch, runtime};

use crate::cli::InstallArgs;
use crate::ui::Ui;

pub fn install(paths: &Paths, ui: &Ui, json: bool, args: InstallArgs) -> Result<ExitCode> {
    let InstallArgs {
        sources,
        dry_run,
        yes,
        pick,
        id,
        title,
        proton,
        offline,
    } = args;
    let sources = if sources.is_empty() {
        vec![pick_download(pick)?]
    } else {
        sources
    };

    // One folder of many downloads, or several paths: plan them as a batch.
    let units = match sources.as_slice() {
        [dir] if dir.is_dir() && batch::is_collection(dir) => batch::plan(&children(dir)?),
        [_] => Vec::new(),
        many => batch::plan(many),
    };
    let games = units.iter().filter(|u| u.role == Role::Game).count();
    if units.is_empty() || (games == 1 && units.len() == 1) {
        if dry_run {
            ui.result(&units, || ui.line("one game; nothing to plan"));
            return Ok(ExitCode::SUCCESS);
        }
        let files = units.first().map(|u| u.files.clone()).unwrap_or(sources);
        let opts = install::Options {
            source: files[0].clone(),
            extra: files[1..].to_vec(),
            id,
            title,
            force_proton: proton,
            offline,
        };
        return one(paths, ui, json, &opts);
    }
    if id.is_some() || title.is_some() {
        bail!("--id and --title apply to one game; this installs {games}");
    }
    ui.result(&units, || show_plan(paths, ui, &units));
    if dry_run {
        return Ok(ExitCode::SUCCESS);
    }
    if !yes && !json && std::io::stdin().is_terminal() && !ui.confirm_default_yes("go ahead?")? {
        bail!("cancelled");
    }
    run_batch(paths, ui, json, &units, proton, offline)
}

fn one(paths: &Paths, ui: &Ui, json: bool, opts: &install::Options) -> Result<ExitCode> {
    let game = install::install(paths, opts, ui)?;
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

fn children(dir: &PathBuf) -> Result<Vec<PathBuf>> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir)?
        .flatten()
        .map(|e| e.path())
        .collect();
    v.sort();
    Ok(v)
}

/// An installed game that came from these files, so reruns skip it.
fn installed_from(installed: &[Game], unit: &Unit) -> Option<String> {
    let first = std::fs::canonicalize(&unit.files[0]).ok()?;
    installed
        .iter()
        .find(|g| g.source == first)
        .map(|g| g.id.clone())
}

fn show_plan(paths: &Paths, ui: &Ui, units: &[Unit]) {
    let installed = library::list(paths).unwrap_or_default();
    let gb = |b: u64| format!("{:.1} GB", b as f64 / 1e9);
    let games: Vec<&Unit> = units.iter().filter(|u| u.role == Role::Game).collect();
    let total: u64 = games.iter().map(|u| u.size).sum();
    ui.line(&format!(
        "{} games, {} of downloads:",
        games.len(),
        gb(total)
    ));
    for u in units {
        let files = if u.files.len() > 1 {
            format!(", {} files", u.files.len())
        } else {
            String::new()
        };
        let detail = ui.dim(&format!("({}{files})", gb(u.size)));
        match &u.role {
            Role::Game => match installed_from(&installed, u) {
                Some(id) => ui.line(&format!(
                    "  = {}  {}",
                    u.name,
                    ui.dim(&format!("already installed as {id}"))
                )),
                None => ui.line(&format!("  + {}  {detail}", u.name)),
            },
            Role::Patch { base: Some(b) } => ui.line(&format!(
                "    patch: {}, applied to {}  {detail}",
                u.name, units[*b].name
            )),
            Role::Patch { base: None } => {
                match installed.iter().find(|g| batch::patch_fits(&u.files, g)) {
                    Some(g) => ui.line(&format!(
                        "    patch: {}, applied to installed {}  {detail}",
                        u.name, g.id
                    )),
                    None => ui.line(&format!(
                        "  ? {}  {}",
                        u.name,
                        ui.dim("a patch, but its game isn't here; skipped")
                    )),
                }
            }
            Role::Duplicate { of } => ui.line(&format!(
                "  - {}  {}",
                u.name,
                ui.dim(&format!("another copy of {}; skipped", units[*of].name))
            )),
        }
    }
}

#[derive(serde::Serialize)]
struct Outcome {
    download: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<String>,
    status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

fn run_batch(
    paths: &Paths,
    ui: &Ui,
    json: bool,
    units: &[Unit],
    proton: bool,
    offline: bool,
) -> Result<ExitCode> {
    let reporter = ui as &dyn omavn_core::Reporter;
    let installed = library::list(paths)?;
    let mut ids: Vec<Option<String>> = vec![None; units.len()];
    let mut outcomes = Vec::new();

    for (i, u) in units.iter().enumerate() {
        if u.role != Role::Game {
            continue;
        }
        if let Some(id) = installed_from(&installed, u) {
            ids[i] = Some(id.clone());
            outcomes.push(Outcome {
                download: u.name.clone(),
                id: Some(id),
                status: "already installed",
                error: None,
            });
            continue;
        }
        reporter.step(format!("[{}] {}", i + 1, u.name));
        let opts = install::Options {
            source: u.files[0].clone(),
            extra: u.files[1..].to_vec(),
            id: None,
            title: None,
            force_proton: proton,
            offline,
        };
        match install::install(paths, &opts, ui) {
            Ok(g) => {
                ui.finish_progress();
                ui.success(&format!("{} installed as `{}`", g.title, g.id));
                ids[i] = Some(g.id.clone());
                outcomes.push(Outcome {
                    download: u.name.clone(),
                    id: Some(g.id),
                    status: "installed",
                    error: None,
                });
            }
            Err(e) => {
                ui.finish_progress();
                reporter.warn(format!("{} failed: {e:#}", u.name));
                outcomes.push(Outcome {
                    download: u.name.clone(),
                    id: None,
                    status: "failed",
                    error: Some(format!("{e:#}")),
                });
            }
        }
    }

    for u in units {
        let target = match &u.role {
            Role::Patch { base: Some(b) } => ids[*b].clone(),
            Role::Patch { base: None } => library::list(paths)?
                .into_iter()
                .find(|g| batch::patch_fits(&u.files, g))
                .map(|g| g.id),
            Role::Duplicate { .. } => {
                outcomes.push(Outcome {
                    download: u.name.clone(),
                    id: None,
                    status: "skipped (older copy)",
                    error: None,
                });
                continue;
            }
            Role::Game => continue,
        };
        let Some(id) = target else {
            outcomes.push(Outcome {
                download: u.name.clone(),
                id: None,
                status: "skipped (game not installed)",
                error: None,
            });
            continue;
        };
        let mut game = library::load(paths, &id)?;
        reporter.step(format!("patching {} with {}", game.title, u.name));
        let res = patch::apply(paths, &mut game, &u.files, ui);
        ui.finish_progress();
        match res {
            Ok(_) => outcomes.push(Outcome {
                download: u.name.clone(),
                id: Some(id),
                status: "patch applied",
                error: None,
            }),
            Err(e) => {
                reporter.warn(format!("{} failed: {e:#}", u.name));
                outcomes.push(Outcome {
                    download: u.name.clone(),
                    id: Some(id),
                    status: "patch failed",
                    error: Some(format!("{e:#}")),
                });
            }
        }
    }

    let failed = outcomes
        .iter()
        .filter(|o| o.status.ends_with("failed"))
        .count();
    let done = outcomes.iter().filter(|o| o.status == "installed").count();
    if omavn_core::desktop::has_display() && !json && done > 0 {
        omavn_core::omarchy::notify(
            &format!(
                "{done} visual novel{} installed",
                if done == 1 { "" } else { "s" }
            ),
            "Open your library to play",
            None,
            &[],
            None,
        );
    }
    ui.result(&outcomes, || {
        ui.line("");
        let w = outcomes.iter().map(|o| o.download.chars().count()).max().unwrap_or(0).min(40);
        for o in &outcomes {
            let id = o.id.as_deref().map(|i| format!(" → {i}")).unwrap_or_default();
            ui.line(&format!("  {:<w$}  {}{id}", o.download, o.status));
            if let Some(e) = &o.error {
                ui.line(&format!("  {:<w$}  {}", "", ui.dim(e)));
            }
        }
        if failed > 0 {
            ui.line(&ui.dim("\nreport what failed with: omavn feedback --kind game \"<download> fails to install\""));
        }
    });
    Ok(if failed > 0 {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    })
}

pub fn patch(paths: &Paths, ui: &Ui, game: String, files: Vec<PathBuf>) -> Result<ExitCode> {
    let mut game = library::find(paths, &game)?;
    let applied = patch::apply(paths, &mut game, &files, ui)?;
    ui.finish_progress();
    ui.result(&applied, || match &applied {
        patch::Applied::Files {
            copied,
            into,
            backup,
        } => {
            ui.success(&format!("copied {copied} files into {}", pretty(into)));
            if let Some(b) = backup {
                ui.line(&ui.dim(&format!("  replaced files saved in {}", pretty(b))));
            }
        }
        patch::Applied::Program { exe, .. } => ui.success(&format!("ran {exe} on {}", game.title)),
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
