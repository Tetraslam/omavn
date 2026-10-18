mod ui;

use std::ffi::OsString;
use std::io::IsTerminal;
use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand, ValueEnum};
use omavn_core::library::{Display, Game};
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
    command: Option<Cmd>,
}

#[derive(Subcommand)]
enum Cmd {
    /// Install a game from a download (file or folder).
    #[command(visible_alias = "add")]
    Install {
        /// The archive, installer, or folder you downloaded.
        #[arg(required_unless_present = "pick")]
        source: Option<PathBuf>,
        /// Choose the download from a menu (Omarchy).
        #[arg(long)]
        pick: bool,
        /// Library id (defaults to a slug of the title).
        #[arg(long)]
        id: Option<String>,
        /// Display title.
        #[arg(long)]
        title: Option<String>,
        /// Run through Proton even if a native runtime is available.
        #[arg(long)]
        proton: bool,
        /// Skip the VNDB lookup and art download.
        #[arg(long)]
        offline: bool,
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
        /// Show the runner's output instead of writing it to the game's log.
        #[arg(short, long)]
        verbose: bool,
        /// Run in a plain window instead of gamescope, just this once.
        #[arg(long, conflicts_with = "gamescope")]
        window: bool,
        /// Run inside gamescope, just this once.
        #[arg(long)]
        gamescope: bool,
        /// Extra arguments passed to the game.
        #[arg(last = true)]
        args: Vec<OsString>,
    },
    /// List installed games.
    #[command(visible_alias = "ls")]
    List,
    /// Pick a game from a cover-art grid and play it (Omarchy's picker, or a text menu).
    #[command(visible_alias = "pick")]
    Library,
    /// Refresh a game's VNDB metadata, art, and launcher entry.
    Meta {
        /// Game id or title; omit with --all.
        game: Option<String>,
        /// Use this VNDB id (e.g. v548) instead of searching by title.
        #[arg(long)]
        vndb: Option<String>,
        /// Refresh every installed game.
        #[arg(long, conflicts_with = "game")]
        all: bool,
    },
    /// Change a game setting: display (auto|gamescope|window), target, title, locale (e.g. ja), env.NAME.
    ///
    /// Omit the value to reset it (for env.NAME, to remove the variable).
    Set {
        game: String,
        key: String,
        value: Option<String>,
    },
    /// Show details about an installed game.
    Info { game: String },
    /// Uninstall a game, including its saves.
    #[command(visible_alias = "rm")]
    Remove {
        #[arg(required_unless_present = "pick")]
        game: Option<String>,
        /// Choose the game from a menu (Omarchy).
        #[arg(long)]
        pick: bool,
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
    /// Set up (or undo) Omarchy integration: menu entries, window rules, packages.
    Omarchy {
        #[command(subcommand)]
        command: OmarchyCmd,
    },
    /// Show where a game keeps its saves (for backups), newest first.
    Saves {
        game: String,
        /// Open the newest save folder in your file manager.
        #[arg(long)]
        open: bool,
    },
    /// Print shell completions, e.g. `omavn completions bash > ~/.local/share/bash-completion/completions/omavn`.
    Completions {
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },
    /// Manage omavn itself.
    #[command(name = "self")]
    Itself {
        #[command(subcommand)]
        command: SelfAction,
    },
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

#[derive(Subcommand)]
enum OmarchyCmd {
    /// Install gamescope and umu-launcher, add window rules and menu entries.
    Setup {
        /// Don't install packages (no sudo).
        #[arg(long)]
        skip_packages: bool,
    },
    /// Remove the window rules and menu entries.
    Remove,
    /// Show what is set up.
    Status,
}

#[derive(Subcommand)]
enum SelfAction {
    /// Update omavn to the latest release.
    Update {
        /// Reinstall even if already up to date.
        #[arg(long)]
        force: bool,
    },
    /// Check whether a newer release exists.
    Check,
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
    let Some(command) = cli.command else {
        return home(&paths, ui, cli.json);
    };
    match command {
        Cmd::Install {
            source,
            pick,
            id,
            title,
            proton,
            offline,
        } => {
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
            let game = install::install(&paths, &opts, ui)?;
            ui.finish_progress();
            if omavn_core::desktop::has_display() && !cli.json {
                let exe = std::env::current_exe()?.display().to_string();
                omavn_core::omarchy::notify(
                    &format!("{} is ready", game.title),
                    "Click to play",
                    Some(&omavn_core::art::cover(&paths, &game)),
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
        }
        Cmd::Run {
            game,
            target,
            print,
            detach,
            verbose,
            window,
            gamescope,
            args,
        } => {
            let game = library::find(&paths, &game)?;
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
            let spec = launch::game(&paths, &game, &opts, ui)?;
            ui.finish_progress();
            if print {
                ui.result(&spec, || ui.line(&shell_line(&spec)));
                return Ok(ExitCode::SUCCESS);
            }
            let mut cmd = spec.command();
            let log = game.log_file(&paths);
            if !verbose {
                let file = std::fs::File::create(&log)?;
                cmd.stdout(file.try_clone()?)
                    .stderr(file)
                    .stdin(std::process::Stdio::null());
            }
            let mut child = cmd
                .spawn()
                .with_context(|| format!("starting {}", spec.program.display()))?;
            if detach {
                ui.result(
                    &serde_json::json!({ "pid": child.id(), "log": log }),
                    || ui.success(&format!("started {} (pid {})", game.title, child.id())),
                );
                return Ok(ExitCode::SUCCESS);
            }
            // Launched from a menu or launcher: there is no terminal to show
            // progress, and Proton takes a few seconds to open a window.
            let headless = !std::io::stdout().is_terminal() && !cli.json;
            if headless {
                omavn_core::omarchy::notify(
                    &format!("Starting {}", game.title),
                    "",
                    Some(&omavn_core::art::cover(&paths, &game)),
                    &[],
                    Some(5000),
                );
            } else if !verbose && !cli.json {
                ui.line(&format!("▶ {}", game.title));
            }
            let started = std::time::Instant::now();
            let status = child.wait()?;
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
            return Ok(ExitCode::from(
                status.code().unwrap_or(1).clamp(0, 255) as u8
            ));
        }
        Cmd::Library => return library_picker(&paths, ui),
        Cmd::Meta { game, vndb, all } => {
            let mut games = if all {
                library::list(&paths)?
            } else {
                let q = game.context("which game? (or --all)")?;
                vec![library::find(&paths, &q)?]
            };
            for g in &mut games {
                (ui as &dyn omavn_core::Reporter).step(g.title.clone());
                omavn_core::meta::refresh(&paths, g, vndb.as_deref(), ui)?;
            }
            ui.finish_progress();
            ui.result(&games, || {
                ui.success(&format!("refreshed {} game(s)", games.len()))
            });
        }
        Cmd::Set { game, key, value } => {
            let mut game = library::find(&paths, &game)?;
            set(&mut game, &key, value.as_deref())?;
            game.save(&paths)?;
            ui.result(&game, || ui.success(&format!("{}: {key} updated", game.id)));
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
        Cmd::Remove { game, pick, yes } => {
            let game = match game {
                Some(g) => library::find(&paths, &g)?,
                None if pick => match pick_game(&paths)? {
                    Some(g) => g,
                    None => return Ok(ExitCode::SUCCESS),
                },
                None => bail!("which game?"),
            };
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
            omavn_core::meta::remove(&paths, &game);
            std::fs::remove_dir_all(&dir)?;
            ui.result(&serde_json::json!({ "removed": game.id }), || {
                ui.success(&format!("removed {}", game.id))
            });
        }
        Cmd::Exec { game, command } => {
            let game = library::find(&paths, &game)?;
            let files = game.files_dir(&paths);
            let spec = launch::proton_spec(
                &paths,
                &game.prefix(&paths),
                &game.umu_id(),
                command,
                files,
                ui,
            )?;
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
        Cmd::Saves { game, open } => {
            let game = library::find(&paths, &game)?;
            let dirs = omavn_core::saves::find(&paths, &game);
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
        }
        Cmd::Completions { shell } => {
            use clap::CommandFactory;
            clap_complete::generate(shell, &mut Cli::command(), "omavn", &mut std::io::stdout());
        }
        Cmd::Omarchy { command } => match command {
            OmarchyCmd::Setup { skip_packages } => {
                let exe = std::env::current_exe()?;
                let changes = omavn_core::omarchy::setup(&exe, skip_packages)?;
                ui.result(&changes, || {
                    if changes.is_empty() {
                        ui.success("already set up");
                    }
                    for c in &changes {
                        ui.success(c);
                    }
                    ui.line("open your library from the Omarchy menu (Visual Novels) or run: omavn library");
                });
            }
            OmarchyCmd::Remove => {
                let changes = omavn_core::omarchy::remove()?;
                ui.result(&changes, || {
                    for c in &changes {
                        ui.success(c);
                    }
                });
            }
            OmarchyCmd::Status => {
                let s = omavn_core::omarchy::status();
                ui.result(&s, || {
                    ui.check(
                        s.omarchy,
                        true,
                        "omarchy",
                        if s.omarchy { "detected" } else { "not found" },
                    );
                    for (p, ok) in &s.packages {
                        ui.check(*ok, false, p, if *ok { "installed" } else { "missing" });
                    }
                    ui.check(
                        s.window_rules,
                        false,
                        "window rules",
                        if s.window_rules {
                            "loaded"
                        } else {
                            "not set up"
                        },
                    );
                    ui.check(
                        s.menu,
                        false,
                        "menu",
                        if s.menu {
                            "Visual Novels entry present"
                        } else {
                            "not set up"
                        },
                    );
                });
            }
        },
        Cmd::Itself { command } => match command {
            SelfAction::Check => {
                let a = omavn_core::update::check()?;
                ui.result(&a, || {
                    if a.newer {
                        ui.line(&format!(
                            "omavn {} is out (you have {}); run: omavn self update",
                            a.latest, a.current
                        ));
                    } else {
                        ui.success(&format!("omavn {} is the latest", a.current));
                    }
                });
            }
            SelfAction::Update { force } => {
                let updated = omavn_core::update::update(&paths, force, ui)?;
                ui.finish_progress();
                let current = omavn_core::update::VERSION;
                ui.result(
                    &serde_json::json!({ "updated": updated, "previous": current }),
                    || match &updated {
                        Some(v) => ui.success(&format!("updated omavn {current} -> {v}")),
                        None => ui.success(&format!("omavn {current} is already the latest")),
                    },
                );
            }
        },
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

/// `omavn` on its own: the library at a glance, or how to get started.
fn home(paths: &Paths, ui: &Ui, json: bool) -> Result<ExitCode> {
    let games = library::list(paths)?;
    if json {
        ui.result(&games, || {});
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
        for g in &games {
            ui.line(&format!("  {:<24} {}", g.id, g.title));
        }
        ui.line("\nplay one:  omavn run <id>    browse:  omavn library    add more:  omavn install <download>");
    }
    ui.line(&ui.dim("\nall commands: omavn --help    docs: https://omavn.dev"));
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
        .arg("zip rar 7z exe 001 msi")
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

/// Show the library as a cover grid (Omarchy) or a numbered list, then play the pick.
fn library_picker(paths: &Paths, ui: &Ui) -> Result<ExitCode> {
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
    let picked = if runtime::which("omarchy-menu-images").is_some() {
        let out = std::process::Command::new("omarchy-menu-images")
            .args(["--show-labels", "--filterable", "--print-name"])
            .arg(omavn_core::art::library_dir(paths))
            .output()?;
        let name = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if name.is_empty() {
            return Ok(ExitCode::SUCCESS);
        }
        omavn_core::art::game_for_entry(paths, &name)
            .with_context(|| format!("no game for library entry `{name}`"))?
    } else {
        for (i, g) in games.iter().enumerate() {
            ui.line(&format!("{:>3}  {}", i + 1, g.title));
        }
        eprint!("play which? ");
        let mut line = String::new();
        std::io::stdin().read_line(&mut line)?;
        let n: usize = line.trim().parse().context("not a number")?;
        games
            .get(n.wrapping_sub(1))
            .context("no such game")?
            .id
            .clone()
    };
    let exe = std::env::current_exe()?;
    let status = std::process::Command::new(exe)
        .args(["run", &picked])
        .status()?;
    Ok(ExitCode::from(
        status.code().unwrap_or(1).clamp(0, 255) as u8
    ))
}

fn set(game: &mut Game, key: &str, value: Option<&str>) -> Result<()> {
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
        "ja" => "ja_JP.UTF-8".into(),
        "zh" | "zh-cn" => "zh_CN.UTF-8".into(),
        "zh-tw" => "zh_TW.UTF-8".into(),
        "ko" => "ko_KR.UTF-8".into(),
        "en" => "en_US.UTF-8".into(),
        other => other.into(),
    }
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
    let gamescope = runtime::which("gamescope");
    add(
        "gamescope",
        gamescope.is_some(),
        false,
        if gamescope.is_some() {
            "Windows games run scaled to fill the screen".into()
        } else {
            "not installed; fixed-resolution games may be stretched by tiling window managers"
                .into()
        },
    );
    let lib32 = omavn_core::desktop::lib32_vulkan();
    add(
        "32-bit vulkan",
        lib32,
        false,
        if lib32 {
            "found (most VNs are 32-bit)".into()
        } else {
            format!(
                "missing; 32-bit games may render black. {}",
                omavn_core::desktop::lib32_vulkan_hint()
            )
        },
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
