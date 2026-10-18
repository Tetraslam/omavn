use std::process::ExitCode;

use anyhow::Result;

use crate::ui::Ui;

pub fn setup(ui: &Ui, skip_packages: bool) -> Result<ExitCode> {
    let exe = std::env::current_exe()?;
    let changes = omavn_core::omarchy::setup(&exe, skip_packages)?;
    ui.result(&changes, || {
        if changes.is_empty() {
            ui.success("already set up");
        }
        for c in &changes {
            ui.success(c);
        }
        ui.line(
            "open your library with Super+Shift+V, Visual Novels in the Omarchy menu, or omavn gui",
        );
    });
    Ok(ExitCode::SUCCESS)
}

pub fn remove(ui: &Ui) -> Result<ExitCode> {
    let changes = omavn_core::omarchy::remove()?;
    ui.result(&changes, || {
        for c in &changes {
            ui.success(c);
        }
    });
    Ok(ExitCode::SUCCESS)
}

pub fn status(ui: &Ui) -> Result<ExitCode> {
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
    Ok(ExitCode::SUCCESS)
}
