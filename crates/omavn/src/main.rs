mod ui;

use std::ffi::OsString;
use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand, ValueEnum};
use omavn_core::{Paths, detect, feedback, install, launch, library, runtime};

use crate::ui::Ui;

/// Play visual novels on Linux. Point omavn at whatever you downloaded
/// (zips, split rars, installers, plain folders) and it handles the rest.
#[derive(Parser)]
#[command(name = "omavn", version, propagate_version = true)]
struct Cli {
    /// Machine-readable output: results as JSON on stdout, progress as JSON lines on stderr.
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Install a game from a download (file or folder).
    #[command(visible_alias = "add")]
    Install {
        /// The archive, installer, or folder you downloaded.
        source: PathBuf,
        /// Library id (defaults to a slug of the title).
        #[arg(long)]
        id: Option<String>,
        /// Display title.
        #[arg(long)]
        title: Option<String>,
        /// Run through Proton even if a native runtime is available.
        #[arg(long)]
        proton: bool,
    },
    /// Launch an installed game.
    #[command(visible_alias = "play")]
    Run {
        /// Game id or part of its title.
        game: String,
        /// Which entry point (see `omavn info`), e.g. a config tool or another edition.
        #[arg(short, long)]
        target: Option<String>,
        /// Print the command instead of running it.
        #[arg(long)]
        print: bool,
        /// Return immediately instead of waiting for the game to exit.
        #[arg(short, long)]
        detach: bool,
        /// Extra arguments passed to the game.
        #[arg(last = true)]
        args: Vec<OsString>,
    },
    /// List installed games.
    #[command(visible_alias = "ls")]
    List,
    /// Show details about an installed game.
    Info { game: String },
    /// Uninstall a game, including its saves.
    #[command(visible_alias = "rm")]
    Remove {
        game: String,
        /// Don't ask for confirmation.
        #[arg(short, long)]
        yes: bool,
    },
    /// Run a program inside a game's Wine prefix (e.g. `winecfg`, or another exe).
    Exec {
        game: String,
        #[arg(last = true, required = true)]
        command: Vec<OsString>,
    },
    /// Look at a download or folder and report what omavn would do, without installing.
    Inspect { source: PathBuf },
    /// Manage Proton, umu-launcher, and Ren'Py runtimes.
    Runtime {
        #[command(subcommand)]
        command: RuntimeCmd,
    },
    /// Check that this machine can run games.
    Doctor,
    /// Report a bug, a game that doesn't work, or an idea. Opens a prefilled GitHub issue.
    Feedback {
        /// What happened, or what you'd like.
        message: Vec<String>,
        #[arg(short, long, value_enum, default_value = "bug")]
        kind: FeedbackKind,
        /// Attach details about this installed game.
        #[arg(short, long)]
        game: Option<String>,
        /// Only print the link, don't open a browser.
        #[arg(long)]
        no_open: bool,
    },
}

