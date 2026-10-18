use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::Result;
use omavn_core::{Paths, detect};

use crate::ui::Ui;

pub fn inspect(paths: &Paths, ui: &Ui, source: PathBuf) -> Result<ExitCode> {
    let staged = omavn_core::ingest::unwrap(&source, &paths.staging(), ui)?;
    ui.finish_progress();
    let payload = detect::classify(&staged.root())?;
    let report = match &payload {
        detect::Payload::Game { root, engine } => {
            let targets = detect::targets(root, *engine);
            let entry = omavn_core::catalog::lookup(paths, root)?;
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
    Ok(ExitCode::SUCCESS)
}
