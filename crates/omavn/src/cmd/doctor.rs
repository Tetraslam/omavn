use std::process::ExitCode;

use anyhow::Result;
use omavn_core::{Paths, runtime};

use crate::ui::Ui;

pub fn run(paths: &Paths, ui: &Ui) -> Result<ExitCode> {
    let checks = checks(paths);
    let ok = checks.iter().all(|c| c.ok || !c.required);
    ui.result(&checks, || {
        for c in &checks {
            ui.check(c.ok, c.required, &c.name, &c.detail);
        }
    });
    if !ok {
        return Ok(ExitCode::FAILURE);
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

fn checks(paths: &Paths) -> Vec<Check> {
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