#[derive(Subcommand)]
enum RuntimeCmd {
    /// Show available runtimes.
    #[command(visible_alias = "ls")]
    List,
    /// Download a runtime.
    Install {
        #[arg(value_enum)]
        what: RuntimeKind,
        /// Ren'Py version (for `renpy`).
        version: Option<String>,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum RuntimeKind {
    /// Latest GE-Proton.
    Proton,
    Umu,
    Renpy,
}

#[derive(Clone, Copy, ValueEnum)]
enum FeedbackKind {
    Bug,
    Game,
    Idea,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let ui = Ui::new(cli.json);
    match run(cli, &ui) {
        Ok(code) => code,
        Err(e) => {
            ui.error(&e);
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli, ui: &Ui) -> Result<ExitCode> {
    let paths = Paths::from_env()?;
    match cli.command {
        Cmd::Install {
            source,
            id,
            title,
            proton,
        } => {
            let opts = install::Options {
                source,
                id,
                title,
                force_proton: proton,
            };
            let game = install::install(&paths, &opts, ui)?;
            ui.finish_progress();
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
        }
        Cmd::Run {
            game,
            target,
            print,
            detach,
            args,
        } => {
            let game = library::find(&paths, &game)?;
            let spec = launch::game(&paths, &game, target.as_deref(), &args, ui)?;
            ui.finish_progress();
            if print {
                ui.result(&spec, || ui.line(&shell_line(&spec)));
                return Ok(ExitCode::SUCCESS);
            }
            let mut child = spec
                .command()
                .spawn()
                .with_context(|| format!("starting {}", spec.program.display()))?;
            if detach {
                ui.result(&serde_json::json!({ "pid": child.id() }), || {
                    ui.success(&format!("started {} (pid {})", game.title, child.id()))
                });
                return Ok(ExitCode::SUCCESS);
            }
            let status = child.wait()?;
            return Ok(ExitCode::from(
                status.code().unwrap_or(1).clamp(0, 255) as u8
            ));
        }
        Cmd::List => {
            let games = library::list(&paths)?;
            ui.result(&games, || {
                if games.is_empty() {
                    ui.line("no games yet. install one with: omavn install <download>");
                }
                for g in &games {
                    ui.line(&format!(
                        "{:<24} {}  {}",
                        g.id,
                        g.title,
                        ui.dim(&format!("({})", g.engine.name()))
                    ));
                }
            });
        }
        Cmd::Info { game } => {
            let game = library::find(&paths, &game)?;
            let dir = game.dir(&paths);
            let info = serde_json::json!({
                "game": &game,
                "dir": &dir,
                "files": game.files_dir(&paths),
                "prefix": game.prefix(&paths),
            });
            ui.result(&info, || {
                ui.line(&format!("{} ({})", game.title, game.id));
                ui.line(&format!("  engine   {}", game.engine.name()));
                ui.line(&format!("  runner   {:?}", game.runner));
                ui.line(&format!("  files    {}", game.files_dir(&paths).display()));
                if game.prefix(&paths).exists() {
                    ui.line(&format!("  prefix   {}", game.prefix(&paths).display()));
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
        }
        Cmd::Remove { game, yes } => {
            let game = library::find(&paths, &game)?;
            let dir = game.dir(&paths);
            if !yes
                && !ui.confirm(&format!(
                    "delete {} and its saves ({})?",
                    game.title,
                    dir.display()
                ))?
            {
                bail!("cancelled (pass --yes to skip this prompt)");
            }
            std::fs::remove_dir_all(&dir)?;
            ui.result(&serde_json::json!({ "removed": game.id }), || {
                ui.success(&format!("removed {}", game.id))
            });
        }
        Cmd::Exec { game, command } => {
            let game = library::find(&paths, &game)?;
            let files = game.files_dir(&paths);
            let spec = launch::proton_spec(&paths, &game.prefix(&paths), command, files, ui)?;
            ui.finish_progress();
            let status = spec.command().status()?;
            return Ok(ExitCode::from(
                status.code().unwrap_or(1).clamp(0, 255) as u8
            ));
        }
        Cmd::Inspect { source } => {
            let staged = omavn_core::ingest::unwrap(&source, &paths.staging(), ui)?;
            ui.finish_progress();
            let payload = detect::classify(&staged.root())?;
            let report = match &payload {
                detect::Payload::Game { root, engine } => {
                    let targets = detect::targets(root, *engine);
                    let entry = omavn_core::catalog::lookup(&paths, root)?;
                    serde_json::json!({
                        "type": "game",
                        "engine": engine,
                        "catalog": entry.map(|e| e.id),
                        "renpy_version": detect::renpy_version(root),
                        "targets": targets,
                    })
                }
                detect::Payload::Installer { exe, kind } => serde_json::json!({
                    "type": "installer",
                    "kind": kind,
                    "exe": exe.strip_prefix(&staged.dir).unwrap_or(exe),
                    "unattended": kind.silent_args("C:\\").is_some(),
                }),
            };
            ui.result(&report, || {
                ui.line(&serde_json::to_string_pretty(&report).unwrap_or_default())
            });
        }
        Cmd::Runtime { command } => match command {
            RuntimeCmd::List => {
                let protons = runtime::find_protons(&paths);
                let umu = runtime::which("umu-run").or_else(|| {
                    Some(paths.runtimes().join("umu/umu/umu-run")).filter(|p| p.is_file())
                });
                let renpy: Vec<String> = std::fs::read_dir(paths.runtimes().join("renpy"))
                    .map(|rd| {
                        rd.flatten()
                            .map(|e| e.file_name().to_string_lossy().into_owned())
                            .filter(|n| !n.starts_with('.'))
                            .collect()
                    })
                    .unwrap_or_default();
                let report = serde_json::json!({ "proton": protons, "umu": umu, "renpy": renpy });
                ui.result(&report, || {
                    ui.line("proton");
                    if protons.is_empty() {
                        ui.line("  none (installed automatically when needed)");
                    }
                    for (i, p) in protons.iter().enumerate() {
                        let mark = if i == 0 { "*" } else { " " };
                        let tag = if p.managed { "managed" } else { "found" };
                        ui.line(&format!(
                            " {mark} {:<28} {}",
                            p.name,
                            ui.dim(&format!("{tag}, {}", p.path.display()))
                        ));
                    }
                    ui.line(&format!(
                        "umu-run\n   {}",
                        umu.map(|p| p.display().to_string())
                            .unwrap_or("not installed".into())
                    ));
                    ui.line(&format!(
                        "ren'py\n   {}",
                        if renpy.is_empty() {
                            "none".into()
                        } else {
                            renpy.join(", ")
                        }
                    ));
                });
            }
            RuntimeCmd::Install { what, version } => {
                let path = match what {
                    RuntimeKind::Proton => runtime::install_ge_proton(&paths, ui)?.path,
                    RuntimeKind::Umu => runtime::umu(&paths, ui)?,
                    RuntimeKind::Renpy => {
                        let v = version.context(
                            "which Ren'Py version? e.g. `omavn runtime install renpy 8.5.2`",
                        )?;
                        runtime::renpy(&paths, &v, ui)?
                    }
                };
                ui.finish_progress();
                ui.result(&serde_json::json!({ "path": path }), || {
                    ui.success(&format!("ready: {}", path.display()))
                });
            }
        },
        Cmd::Doctor => {
            let checks = doctor(&paths);
            let ok = checks.iter().all(|c| c.ok || !c.required);
            ui.result(&checks, || {
                for c in &checks {
                    ui.check(c.ok, c.required, &c.name, &c.detail);
                }
            });
            if !ok {
                return Ok(ExitCode::FAILURE);
            }
        }
        Cmd::Feedback {
            message,
            kind,
            game,
            no_open,
        } => {
            let kind = match kind {
                FeedbackKind::Bug => feedback::Kind::Bug,
                FeedbackKind::Game => feedback::Kind::Game,
                FeedbackKind::Idea => feedback::Kind::Idea,
            };
            let game = game.map(|g| library::find(&paths, &g)).transpose()?;
            let report = feedback::build(&paths, kind, &message.join(" "), game.as_ref());
            let opened = !no_open && !cli.json && open_url(&report.url);
            ui.result(&report, || {
                if opened {
                    ui.success("opened a prefilled issue in your browser");
                } else {
                    ui.line("open this link to file it (everything is prefilled):");
                }
                ui.line(&report.url);
            });
        }
    }
    Ok(ExitCode::SUCCESS)
}

#[derive(serde::Serialize)]
struct Check {
    name: String,
    ok: bool,
    required: bool,
    detail: String,
}

fn doctor(paths: &Paths) -> Vec<Check> {
    let mut out = Vec::new();
    let mut add = |name: &str, ok: bool, required: bool, detail: String| {
        out.push(Check {
            name: name.into(),
            ok,
            required,
            detail,
        });
    };
    let protons = runtime::find_protons(paths);
    add(
        "proton",
        true,
        false,
        protons
            .first()
            .map(|p| p.name.clone())
            .unwrap_or_else(|| "none yet, GE-Proton downloads on first use".into()),
    );
    let umu = runtime::which("umu-run")
        .or_else(|| Some(paths.runtimes().join("umu/umu/umu-run")).filter(|p| p.is_file()));
    add(
        "umu-launcher",
        true,
        false,
        umu.map(|p| p.display().to_string())
            .unwrap_or("downloads on first use".into()),
    );
    let py = runtime::which("python3");
    add(
        "python3",
        py.is_some(),
        true,
        py.map(|p| p.display().to_string())
            .unwrap_or("needed by umu-launcher".into()),
    );
    let display = std::env::var("WAYLAND_DISPLAY").is_ok() || std::env::var("DISPLAY").is_ok();
    add(
        "display",
        display,
        true,
        if display {
            "ok".into()
        } else {
            "no WAYLAND_DISPLAY or DISPLAY".into()
        },
    );
    let vulkan = ["/usr/share/vulkan/icd.d", "/etc/vulkan/icd.d"]
        .iter()
        .any(|d| {
            std::fs::read_dir(d)
                .map(|mut r| r.next().is_some())
                .unwrap_or(false)
        });
    add(
        "vulkan",
        vulkan,
        false,
        if vulkan {
            "driver found".into()
        } else {
            "no Vulkan driver; DXVK will fall back to wined3d".into()
        },
    );
    let free = fs_free(&paths.data);
    add(
        "disk",
        free.is_none_or(|b| b > 10 << 30),
        false,
        free.map(|b| format!("{} GiB free", b >> 30))
            .unwrap_or("?".into()),
    );
    out
}

fn fs_free(path: &std::path::Path) -> Option<u64> {
    let mut p = path.to_path_buf();
    while !p.exists() {
        p = p.parent()?.to_path_buf();
    }
    let out = std::process::Command::new("df")
        .arg("-B1")
        .arg("--output=avail")
        .arg(&p)
        .output()
        .ok()?;
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .nth(1)?
        .trim()
        .parse()
        .ok()
}

fn open_url(url: &str) -> bool {
    std::process::Command::new("xdg-open")
        .arg(url)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
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
