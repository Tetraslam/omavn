//! Bug reports and requests: gather diagnostics and build a prefilled
//! GitHub issue, so reporting a broken game takes one command.

use std::fmt::Write;
use std::process::Command;

use crate::library::Game;
use crate::paths::Paths;
use crate::runtime;

pub const REPO: &str = "Tetraslam/omavn";

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Bug,
    Game,
    Idea,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Report {
    pub title: String,
    pub body: String,
    pub labels: Vec<String>,
    pub url: String,
}

pub fn system_info(paths: &Paths) -> String {
    let mut s = String::new();
    let os = std::fs::read_to_string("/etc/os-release")
        .ok()
        .and_then(|t| {
            t.lines().find_map(|l| {
                l.strip_prefix("PRETTY_NAME=")
                    .map(|v| v.trim_matches('"').to_string())
            })
        })
        .unwrap_or_else(|| "unknown".into());
    let kernel = Command::new("uname")
        .arg("-r")
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());
    let session = std::env::var("XDG_SESSION_TYPE").unwrap_or_else(|_| "unknown".into());
    let desktop = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_else(|_| "unknown".into());
    let proton = runtime::find_protons(paths)
        .into_iter()
        .next()
        .map(|p| p.name);
    let _ = writeln!(s, "- omavn: {}", env!("CARGO_PKG_VERSION"));
    let _ = writeln!(
        s,
        "- os: {os} (kernel {})",
        kernel.as_deref().unwrap_or("?")
    );
    let _ = writeln!(s, "- session: {session} / {desktop}");
    let _ = writeln!(
        s,
        "- proton: {}",
        proton.as_deref().unwrap_or("none installed")
    );
    let _ = writeln!(
        s,
        "- umu-run: {}",
        runtime::which("umu-run")
            .map(|_| "on PATH")
            .unwrap_or("managed or missing")
    );
    s
}

pub fn build(paths: &Paths, kind: Kind, message: &str, game: Option<&Game>) -> Report {
    let mut body = String::new();
    let _ = writeln!(
        body,
        "{}\n",
        if message.is_empty() {
            "<!-- what happened / what you'd like -->"
        } else {
            message
        }
    );
    if let Some(g) = game {
        let _ = writeln!(body, "### Game\n");
        let _ = writeln!(body, "- title: {}", g.title);
        let _ = writeln!(
            body,
            "- engine: {} / runner: {:?}",
            g.engine.name(),
            g.runner
        );
        if let Some(c) = &g.catalog_id {
            let _ = writeln!(body, "- catalog: {c}");
        }
        let targets: Vec<String> = g
            .targets
            .iter()
            .map(|t| format!("`{}`", t.exe.display()))
            .collect();
        let _ = writeln!(body, "- targets: {}", targets.join(", "));
        if let Some(name) = g.source.file_name() {
            let _ = writeln!(body, "- download: `{}`", name.to_string_lossy());
        }
        body.push('\n');
    }
    let _ = writeln!(body, "### System\n\n{}", system_info(paths));

    let first_line = message.lines().next().unwrap_or("").trim();
    let title = match (kind, game) {
        (Kind::Game, Some(g)) if first_line.is_empty() => format!("[game] {}", g.title),
        (Kind::Game, Some(g)) => format!("[game] {}: {first_line}", g.title),
        _ if first_line.is_empty() => format!("[{}] ", label(kind)),
        _ => format!("[{}] {first_line}", label(kind)),
    };
    let labels = vec![label(kind).to_string()];
    let url = format!(
        "https://github.com/{REPO}/issues/new?title={}&body={}&labels={}",
        encode(&title),
        encode(&body),
        encode(&labels.join(","))
    );
    Report {
        title,
        body,
        labels,
        url,
    }
}

fn label(kind: Kind) -> &'static str {
    match kind {
        Kind::Bug => "bug",
        Kind::Game => "game-compat",
        Kind::Idea => "idea",
    }
}

fn encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
            out.push(b as char);
        } else {
            let _ = write!(out, "%{b:02X}");
        }
    }
    out
}
