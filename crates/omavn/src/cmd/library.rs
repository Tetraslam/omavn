use std::process::ExitCode;

use anyhow::Result;
use omavn_core::Paths;
use omavn_core::library::{self, Game};

use super::played;
use crate::cli::{ListArgs, SortBy};
use crate::ui::Ui;
use omavn_core::paths::pretty;

/// `omavn` on its own: a summary that stays short with a big library.
pub fn home(paths: &Paths, ui: &Ui, json: bool) -> Result<ExitCode> {
    let games = library::search(paths, "", library::Sort::Recent)?;
    if json {
        let recent: Vec<&Game> = games.iter().take(5).collect();
        ui.result(&serde_json::json!({ "version": env!("CARGO_PKG_VERSION"), "games": games.len(), "recent": recent }), || {});
        return Ok(ExitCode::SUCCESS);
    }
    ui.line(&format!(
        "omavn {}: play visual novels on Linux\n",
        env!("CARGO_PKG_VERSION")
    ));
    if games.is_empty() {
        ui.line("no games yet. point omavn at a download, exactly as you got it:\n");
        ui.line("  omavn install ~/Downloads/your-vn.zip");
        ui.line("  omavn install ~/Downloads/\"A Folder Of Parts\"");
        if omavn_core::omarchy::detected() && !omavn_core::omarchy::status().menu {
            ui.line("\non Omarchy? omavn omarchy setup adds Visual Novels to your menu");
        }
    } else {
        let n = games.len();
        ui.line(&format!(
            "{n} game{}{}",
            if n == 1 { "" } else { "s" },
            if n > 5 { ", recently played:" } else { ":" }
        ));
        let w = games
            .iter()
            .take(5)
            .map(|g| g.id.chars().count())
            .max()
            .unwrap_or(0)
            .min(32);
        for g in games.iter().take(5) {
            ui.line(&format!(
                "  {:<w$}  {}  {}",
                g.id,
                g.title,
                ui.dim(&played(g))
            ));
        }
        ui.line("\nplay:  omavn run [game]    find:  omavn list <words>    add:  omavn install <download>");
    }
    ui.line(&ui.dim("all commands: omavn --help    docs: https://omavn.dev"));
    Ok(ExitCode::SUCCESS)
}
pub fn list(paths: &Paths, ui: &Ui, args: ListArgs) -> Result<ExitCode> {
    let ListArgs { query, sort, limit } = args;
    let sort = match sort {
        SortBy::Recent => library::Sort::Recent,
        SortBy::Title => library::Sort::Title,
        SortBy::Installed => library::Sort::Installed,
    };
    let mut games = library::search(paths, &query.join(" "), sort)?;
    let total = games.len();
    if let Some(n) = limit {
        games.truncate(n);
    }
    ui.result(&games, || {
        if total == 0 {
            if query.is_empty() {
                ui.line("no games yet. install one with: omavn install <download>");
            } else {
                ui.line(&format!("no games match `{}`", query.join(" ")));
            }
            return;
        }
        let w = games
            .iter()
            .map(|g| g.id.chars().count())
            .max()
            .unwrap_or(0)
            .min(32);
        for g in &games {
            ui.line(&format!(
                "{:<w$}  {}  {}",
                g.id,
                g.title,
                ui.dim(&played(g))
            ));
        }
        if games.len() < total {
            ui.line(&ui.dim(&format!(
                "{} of {total}; see all with --limit {total}",
                games.len()
            )));
        }
    });
    Ok(ExitCode::SUCCESS)
}

pub fn info(paths: &Paths, ui: &Ui, game: String) -> Result<ExitCode> {
    let game = library::find(paths, &game)?;
    let dir = game.dir(paths);
    let info = serde_json::json!({
        "game": &game,
        "dir": &dir,
        "files": game.files_dir(paths),
        "prefix": game.prefix(paths),
    });
    ui.result(&info, || {
        ui.line(&format!("{} ({})", game.title, game.id));
        ui.line(&format!("  engine   {}", game.engine.name()));
        ui.line(&format!("  runner   {:?}", game.runner));
        ui.line(&format!("  files    {}", pretty(&game.files_dir(paths))));
        if game.prefix(paths).exists() {
            ui.line(&format!("  prefix   {}", pretty(&game.prefix(paths))));
        }
        ui.line("  targets");
        for t in &game.targets {
            let mark = if t.id == game.default_target {
                "*"
            } else {
                " "
            };
            ui.line(&format!(
                "   {mark} {:<14} {}  {}",
                t.id,
                t.name,
                ui.dim(&t.exe.display().to_string())
            ));
        }
        if let Some(n) = &game.notes {
            ui.line(&format!("  notes    {n}"));
        }
    });
    Ok(ExitCode::SUCCESS)
}
