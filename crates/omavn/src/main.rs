mod cli;
mod cmd;
mod ui;

use std::process::ExitCode;

use anyhow::Result;
use clap::{CommandFactory, Parser};
use omavn_core::Paths;

use crate::cli::{Cli, Cmd, OmarchyCmd, RuntimeCmd, SelfAction, SteamCmd};
use crate::ui::Ui;

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
    let paths = &paths;
    let json = cli.json;
    let Some(command) = cli.command else {
        return cmd::library::home(paths, ui, json);
    };
    match command {
        Cmd::Install(args) => cmd::install::install(paths, ui, json, args),
        Cmd::Run(args) => cmd::play::run(paths, ui, json, args),
        Cmd::List(args) => cmd::library::list(paths, ui, args),
        Cmd::Gui => cmd::play::gui(paths, ui),
        Cmd::Meta(args) => cmd::manage::meta(paths, ui, args),
        Cmd::Set(args) => cmd::manage::set(paths, ui, args),
        Cmd::Info { game } => cmd::library::info(paths, ui, game),
        Cmd::Remove(args) => cmd::manage::remove(paths, ui, args),
        Cmd::Patch { game, files } => cmd::install::patch(paths, ui, game, files),
        Cmd::Exec(args) => cmd::manage::exec(paths, ui, args),
        Cmd::Winetricks(args) => cmd::manage::winetricks(paths, ui, args),
        Cmd::Saves(args) => cmd::manage::saves(paths, ui, args),
        Cmd::Inspect { source } => cmd::inspect::inspect(paths, ui, source),
        Cmd::Runtime { command } => match command {
            RuntimeCmd::List => cmd::runtime::list(paths, ui),
            RuntimeCmd::Install { what, version } => {
                cmd::runtime::install(paths, ui, what, version)
            }
        },
        Cmd::Doctor => cmd::doctor::run(paths, ui),
        Cmd::Omarchy { command } => match command {
            OmarchyCmd::Setup { skip_packages } => cmd::omarchy::setup(ui, skip_packages),
            OmarchyCmd::Remove => cmd::omarchy::remove(ui),
            OmarchyCmd::Status => cmd::omarchy::status(ui),
        },
        Cmd::Steam { command } => match command {
            SteamCmd::Add { games, restart } => cmd::steam::add(paths, ui, games, restart),
            SteamCmd::Remove { games, restart } => cmd::steam::remove(paths, ui, games, restart),
            SteamCmd::Status => cmd::steam::status(paths, ui),
        },
        Cmd::Itself { command } => match command {
            SelfAction::Check => cmd::itself::check(ui),
            SelfAction::Update { force } => cmd::itself::update(paths, ui, force),
        },
        Cmd::Feedback(args) => cmd::feedback::run(paths, ui, json, args),
        Cmd::Completions { shell } => {
            clap_complete::generate(shell, &mut Cli::command(), "omavn", &mut std::io::stdout());
            Ok(ExitCode::SUCCESS)
        }
    }
}
