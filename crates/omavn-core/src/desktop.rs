//! Facts about the user's desktop session.

use std::process::Command;

pub fn has_display() -> bool {
    std::env::var_os("WAYLAND_DISPLAY").is_some() || std::env::var_os("DISPLAY").is_some()
}

pub fn is_hyprland() -> bool {
    std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_some()
}

/// Native pixel size of the focused monitor, from the compositor.
pub fn focused_output_size() -> Option<(u32, u32)> {
    if is_hyprland() {
        let out = Command::new("hyprctl")
            .args(["-j", "monitors"])
            .output()
            .ok()?;
        let monitors: Vec<serde_json::Value> = serde_json::from_slice(&out.stdout).ok()?;
        let m = monitors
            .iter()
            .find(|m| m["focused"] == true)
            .or(monitors.first())?;
        return Some((m["width"].as_u64()? as u32, m["height"].as_u64()? as u32));
    }
    if std::env::var_os("SWAYSOCK").is_some() {
        let out = Command::new("swaymsg")
            .args(["-r", "-t", "get_outputs"])
            .output()
            .ok()?;
        let outputs: Vec<serde_json::Value> = serde_json::from_slice(&out.stdout).ok()?;
        let o = outputs
            .iter()
            .find(|o| o["focused"] == true)
            .or(outputs.first())?;
        let mode = &o["current_mode"];
        return Some((
            mode["width"].as_u64()? as u32,
            mode["height"].as_u64()? as u32,
        ));
    }
    None
}
