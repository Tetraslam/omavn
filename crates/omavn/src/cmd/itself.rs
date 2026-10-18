use std::process::ExitCode;

use anyhow::Result;
use omavn_core::Paths;

use crate::ui::Ui;

pub fn check(ui: &Ui) -> Result<ExitCode> {
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
    Ok(ExitCode::SUCCESS)
}

pub fn update(paths: &Paths, ui: &Ui, force: bool) -> Result<ExitCode> {
    let updated = omavn_core::update::update(paths, force, ui)?;
    ui.finish_progress();
    let current = omavn_core::update::VERSION;
    ui.result(
        &serde_json::json!({ "updated": updated, "previous": current }),
        || match &updated {
            Some(v) => ui.success(&format!("updated omavn {current} -> {v}")),
            None => ui.success(&format!("omavn {current} is already the latest")),
        },
    );
    Ok(ExitCode::SUCCESS)
}
