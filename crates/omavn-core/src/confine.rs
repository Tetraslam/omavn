//! Keep big installs from crowding out the desktop.
//!
//! Unpacking a game means reading and writing tens of gigabytes. Even with
//! the page-cache hints in [`crate::archive`], a Windows installer running
//! beside it can push a desktop into memory pressure, and systemd-oomd then
//! kills the biggest app (usually the browser). Where systemd is available,
//! omavn reruns itself in a transient scope with a memory ceiling sized
//! from the machine's RAM, so pressure stays inside the install.

use std::fs;
use std::path::Path;
use std::process::{Command, ExitStatus, Stdio};

use crate::runtime::which;

const MARKER: &str = "OMAVN_CONFINED";

/// Rerun this process inside a memory-capped systemd scope and return its
/// exit status, or `None` to carry on unconfined: already confined, opted
/// out with `OMAVN_CONFINE=0`, already under a memory limit, or no systemd
/// user manager to ask (containers, non-systemd distros).
pub fn rerun_confined() -> Option<ExitStatus> {
    if std::env::var_os(MARKER).is_some() || std::env::var("OMAVN_CONFINE").is_ok_and(|v| v == "0")
    {
        return None;
    }
    if already_limited() || !scopes_work() {
        return None;
    }
    let systemd_run = which("systemd-run")?;
    let (high, max) = limits(mem_total()?);
    let exe = std::env::current_exe().ok()?;
    Command::new(systemd_run)
        .args(["--user", "--scope", "--quiet", "--collect"])
        .arg(format!("-pMemoryHigh={high}"))
        .arg(format!("-pMemoryMax={max}"))
        .arg("--")
        .arg(exe)
        .args(std::env::args_os().skip(1))
        .env(MARKER, "1")
        .status()
        .ok()
}

/// High: an eighth of RAM, between 1.5 and 4 GiB; the cache an unpack
/// builds up gets reclaimed above it (omavn itself needs a few MB). Max: twice that, a hard stop that
/// only a runaway installer would reach.
pub fn limits(total: u64) -> (u64, u64) {
    let gib = 1u64 << 30;
    let high = (total / 8).clamp(3 * gib / 2, 4 * gib);
    (high, high * 2)
}

fn mem_total() -> Option<u64> {
    let text = fs::read_to_string("/proc/meminfo").ok()?;
    let kb: u64 = text
        .lines()
        .find(|l| l.starts_with("MemTotal:"))?
        .split_whitespace()
        .nth(1)?
        .parse()
        .ok()?;
    Some(kb * 1024)
}

/// Whether our cgroup (or one above it) already caps memory.
fn already_limited() -> bool {
    let Ok(cg) = fs::read_to_string("/proc/self/cgroup") else {
        return false;
    };
    let Some(path) = cg.lines().find_map(|l| l.strip_prefix("0::")) else {
        return false;
    };
    let mut dir = Path::new("/sys/fs/cgroup").join(path.trim_start_matches('/'));
    loop {
        for file in ["memory.max", "memory.high"] {
            if fs::read_to_string(dir.join(file)).is_ok_and(|v| v.trim() != "max") {
                return true;
            }
        }
        if !dir.pop() || dir == Path::new("/sys/fs/cgroup") {
            return false;
        }
    }
}

/// Whether the user's systemd manager will start a memory-capped scope.
/// A trial run catches every way it can't: no systemd, no user bus, no
/// memory controller delegated to the user.
fn scopes_work() -> bool {
    let Some(systemd_run) = which("systemd-run") else {
        return false;
    };
    Command::new(systemd_run)
        .args([
            "--user",
            "--scope",
            "--quiet",
            "--collect",
            "-pMemoryMax=1G",
            "true",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limits_scale_with_ram() {
        let gib = 1u64 << 30;
        assert_eq!(limits(64 * gib), (4 * gib, 8 * gib));
        assert_eq!(limits(16 * gib), (2 * gib, 4 * gib));
        assert_eq!(limits(4 * gib), (3 * gib / 2, 3 * gib));
    }
}
