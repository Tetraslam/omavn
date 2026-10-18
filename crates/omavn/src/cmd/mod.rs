//! One module per group of commands. Each takes what it needs and
//! returns the process exit code.

pub mod doctor;
pub mod feedback;
pub mod inspect;
pub mod install;
pub mod itself;
pub mod library;
pub mod manage;
pub mod omarchy;
pub mod play;
pub mod runtime;

use std::io::IsTerminal;
use std::process::{ExitCode, ExitStatus};

use anyhow::{Result, bail};
use omavn_core::Paths;
use omavn_core::library::{Game, Sort, search};

pub fn exit_code(status: ExitStatus) -> ExitCode {
    ExitCode::from(status.code().unwrap_or(1).clamp(0, 255) as u8)
}

pub fn time_now() -> time::OffsetDateTime {
    time::OffsetDateTime::now_utc()
}

/// "played 3d ago · 12h" / "not played yet".
pub fn played(g: &Game) -> String {
    let Some(last) = g.last_played else {
        return "not played yet".into();
    };
    let ago = (time_now() - last).whole_seconds().max(0) as u64;
    let rel = |s: u64| match s {
        0..60 => "just now".to_string(),
        60..3600 => format!("{}m ago", s / 60),
        3600..86400 => format!("{}h ago", s / 3600),
        _ => format!("{}d ago", s / 86400),
    };
    let total = match g.playtime_secs {
        0 => String::new(),
        s if s < 3600 => format!(" · {}m", s / 60),
        s => format!(" · {}h", s / 3600),
    };
    format!("played {}{total}", rel(ago))
}

/// Fuzzy-pick a game in the terminal. Never opens a window; without a
/// terminal it fails with a pointer to the non-interactive commands.
pub fn pick_in_terminal(paths: &Paths, verb: &str) -> Result<Option<Game>> {
    if !std::io::stdin().is_terminal() || !std::io::stderr().is_terminal() {
        bail!("no game given and no terminal to pick one in; pass one (see `omavn list`)");
    }
    let games = search(paths, "", Sort::Recent)?;
    if games.is_empty() {
        bail!("no games yet. install one with: omavn install <download>");
    }
    let items: Vec<String> = games
        .iter()
        .map(|g| format!("{}  ({})", g.title, g.id))
        .collect();
    let choice = dialoguer::FuzzySelect::new()
        .with_prompt(format!("{verb} which? (type to filter)"))
        .items(&items)
        .default(0)
        .max_length(15)
        .interact_opt()?;
    Ok(choice.map(|i| games[i].clone()))
}

pub fn open_url(url: &str) -> bool {
    std::process::Command::new("xdg-open")
        .arg(url)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}
