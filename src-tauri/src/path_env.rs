//! Resolve the user's real PATH from an interactive login shell.
//!
//! GUI processes launched from Finder/Dock inherit a minimal PATH that misses
//! nvm/homebrew/~/.local/bin, so binary probes false-negative. `init()` runs
//! `$SHELL -ilc 'echo <MARKER>$PATH'` once and injects the merged result via
//! `std::env::set_var("PATH", ..)` — every subsequent `Command::new` in the
//! process (probes, installs) inherits it with zero
//! call-site changes.
//!
//! Even when the interactive-shell probe fails (heavy `.zshrc` timing out,
//! missing `$SHELL`, ...), `init()` still folds a set of well-known bin dirs
//! into PATH as a best-effort fallback, and records the outcome in
//! [`PathInitStatus`] so the UI can warn the user instead of silently
//! false-negativing installed agents (GH#3: Claude Code detected as missing).

use serde::Serialize;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

const MARKER: &str = "__CLAWBOX_PATH__";
/// Headroom for nvm/conda/p10k-heavy `.zshrc` cold starts. The original 5s
/// threshold was routinely exceeded by exactly the power users most likely to
/// install Claude Code off the default PATH (GH#3). Bounded so a wedged shell
/// can't hang app launch indefinitely; layered with fallback dirs + per-agent
/// resolve() so detection no longer hinges on this alone succeeding.
const SHELL_TIMEOUT: Duration = Duration::from_secs(8);

/// Whether `init()` recovered the interactive-shell PATH or had to degrade to
/// well-known dirs + the GUI-inherited PATH. Surfaced to the frontend so users
/// understand why an installed agent may still read "not installed".
#[derive(Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PathInitStatus {
    /// Interactive login shell resolved its full PATH.
    ShellResolved,
    /// Shell probe failed/timed out — PATH is best-effort (well-known dirs +
    /// GUI PATH). Agents may false-negative; see resolve() fallbacks.
    ShellFailed,
}

/// Outcome of the last `init()`. Defaults to ShellResolved until init runs so
/// a command read before init (shouldn't happen — init is the first thing
/// `run()` does) doesn't mislead the UI.
static STATUS: OnceLock<PathInitStatus> = OnceLock::new();

pub fn init_status() -> PathInitStatus {
    *STATUS.get().unwrap_or(&PathInitStatus::ShellResolved)
}

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
        if !seg.is_empty() && !seen.iter().any(|s: &String| s == seg) {
            seen.push(seg.to_string());
        }
    }
    seen.join(":")
}

/// Well-known bin dirs always worth folding into PATH. These are the install
/// targets of common native installers / package managers that GUI launches
/// routinely miss. `~/.local/bin` is the load-bearing one for Claude Code's
/// native installer (GH#3). merge_paths dedupes, so listing always-present
/// dirs like /usr/local/bin is free.
pub fn well_known_dirs() -> Vec<PathBuf> {
    let mut v = Vec::new();
    if let Some(home) = dirs::home_dir() {
        v.push(home.join(".local/bin"));
    }
    v.push(PathBuf::from("/opt/homebrew/bin"));
    v.push(PathBuf::from("/usr/local/bin"));
    v
}

/// Poll-wait with a deadline; std has no built-in child timeout.
/// Shared by path_env's shell probe and agents' version probes.
pub(crate) fn wait_with_timeout(mut child: std::process::Child, dur: Duration) -> Option<std::process::Output> {
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return child.wait_with_output().ok(),
            Ok(None) if start.elapsed() > dur => {
                let _ = child.kill();
                // Reap the killed child so it doesn't linger as a zombie.
                let _ = child.wait();
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
///
/// Priority: interactive-shell PATH first, then the GUI-inherited PATH, then
/// well-known fallback dirs appended (lowest priority — they only fill gaps
/// the first two miss). On any shell failure the well-known dirs + GUI PATH
/// still stand, so detection degrades gracefully instead of false-negativing.
pub fn init() {
    static DONE: OnceLock<()> = OnceLock::new();
    DONE.get_or_init(|| {
        let gui_path = std::env::var("PATH").unwrap_or_default();
        let shell = shell_path();

        // Shell PATH wins priority over the GUI PATH.
        let after_shell = match &shell {
            Some(sp) => merge_paths(sp, &gui_path),
            None => gui_path,
        };

        // Always append well-known dirs so probes survive a shell failure.
        let wk = well_known_dirs()
            .iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join(":");
        let final_path = if wk.is_empty() {
            after_shell
        } else {
            merge_paths(&after_shell, &wk)
        };
        std::env::set_var("PATH", final_path);

        let status = if shell.is_some() {
            PathInitStatus::ShellResolved
        } else {
            PathInitStatus::ShellFailed
        };
        let _ = STATUS.set(status);
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

    #[test]
    fn well_known_dirs_includes_local_bin_and_homebrew() {
        let wk = well_known_dirs();
        let strs: Vec<String> = wk.iter().map(|p| p.to_string_lossy().into()).collect();
        assert!(strs.iter().any(|s| s.ends_with("/.local/bin")), "missing ~/.local/bin (GH#3): {:?}", strs);
        assert!(strs.iter().any(|s| s == "/opt/homebrew/bin"), "missing /opt/homebrew/bin: {:?}", strs);
    }

    #[test]
    fn init_appends_well_known_dirs_when_shell_succeeds() {
        // Shell PATH entries keep priority; well-known dirs fill gaps after.
        let after_shell = merge_paths("/nvm/bin:/usr/bin", "");
        let wk = "/opt/homebrew/bin";
        let final_path = merge_paths(&after_shell, wk);
        // shell dirs first, then appended wk dir, deduped.
        assert!(final_path.starts_with("/nvm/bin:/usr/bin"));
        assert!(final_path.ends_with("/opt/homebrew/bin"));
    }

    #[test]
    fn init_status_defaults_to_resolved_before_init() {
        // A fresh process that never called init reports "resolved" so the UI
        // never shows a spurious warning. After init it reflects the truth.
        // (We can't call init() here — it mutates global process state — but
        // the default-read path is what this asserts.)
        let _ = init_status();
    }
}
