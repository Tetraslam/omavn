use std::process::ExitCode;

use anyhow::Result;
use omavn_core::{Paths, feedback, library};

use super::open_url;
use crate::cli::{FeedbackArgs, FeedbackKind};
use crate::ui::Ui;

pub fn run(paths: &Paths, ui: &Ui, json: bool, args: FeedbackArgs) -> Result<ExitCode> {
    let FeedbackArgs {
        message,
        kind,
        game,
        no_open,
    } = args;
    let kind = match kind {
        FeedbackKind::Bug => feedback::Kind::Bug,
        FeedbackKind::Game => feedback::Kind::Game,
        FeedbackKind::Idea => feedback::Kind::Idea,
    };
    let game = game.map(|g| library::find(paths, &g)).transpose()?;
    let report = feedback::build(paths, kind, &message.join(" "), game.as_ref());
    let opened = !no_open && !json && open_url(&report.url);
    ui.result(&report, || {
        if opened {
            ui.success("opened a prefilled issue in your browser");
        } else {
            ui.line("open this link to file it (everything is prefilled):");
        }
        ui.line(&report.url);
    });
    Ok(ExitCode::SUCCESS)
}
