use std::process::ExitCode;

use anyhow::{Context, Result, bail};
use omavn_core::library::{self, Display, Game};
use omavn_core::{Paths, launch, runtime};

use super::{exit_code, open_url, pick_in_terminal};
use crate::cli::{ExecArgs, MetaArgs, RemoveArgs, SavesArgs, SetArgs, WinetricksArgs};
use crate::ui::Ui;

pub fn meta(paths: &Paths, ui: &Ui, args: MetaArgs) -> Result<ExitCode> {
    let MetaArgs { game, vndb, all } = args;
    let mut games = if all {
        library::list(paths)?
    } else {
        let q = game.context("which game? (or --all)")?;
        vec![library::find(paths, &q)?]
    };
    for g in &mut games {
        (ui as &dyn omavn_core::Reporter).step(g.title.clone());
        omavn_core::meta::refresh(paths, g, vndb.as_deref(), ui)?;
    }
    ui.finish_progress();
    ui.result(&games, || {
        ui.success(&format!("refreshed {} game(s)", games.len()))
    });
    Ok(ExitCode::SUCCESS)
}

pub fn set(paths: &Paths, ui: &Ui, args: SetArgs) -> Result<ExitCode> {
    let SetArgs { game, key, value } = args;
    let mut game = library::find(paths, &game)?;
    apply_setting(&mut game, &key, value.as_deref())?;
    game.save(paths)?;
    ui.result(&game, || ui.success(&format!("{}: {key} updated", game.id)));
    Ok(ExitCode::SUCCESS)
}

pub fn remove(paths: &Paths, ui: &Ui, args: RemoveArgs) -> Result<ExitCode> {
    let RemoveArgs { game, pick, yes } = args;
    let game = match game {
        Some(g) => library::find(paths, &g)?,
        None if pick => match pick_game(paths)? {
            Some(g) => g,
            None => return Ok(ExitCode::SUCCESS),
        },
        None => match pick_in_terminal(paths, "Remove")? {
            Some(g) => g,
            None => return Ok(ExitCode::SUCCESS),
        },
    };
    let dir = game.dir(paths);
    if !yes
        && !ui.confirm(&format!(
            "delete {} and its saves ({})?",
            game.title,
            dir.display()
        ))?
    {
        bail!("cancelled (pass --yes to skip this prompt)");
    }
    omavn_core::meta::remove(paths, &game);
    std::fs::remove_dir_all(&dir)?;
    ui.result(&serde_json::json!({ "removed": game.id }), || {
        ui.success(&format!("removed {}", game.id))
    });
    Ok(ExitCode::SUCCESS)
}

pub fn exec(paths: &Paths, ui: &Ui, args: ExecArgs) -> Result<ExitCode> {
    let ExecArgs { game, command } = args;
    let game = library::find(paths, &game)?;
    let files = game.files_dir(paths);
    let spec = launch::proton_spec(
        paths,
        &game.prefix(paths),
        &game.umu_id(),
        command,
        files,
        ui,
    )?;
    ui.finish_progress();
    let status = spec.command().status()?;
    Ok(exit_code(status))
}

pub fn winetricks(paths: &Paths, ui: &Ui, args: WinetricksArgs) -> Result<ExitCode> {
    let WinetricksArgs { game, verbs } = args;
    let mut game = library::find(paths, &game)?;
    if game.runner != library::Runner::Proton {
        bail!(
            "{} runs natively; winetricks only applies to Proton games",
            game.title
        );
    }
    (ui as &dyn omavn_core::Reporter).step(format!("winetricks {}", verbs.join(" ")));
    launch::winetricks(paths, &game, &verbs, ui)?;
    for v in verbs {
        if !game.winetricks.contains(&v) {
            game.winetricks.push(v);
        }
    }
    game.save(paths)?;
    ui.result(&game.winetricks, || {
        ui.success(&format!("installed into {}", game.id))
    });
    Ok(ExitCode::SUCCESS)
}

pub fn saves(paths: &Paths, ui: &Ui, args: SavesArgs) -> Result<ExitCode> {
    let SavesArgs { game, open, import } = args;
    let game = library::find(paths, &game)?;
    if let Some(src) = import {
        let dest = omavn_core::saves::import(paths, &game, &src)?;
        ui.result(&serde_json::json!({ "imported": dest }), || {
            ui.success(&format!("copied saves to {}", dest.display()))
        });
        return Ok(ExitCode::SUCCESS);
    }
    let dirs = omavn_core::saves::find(paths, &game);
    if open && let Some(d) = dirs.first() {
        open_url(&d.display().to_string());
    }
    ui.result(&dirs, || {
        if dirs.is_empty() {
            ui.line("no saves found yet (play a bit, then check again)");
        }
        for d in &dirs {
            ui.line(&d.display().to_string());
        }
    });
    Ok(ExitCode::SUCCESS)
}

fn pick_game(paths: &Paths) -> Result<Option<Game>> {
    let games = library::list(paths)?;
    if runtime::which("omarchy-menu-select").is_none() {
        bail!("pass the game to remove");
    }
    let rows: Vec<String> = games
        .iter()
        .map(|g| format!("󰂺\t{}\t{}", g.title, g.id))
        .collect();
    let out = std::process::Command::new("omarchy-menu-select")
        .arg("Remove visual novel")
        .args(&rows)
        .output()?;
    let sel = String::from_utf8_lossy(&out.stdout);
    let Some(id) = sel.trim().split('\t').nth(1) else {
        return Ok(None);
    };
    Ok(Some(library::load(paths, id)?))
}

fn apply_setting(game: &mut Game, key: &str, value: Option<&str>) -> Result<()> {
    match (key, value) {
        ("display", v) => {
            game.display = match v.unwrap_or("auto") {
                "auto" => Display::Auto,
                "gamescope" => Display::Gamescope,
                "window" => Display::Window,
                other => bail!("display must be auto, gamescope, or window (got `{other}`)"),
            }
        }
        ("target" | "default-target", Some(v)) => {
            game.target(Some(v))?;
            game.default_target = v.to_string();
        }
        ("title", Some(v)) => game.title = v.to_string(),
        // Untranslated Japanese games often need a Japanese locale (Shift-JIS text).
        ("locale", v) => {
            for var in ["LANG", "LC_ALL"] {
                match v {
                    Some(l) => game.env.insert(var.into(), normalize_locale(l)),
                    None => game.env.remove(var),
                };
            }
        }
        (k, v) if k.starts_with("env.") => {
            let name = &k[4..];
            match v {
                Some(v) => game.env.insert(name.to_string(), v.to_string()),
                None => game.env.remove(name),
            };
        }
        (k, None) => bail!("`{k}` needs a value"),
        (k, _) => bail!("unknown setting `{k}` (display, target, title, locale, env.NAME)"),
    }
    Ok(())
}

/// `ja` -> `ja_JP.UTF-8`; full locale names pass through.
fn normalize_locale(l: &str) -> String {
    match l {
        "ja" => omavn_core::install::JAPANESE_LOCALE.into(),
        "zh" | "zh-cn" => "zh_CN.UTF-8".into(),
        "zh-tw" => "zh_TW.UTF-8".into(),
        "ko" => "ko_KR.UTF-8".into(),
        "en" => "en_US.UTF-8".into(),
        other => other.into(),
    }
}
