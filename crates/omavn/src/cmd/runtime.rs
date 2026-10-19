use std::process::ExitCode;

use anyhow::{Context, Result};
use omavn_core::{Paths, runtime};

use crate::cli::RuntimeKind;
use crate::ui::Ui;
use omavn_core::paths::pretty;

pub fn list(paths: &Paths, ui: &Ui) -> Result<ExitCode> {
    let protons = runtime::find_protons(paths);
    let umu = runtime::which("umu-run")
        .or_else(|| Some(paths.runtimes().join("umu/umu/umu-run")).filter(|p| p.is_file()));
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
                ui.dim(&format!("{tag}, {}", pretty(&p.path)))
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
    Ok(ExitCode::SUCCESS)
}

pub fn install(
    paths: &Paths,
    ui: &Ui,
    what: RuntimeKind,
    version: Option<String>,
) -> Result<ExitCode> {
    let path = match what {
        RuntimeKind::Proton => runtime::install_ge_proton(paths, ui)?.path,
        RuntimeKind::Umu => runtime::umu(paths, ui)?,
        RuntimeKind::Renpy => {
            let v = version
                .context("which Ren'Py version? e.g. `omavn runtime install renpy 8.5.2`")?;
            runtime::renpy(paths, &v, ui)?
        }
    };
    ui.finish_progress();
    ui.result(&serde_json::json!({ "path": path }), || {
        ui.success(&format!("ready: {}", pretty(&path)))
    });
    Ok(ExitCode::SUCCESS)
}
