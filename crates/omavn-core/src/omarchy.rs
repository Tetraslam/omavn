//! Omarchy (https://omarchy.org) integration: menu entries, Hyprland window
//! rules, gaming packages, and notifications. Everything written here is
//! marked and can be undone with [`remove`].

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

use crate::runtime::which;

const MARK: &str = "omavn omarchy setup";

pub fn detected() -> bool {
    which("omarchy-version").is_some() || home().join(".local/share/omarchy").is_dir()
}

fn home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default()
}

fn config_home() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(".config"))
}

fn hypr_main() -> PathBuf {
    config_home().join("hypr/hyprland.lua")
}

fn hypr_rules() -> PathBuf {
    config_home().join("hypr/omavn.lua")
}

fn menu_file() -> PathBuf {
    config_home().join("omarchy/extensions/omarchy-menu.jsonc")
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Status {
    pub omarchy: bool,
    pub packages: Vec<(String, bool)>,
    pub window_rules: bool,
    pub menu: bool,
}

/// Packages that make Windows VNs work well: gamescope for scaling and
/// input, umu-launcher for running Proton outside Steam.
pub const PACKAGES: &[&str] = &["gamescope", "umu-launcher"];

pub fn status() -> Status {
    Status {
        omarchy: detected(),
        packages: PACKAGES
            .iter()
            .map(|p| (p.to_string(), pkg_installed(p)))
            .collect(),
        window_rules: fs::read_to_string(hypr_main())
            .is_ok_and(|t| t.contains("require(\"hypr.omavn\")")),
        menu: fs::read_to_string(menu_file()).is_ok_and(|t| t.contains("\"visual-novels\"")),
    }
}

fn pkg_installed(pkg: &str) -> bool {
    Command::new("pacman")
        .args(["-Q", pkg])
        .output()
        .is_ok_and(|o| o.status.success())
}

/// Install packages (prompts for sudo), window rules, and menu entries.
/// Returns a human-readable list of what changed.
pub fn setup(omavn: &Path, skip_packages: bool) -> Result<Vec<String>> {
    if !detected() {
        bail!("Omarchy not found; this sets up Omarchy-specific integration");
    }
    let mut changes = Vec::new();

    if !skip_packages {
        let missing: Vec<&str> = PACKAGES
            .iter()
            .copied()
            .filter(|p| !pkg_installed(p))
            .collect();
        if !missing.is_empty() {
            run("omarchy-pkg-add", &missing)?;
            changes.push(format!("installed {}", missing.join(", ")));
        }
        // 32-bit Vulkan drivers: most VNs are 32-bit and render black without them.
        if which("omarchy-install-gaming-gpu-lib32").is_some() {
            run("omarchy-install-gaming-gpu-lib32", &[])?;
        }
    }

    let bind = if keybinding_free() {
        Some(omavn)
    } else {
        changes.push(format!(
            "skipped the {KEYS} keybinding: it's already in use"
        ));
        None
    };
    if write_if_changed(&hypr_rules(), &rules_lua(bind))? {
        changes.push(format!("wrote window rules to {}", hypr_rules().display()));
    }
    let main = hypr_main();
    let text = fs::read_to_string(&main).with_context(|| format!("reading {}", main.display()))?;
    if !text.contains("require(\"hypr.omavn\")") {
        let block = format!("\n-- Visual novel window rules ({MARK})\nrequire(\"hypr.omavn\")\n");
        fs::write(
            &main,
            format!("{}{block}", text.trim_end_matches('\n').to_owned() + "\n"),
        )?;
        changes.push(format!("loaded them from {}", main.display()));
    }

    if add_menu_entries(omavn)? {
        changes.push(format!(
            "added Visual Novels to the Omarchy menu ({})",
            menu_file().display()
        ));
    }
    Ok(changes)
}

/// Undo [`setup`] (packages stay installed).
pub fn remove() -> Result<Vec<String>> {
    let mut changes = Vec::new();
    let main = hypr_main();
    if let Ok(text) = fs::read_to_string(&main)
        && text.contains("require(\"hypr.omavn\")")
    {
        let kept: Vec<&str> = text
            .lines()
            .filter(|l| !l.contains("require(\"hypr.omavn\")") && !l.contains(MARK))
            .collect();
        fs::write(&main, kept.join("\n").trim_end().to_owned() + "\n")?;
        changes.push(format!("unloaded window rules from {}", main.display()));
    }
    if fs::remove_file(hypr_rules()).is_ok() {
        changes.push(format!("deleted {}", hypr_rules().display()));
    }
    if remove_menu_entries()? {
        changes.push("removed Visual Novels from the Omarchy menu".into());
    }
    Ok(changes)
}

const KEYS: &str = "SUPER + SHIFT + V";

/// Whether Super+Shift+V is unbound in the running Hyprland (or ours).
fn keybinding_free() -> bool {
    let Ok(out) = Command::new("hyprctl").args(["-j", "binds"]).output() else {
        return true;
    };
    let Ok(binds) = serde_json::from_slice::<Vec<serde_json::Value>>(&out.stdout) else {
        return true;
    };
    // modmask: SUPER = 64, SHIFT = 1.
    !binds.iter().any(|b| {
        b["modmask"] == 65
            && b["key"]
                .as_str()
                .is_some_and(|k| k.eq_ignore_ascii_case("v"))
            && !b["arg"].as_str().unwrap_or("").contains("omavn")
    })
}

fn rules_lua(bind: Option<&Path>) -> String {
    let bind = bind
        .map(|exe| {
            format!(
                "\n-- Open your visual novel library.\no.bind(\"{KEYS}\", \"Visual novels\", o.launch(\"'{}' gui\"))\n",
                exe.display()
            )
        })
        .unwrap_or_default();
    format!(
        r#"-- Written by `{MARK}`; undo with `omavn omarchy remove`.
if not o then return end

-- Visual novels run inside gamescope, scaled to fill the screen with input
-- mapped to match. Keep them opaque and keep the screen awake.
o.window({{ class = "^gamescope$" }}, {{
  tag = "-default-opacity",
  opacity = "1 1",
  idle_inhibit = "fullscreen",
}})

-- Games set to `display window` keep their native size: tiling a
-- fixed-resolution game stretches it and breaks its mouse input.
o.window({{ class = "^steam_app_omavn_.*" }}, {{
  float = true,
  center = true,
  tag = "-default-opacity",
  opacity = "1 1",
  idle_inhibit = "focus",
}})
{bind}"#
    )
}

fn menu_entries(omavn: &Path) -> Vec<(&'static str, String)> {
    let exe = omavn.display().to_string();
    let term = "omarchy-launch-floating-terminal-with-presentation";
    vec![
        (
            "visual-novels",
            format!(
                r#"{{"icon":"󰂺","label":"Visual Novels","aliases":["vn","vns","omavn"],"description":"Your visual novel library","action":"'{exe}' gui"}}"#
            ),
        ),
        (
            "install.gaming.visual-novel",
            format!(
                r#"{{"icon":"󰂺","label":"Visual Novel","action":"{term} \"'{exe}' install --pick\""}}"#
            ),
        ),
        (
            "remove.visual-novel",
            format!(
                r#"{{"icon":"󰂺","label":"Visual Novel","action":"{term} \"'{exe}' remove --pick\""}}"#
            ),
        ),
    ]
}

/// Add our routes to the user's menu extension file without disturbing its
/// formatting or comments.
fn add_menu_entries(omavn: &Path) -> Result<bool> {
    let path = menu_file();
    let original = fs::read_to_string(&path).unwrap_or_else(|_| "{\n}\n".into());
    // Refresh entries an older omavn wrote (each sits on its own line).
    let entries = menu_entries(omavn);
    let mut text: String = original
        .lines()
        .map(|line| {
            let current = entries
                .iter()
                .find(|(k, _)| line.trim_start().starts_with(&format!("\"{k}\":")));
            match current {
                Some((k, v)) => {
                    let comma = if line.trim_end().ends_with(',') {
                        ","
                    } else {
                        ""
                    };
                    format!("  \"{k}\": {v}{comma}")
                }
                None => line.to_string(),
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    if original.ends_with('\n') {
        text.push('\n');
    }
    if text != original {
        fs::write(&path, &text)?;
    }
    let missing: Vec<String> = menu_entries(omavn)
        .into_iter()
        .filter(|(k, _)| !text.contains(&format!("\"{k}\"")))
        .map(|(k, v)| format!("  \"{k}\": {v}"))
        .collect();
    if missing.is_empty() {
        return Ok(text != original);
    }
    let close = last_significant(&text, |c| c == '}')
        .context("menu extension file is not a JSON object")?;
    let before =
        last_significant(&text[..close], |c| !c.is_whitespace()).context("malformed menu file")?;
    let needs_comma = !text[before..].starts_with('{');
    let insert = format!(
        "{}\n{}\n",
        if needs_comma { "," } else { "" },
        missing.join(",\n")
    );
    let at = before + text[before..].chars().next().map_or(0, char::len_utf8);
    let new = format!("{}{}{}", &text[..at], insert, &text[close..]);
    fs::create_dir_all(path.parent().unwrap())?;
    fs::write(&path, new)?;
    Ok(true)
}

fn remove_menu_entries() -> Result<bool> {
    let path = menu_file();
    let Ok(text) = fs::read_to_string(&path) else {
        return Ok(false);
    };
    let keys = [
        "\"visual-novels\"",
        "\"install.gaming.visual-novel\"",
        "\"remove.visual-novel\"",
    ];
    let kept: Vec<&str> = text
        .lines()
        .filter(|l| !keys.iter().any(|k| l.trim_start().starts_with(k)))
        .collect();
    if kept.len() == text.lines().count() {
        return Ok(false);
    }
    // Dropping the last entries can leave a trailing comma before `}`.
    let mut joined = kept.join("\n");
    if let Some(close) = last_significant(&joined, |c| c == '}')
        && let Some(prev) = last_significant(&joined[..close], |c| !c.is_whitespace())
        && joined[prev..].starts_with(',')
    {
        joined.replace_range(prev..prev + 1, "");
    }
    fs::write(&path, joined + "\n")?;
    Ok(true)
}

/// Byte index of the last char matching `pred` outside comments and strings.
fn last_significant(text: &str, pred: impl Fn(char) -> bool) -> Option<usize> {
    let mut last = None;
    let mut chars = text.char_indices().peekable();
    let (mut in_str, mut escaped) = (false, false);
    while let Some((i, c)) = chars.next() {
        if in_str {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_str = false;
                if pred(c) {
                    last = Some(i);
                }
            }
            continue;
        }
        match (c, chars.peek().map(|p| p.1)) {
            ('/', Some('/')) => {
                for (_, c) in chars.by_ref() {
                    if c == '\n' {
                        break;
                    }
                }
                continue;
            }
            ('/', Some('*')) => {
                chars.next();
                while let Some((_, c)) = chars.next() {
                    if c == '*' && chars.peek().map(|p| p.1) == Some('/') {
                        chars.next();
                        break;
                    }
                }
                continue;
            }
            ('"', _) => in_str = true,
            _ => {}
        }
        if pred(c) {
            last = Some(i);
        }
    }
    last
}

fn write_if_changed(path: &Path, content: &str) -> Result<bool> {
    if fs::read_to_string(path).is_ok_and(|t| t == content) {
        return Ok(false);
    }
    fs::create_dir_all(path.parent().unwrap())?;
    fs::write(path, content)?;
    Ok(true)
}

fn run(cmd: &str, args: &[&str]) -> Result<()> {
    let status = Command::new(cmd)
        .args(args)
        .status()
        .with_context(|| format!("running {cmd}"))?;
    if !status.success() {
        bail!("{cmd} failed ({status})");
    }
    Ok(())
}

/// Desktop notification, with Omarchy's richer one when available.
/// `exec` runs when the notification is clicked.
/// `timeout_ms` of `None` leaves expiry to the notification daemon.
pub fn notify(
    headline: &str,
    body: &str,
    image: Option<&Path>,
    exec: &[&str],
    timeout_ms: Option<u32>,
) {
    if which("omarchy-notification-send").is_some() {
        let mut cmd = Command::new("omarchy-notification-send");
        cmd.args(["--app-name", "omavn", "-g", "󰂺", headline, body]);
        if let Some(t) = timeout_ms {
            cmd.arg("-t").arg(t.to_string());
        }
        if let Some(img) = image.filter(|p| p.exists()) {
            cmd.arg("--image").arg(img);
        }
        if !exec.is_empty() {
            cmd.arg("--exec").args(exec);
        }
        let _ = cmd.status();
    } else if which("notify-send").is_some() {
        let mut cmd = Command::new("notify-send");
        cmd.args(["--app-name=omavn", headline, body]);
        if let Some(t) = timeout_ms {
            cmd.arg(format!("--expire-time={t}"));
        }
        if let Some(img) = image.filter(|p| p.exists()) {
            cmd.arg("-i").arg(img);
        }
        let _ = cmd.status();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_last_brace_outside_comments() {
        let t = "{\n  \"a\": \"}\" // }\n}\n";
        assert_eq!(
            last_significant(t, |c| c == '}'),
            Some(t.rfind('}').unwrap())
        );
        let t = "{ /* } */ }";
        assert_eq!(last_significant(t, |c| c == '}'), Some(10));
    }
}
