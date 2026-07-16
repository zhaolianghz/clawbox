//! Resolve the user's real PATH from an interactive login shell.
//!
//! GUI processes launched from Finder/Dock inherit a minimal PATH that misses
//! nvm/homebrew/~/.local/bin, so binary probes false-negative. `init()` runs
//! `$SHELL -ilc 'echo <MARKER>$PATH'` once and injects the merged result via
//! `std::env::set_var("PATH", ..)` — every subsequent `Command::new` in the
//! process (probes, installs, ACP bridge spawns) inherits it with zero
//! call-site changes.

use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

const MARKER: &str = "__CLAWBOX_PATH__";
const SHELL_TIMEOUT: Duration = Duration::from_secs(5);

/// Extract the PATH from marker-prefixed shell output. Interactive shells may
/// print banners/prompt noise; the last marker line wins.
pub fn parse_marker_output(out: &str) -> Option<String> {
    out.lines()
        .rev()
        .find_map(|l| l.trim().strip_prefix(MARKER))
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty())
}

/// Shell PATH first, then any current-PATH entries not already present.
pub fn merge_paths(shell_path: &str, current: &str) -> String {
    let mut seen = Vec::new();
    for seg in shell_path.split(':').chain(current.split(':')) {
        if !seg.is_empty() && !seen.iter().any(|s| s == seg) {
            seen.push(seg.to_string());
        }
    }
    seen.join(":")
}

/// Poll-wait with a deadline; std has no built-in child timeout.
fn wait_with_timeout(mut child: std::process::Child, dur: Duration) -> Option<std::process::Output> {
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return child.wait_with_output().ok(),
            Ok(None) if start.elapsed() > dur => {
                let _ = child.kill();
                return None;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(100)),
            Err(_) => return None,
        }
    }
}

fn shell_path() -> Option<String> {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());
    // -i -l: interactive login shell so BOTH .zprofile and .zshrc run (nvm
    // typically lives in .zshrc; a plain `-lc` would miss it).
    let child = Command::new(&shell)
        .args(["-ilc", &format!("echo {}$PATH", MARKER)])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let out = wait_with_timeout(child, SHELL_TIMEOUT)?;
    parse_marker_output(&String::from_utf8_lossy(&out.stdout))
}

/// Resolve and inject the real PATH. Idempotent; call once at startup BEFORE
/// any threads spawn (edition-2021 `set_var` is safe but not thread-safe).
/// On any failure the current PATH is left untouched (fail-open: dev launches
/// from a terminal already have a good PATH).
pub fn init() {
    static DONE: OnceLock<()> = OnceLock::new();
    DONE.get_or_init(|| {
        if let Some(sp) = shell_path() {
            let current = std::env::var("PATH").unwrap_or_default();
            std::env::set_var("PATH", merge_paths(&sp, &current));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_extracts_marker_line_amid_noise() {
        let out = "welcome banner\n__CLAWBOX_PATH__/usr/bin:/opt/homebrew/bin\n";
        assert_eq!(
            parse_marker_output(out).as_deref(),
            Some("/usr/bin:/opt/homebrew/bin")
        );
    }

    #[test]
    fn parse_returns_none_without_marker_or_empty() {
        assert_eq!(parse_marker_output("no marker here"), None);
        assert_eq!(parse_marker_output("__CLAWBOX_PATH__\n"), None);
    }

    #[test]
    fn parse_takes_last_marker_line() {
        // .zshrc echoing the marker string is pathological but cheap to defend:
        // the real `echo` runs last.
        let out = "__CLAWBOX_PATH__stale\nprompt noise\n__CLAWBOX_PATH__/real/bin\n";
        assert_eq!(parse_marker_output(out).as_deref(), Some("/real/bin"));
    }

    #[test]
    fn merge_dedupes_and_keeps_shell_path_first() {
        let merged = merge_paths("/a:/b:/usr/bin", "/usr/bin:/c");
        assert_eq!(merged, "/a:/b:/usr/bin:/c");
    }

    #[test]
    fn merge_skips_empty_segments() {
        assert_eq!(merge_paths("/a::/b", ""), "/a:/b");
    }
}
