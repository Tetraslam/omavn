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

/// Whether 32-bit Vulkan drivers are installed. Most visual novels are
/// 32-bit, and without these DXVK can't render them.
pub fn lib32_vulkan() -> bool {
    let dirs = ["/usr/lib32", "/usr/lib/i386-linux-gnu", "/usr/lib"];
    let loader = dirs
        .iter()
        .any(|d| std::path::Path::new(d).join("libvulkan.so.1").exists());
    let drivers = [
        "libGLX_nvidia.so.0",
        "libvulkan_intel.so",
        "libvulkan_radeon.so",
        "libvulkan_lvp.so",
    ];
    // /usr/lib is 64-bit on Arch and Debian, so only trust it for the
    // loader check on distros that keep 32-bit libraries there (Fedora).
    let driver = dirs[..2].iter().any(|d| {
        drivers
            .iter()
            .any(|f| std::path::Path::new(d).join(f).exists())
    }) || std::path::Path::new("/usr/lib64").exists()
        && drivers
            .iter()
            .any(|f| std::path::Path::new("/usr/lib").join(f).exists());
    loader && driver
}

/// How to install 32-bit Vulkan drivers on this system.
pub fn lib32_vulkan_hint() -> &'static str {
    if crate::omarchy::detected() {
        "run: omavn omarchy setup"
    } else if crate::runtime::which("pacman").is_some() {
        "enable [multilib] and install lib32-vulkan-icd-loader plus lib32-nvidia-utils, lib32-vulkan-radeon, or lib32-vulkan-intel"
    } else if crate::runtime::which("apt").is_some() {
        "run: sudo dpkg --add-architecture i386 && sudo apt update && sudo apt install mesa-vulkan-drivers:i386 libvulkan1:i386"
    } else {
        "install your distro's 32-bit Vulkan loader and GPU driver packages"
    }
}
