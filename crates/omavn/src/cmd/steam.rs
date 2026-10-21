use std::process::ExitCode;

use anyhow::{Result, bail};
use omavn_core::library::{self, Game};
use omavn_core::{Paths, steam};

use crate::ui::Ui;

/// Run `f` with Steam closed: fail if it's running, unless `restart`
/// lets us close it and start it again afterwards.
fn with_steam_closed<T>(ui: &Ui, restart: bool, f: impl FnOnce() -> Result<T>) -> Result<T> {
    let was_running = steam::is_running();
    if was_running {
        if !restart {
            bail!(
                "Steam is running and would undo this when it exits; close it, or pass --restart"
            );
        }
        ui.line("closing Steam...");
        steam::shutdown()?;
    }
    let out = f();
    if was_running {
        steam::start();
    }
    out
}

pub fn add(paths: &Paths, ui: &Ui, ids: Vec<String>, restart: bool) -> Result<ExitCode> {
    let games: Vec<Game> = if ids.is_empty() {
        library::list(paths)?
    } else {
        ids.iter()
            .map(|q| library::find(paths, q))
            .collect::<Result<_>>()?
    };
    if games.is_empty() {
        bail!("no games yet. install one with: omavn install <download>");
    }
    for g in games
        .iter()
        .filter(|g| !omavn_core::art::card(paths, g).exists())
    {
        let _ = omavn_core::art::render(paths, g);
    }
    let exe = std::env::current_exe()?;
    let changes = with_steam_closed(ui, restart, || steam::add(paths, &exe, &games))?;
    ui.result(&changes, || {
        for g in &games {
            ui.success(&format!("{} is in Steam", g.title));
        }
        ui.line("they're under Non-Steam in your library, with art and controller support");
    });
    Ok(ExitCode::SUCCESS)
}

pub fn remove(paths: &Paths, ui: &Ui, ids: Vec<String>, restart: bool) -> Result<ExitCode> {
    let ids: Vec<String> = ids
        .iter()
        .map(|q| {
            library::find(paths, q)
                .map(|g| g.id)
                .or_else(|_| Ok::<_, anyhow::Error>(q.clone()))
        })
        .collect::<Result<_>>()?;
    let removed = with_steam_closed(ui, restart, || steam::remove(&ids))?;
    ui.result(&removed, || {
        if removed.is_empty() {
            ui.line("nothing to remove");
        }
        for id in &removed {
            ui.success(&format!("removed {id} from Steam"));
        }
    });
    Ok(ExitCode::SUCCESS)
}

pub fn status(paths: &Paths, ui: &Ui) -> Result<ExitCode> {
    let listed = steam::listed();
    let games = library::list(paths)?;
    let report: Vec<serde_json::Value> = games
        .iter()
        .map(|g| serde_json::json!({ "id": g.id, "in_steam": listed.contains(&g.id) }))
        .collect();
    ui.result(&report, || {
        if steam::accounts().is_empty() {
            ui.line("no Steam account found");
            return;
        }
        for g in &games {
            ui.check(
                listed.contains(&g.id),
                false,
                &g.id,
                if listed.contains(&g.id) {
                    "in Steam"
                } else {
                    "not added"
                },
            );
        }
        if steam::is_running() {
            ui.line(&ui.dim("Steam is running; changes need --restart"));
        }
    });
    Ok(ExitCode::SUCCESS)
}
