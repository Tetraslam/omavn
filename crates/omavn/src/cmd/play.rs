use std::io::IsTerminal;
use std::process::ExitCode;

use anyhow::{Context, Result, bail};
use omavn_core::library::{self, Display};
use omavn_core::{Paths, launch, runtime};

use super::{exit_code, pick_in_terminal, time_now};
use crate::cli::RunArgs;
use crate::ui::Ui;

pub fn run(paths: &Paths, ui: &Ui, json: bool, args: RunArgs) -> Result<ExitCode> {
    let RunArgs {
        game,
        target,
        print,
        detach,
        verbose,
        window,
        gamescope,
        args,
    } = args;
    let mut game = match game {
        Some(g) => library::find(paths, &g)?,
        None => match pick_in_terminal(paths, "Play")? {
            Some(g) => g,
            None => return Ok(ExitCode::SUCCESS),
        },
    };
    let display = match (window, gamescope) {
        (true, _) => Some(Display::Window),
        (_, true) => Some(Display::Gamescope),
        _ => None,
    };
    let opts = launch::Options {
        target,
        args,
        display,
    };
    let spec = launch::game(paths, &game, &opts, ui)?;
    ui.finish_progress();
    if print {
        ui.result(&spec, || ui.line(&shell_line(&spec)));
        return Ok(ExitCode::SUCCESS);
    }
    let mut cmd = spec.command();
    let log = game.log_file(paths);
    if !verbose {
        let file = std::fs::File::create(&log)?;
        cmd.stdout(file.try_clone()?)
            .stderr(file)
            .stdin(std::process::Stdio::null());
    }
    let mut child = cmd
        .spawn()
        .with_context(|| format!("starting {}", spec.program.display()))?;
    game.last_played = Some(time_now());
    let _ = game.save(paths);
    if detach {
        ui.result(
            &serde_json::json!({ "pid": child.id(), "log": log }),
            || ui.success(&format!("started {} (pid {})", game.title, child.id())),
        );
        return Ok(ExitCode::SUCCESS);
    }
    // Launched from a menu or launcher: there is no terminal to show
    // progress, and Proton takes a few seconds to open a window.
    let headless = !std::io::stdout().is_terminal() && !json;
    if headless {
        omavn_core::omarchy::notify(
            &format!("Starting {}", game.title),
            "",
            Some(&omavn_core::art::cover(paths, &game)),
            &[],
            Some(5000),
        );
    } else if !verbose && !json {
        ui.line(&format!("▶ {}", game.title));
    }
    let started = std::time::Instant::now();
    let status = child.wait()?;
    game.playtime_secs += started.elapsed().as_secs();
    let _ = game.save(paths);
    if headless && !status.success() && started.elapsed().as_secs() < 15 {
        let log_s = log.display().to_string();
        omavn_core::omarchy::notify(
            &format!("{} didn't start", game.title),
            &format!(
                "Click for the log. Report it with: omavn feedback --game {}",
                game.id
            ),
            None,
            &["xdg-open", &log_s],
            None,
        );
    }
    if !status.success() && !verbose {
        ui.line(&ui.dim(&format!("exited with {status}; log: {}", log.display())));
    }
    Ok(exit_code(status))
}

/// The graphical library: Omarchy's cover grid. Other desktops don't have
/// one yet, so say so instead of silently falling back to something else.
pub fn gui(paths: &Paths, ui: &Ui) -> Result<ExitCode> {
    if runtime::which("omarchy-menu-images").is_none() {
        bail!(
            "no graphical library on this desktop yet (it uses Omarchy's picker); use `omavn run` to pick in the terminal"
        );
    }
    let games = library::list(paths)?;
    if games.is_empty() {
        bail!("no games yet. install one with: omavn install <download>");
    }
    for g in games
        .iter()
        .filter(|g| !omavn_core::art::card(paths, g).exists())
    {
        let _ = omavn_core::art::render(paths, g);
    }
    let out = std::process::Command::new("omarchy-menu-images")
        .args(["--show-labels", "--filterable", "--print-name"])
        .arg(omavn_core::art::library_dir(paths))
        .output()?;
    let name = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if name.is_empty() {
        return Ok(ExitCode::SUCCESS);
    }
    let picked = omavn_core::art::game_for_entry(paths, &name)
        .with_context(|| format!("no game for library entry `{name}`"))?;
    let _ = ui;
    let exe = std::env::current_exe()?;
    let status = std::process::Command::new(exe)
        .args(["run", &picked])
        .status()?;
    Ok(exit_code(status))
}
fn shell_line(spec: &launch::Spec) -> String {
    let q = |s: &str| {
        if s.chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_./=:".contains(c))
        {
            s.to_string()
        } else {
            format!("'{}'", s.replace('\'', "'\\''"))
        }
    };
    let mut parts = vec![format!("cd {} &&", q(&spec.cwd.display().to_string()))];
    parts.extend(spec.env.iter().map(|(k, v)| format!("{k}={}", q(v))));
    if let Some(a0) = &spec.arg0 {
        parts.push(format!("exec -a {}", q(&a0.display().to_string())));
    }
    parts.push(q(&spec.program.display().to_string()));
    parts.extend(spec.args.iter().map(|a| q(&a.to_string_lossy())));
    parts.join(" ")
}
