use std::ffi::OsString;
use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};

/// Play visual novels on Linux. Point omavn at whatever you downloaded
/// (zips, split rars, installers, plain folders) and it handles the rest.
#[derive(Parser)]
#[command(name = "omavn", version, propagate_version = true)]
pub struct Cli {
    /// Machine-readable output: results as JSON on stdout, progress as JSON lines on stderr.
    #[arg(long, global = true)]
    pub json: bool,

    #[command(subcommand)]
    pub command: Option<Cmd>,
}

#[derive(Subcommand)]
pub enum Cmd {
    /// Install a game from a download (file or folder).
    #[command(visible_alias = "add")]
    Install(InstallArgs),
    /// Launch an installed game.
    #[command(visible_alias = "play")]
    Run(RunArgs),
    /// List installed games, most recently played first. Never interactive.
    #[command(visible_alias = "ls")]
    List(ListArgs),
    /// Open the graphical library (a cover grid on Omarchy) and play the pick.
    #[command(visible_alias = "launcher", alias = "library")]
    Gui,
    /// Refresh a game's VNDB metadata, art, and launcher entry.
    Meta(MetaArgs),
    /// Change a game setting: display (auto|gamescope|window), target, title, locale (e.g. ja), env.NAME.
    ///
    /// Omit the value to reset it (for env.NAME, to remove the variable).
    Set(SetArgs),
    /// Show details about an installed game.
    Info { game: String },
    /// Uninstall a game, including its saves.
    #[command(visible_alias = "rm")]
    Remove(RemoveArgs),
    /// Run a program inside a game's Wine prefix (e.g. `winecfg`, or another exe).
    Exec(ExecArgs),
    /// Install Windows components into a game's prefix with winetricks (e.g. quartz wmp9).
    Winetricks(WinetricksArgs),
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
    Saves(SavesArgs),
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
    Feedback(FeedbackArgs),
}

#[derive(Subcommand)]
pub enum RuntimeCmd {
    /// Show available runtimes.
    #[command(visible_alias = "ls")]
    List,
    /// Download a runtime.
    Install {
        #[arg(value_enum)]
        what: RuntimeKind,
        /// Ren'Py version (for `renpy`).
        #[arg(id = "renpy_version", value_name = "VERSION")]
        version: Option<String>,
    },
}

#[derive(Clone, Copy, ValueEnum)]
pub enum SortBy {
    Recent,
    Title,
    Installed,
}

#[derive(Subcommand)]
pub enum OmarchyCmd {
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
pub enum SelfAction {
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
pub enum RuntimeKind {
    /// Latest GE-Proton.
    Proton,
    Umu,
    Renpy,
}

#[derive(Clone, Copy, ValueEnum)]
pub enum FeedbackKind {
    Bug,
    Game,
    Idea,
}

#[derive(Args)]
pub struct InstallArgs {
    /// The archive, installer, or folder you downloaded.
    #[arg(required_unless_present = "pick")]
    pub source: Option<PathBuf>,
    /// Choose the download from a menu (Omarchy).
    #[arg(long)]
    pub pick: bool,
    /// Library id (defaults to a slug of the title).
    #[arg(long)]
    pub id: Option<String>,
    /// Display title.
    #[arg(long)]
    pub title: Option<String>,
    /// Run through Proton even if a native runtime is available.
    #[arg(long)]
    pub proton: bool,
    /// Skip the VNDB lookup and art download.
    #[arg(long)]
    pub offline: bool,
}

#[derive(Args)]
pub struct RunArgs {
    /// Game id or part of its title. Omit it to pick from a list (needs a terminal).
    pub game: Option<String>,
    /// Which entry point (see `omavn info`), e.g. a config tool or another edition.
    #[arg(short, long)]
    pub target: Option<String>,
    /// Print the command instead of running it.
    #[arg(long)]
    pub print: bool,
    /// Return immediately instead of waiting for the game to exit.
    #[arg(short, long)]
    pub detach: bool,
    /// Show the runner's output instead of writing it to the game's log.
    #[arg(short, long)]
    pub verbose: bool,
    /// Run in a plain window instead of gamescope, just this once.
    #[arg(long, conflicts_with = "gamescope")]
    pub window: bool,
    /// Run inside gamescope, just this once.
    #[arg(long)]
    pub gamescope: bool,
    /// Extra arguments passed to the game.
    #[arg(last = true)]
    pub args: Vec<OsString>,
}

#[derive(Args)]
pub struct ListArgs {
    /// Only games whose id, title, or engine contain all of these words.
    pub query: Vec<String>,
    #[arg(long, value_enum, default_value = "recent")]
    pub sort: SortBy,
    /// Show at most this many.
    #[arg(short = 'n', long)]
    pub limit: Option<usize>,
}

#[derive(Args)]
pub struct MetaArgs {
    /// Game id or title; omit with --all.
    pub game: Option<String>,
    /// Use this VNDB id (e.g. v548) instead of searching by title.
    #[arg(long)]
    pub vndb: Option<String>,
    /// Refresh every installed game.
    #[arg(long, conflicts_with = "game")]
    pub all: bool,
}

#[derive(Args)]
pub struct SetArgs {
    pub game: String,
    pub key: String,
    pub value: Option<String>,
}

#[derive(Args)]
pub struct RemoveArgs {
    /// Omit to pick from a list (needs a terminal).
    pub game: Option<String>,
    /// Choose the game from a menu (Omarchy).
    #[arg(long)]
    pub pick: bool,
    /// Don't ask for confirmation.
    #[arg(short, long)]
    pub yes: bool,
}

#[derive(Args)]
pub struct ExecArgs {
    pub game: String,
    #[arg(last = true, required = true)]
    pub command: Vec<OsString>,
}

#[derive(Args)]
pub struct WinetricksArgs {
    pub game: String,
    #[arg(required = true)]
    pub verbs: Vec<String>,
}

#[derive(Args)]
pub struct SavesArgs {
    pub game: String,
    /// Open the newest save folder in your file manager.
    #[arg(long)]
    pub open: bool,
    /// Copy saves from Windows into the game, e.g. /mnt/c/Users/me/AppData/Roaming/light.
    #[arg(long, value_name = "PATH")]
    pub import: Option<PathBuf>,
}

#[derive(Args)]
pub struct FeedbackArgs {
    /// What happened, or what you'd like.
    pub message: Vec<String>,
    #[arg(short, long, value_enum, default_value = "bug")]
    pub kind: FeedbackKind,
    /// Attach details about this installed game.
    #[arg(short, long)]
    pub game: Option<String>,
    /// Only print the link, don't open a browser.
    #[arg(long)]
    pub no_open: bool,
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;

    #[test]
    fn cli_is_well_formed() {
        super::Cli::command().debug_assert();
    }
}
