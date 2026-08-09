//! Child-process spawning that stays invisible on Windows.
//!
//! ClawBox is a GUI process, but nearly everything it does shells out: probing
//! ~16 agents at startup, `npm config get prefix`, installs, `git`. On Windows
//! each such spawn from a GUI (subsystem:windows) process allocates a fresh
//! console — the user sees a black box flash, ~16 of them in a row on launch.
//! `CREATE_NO_WINDOW` suppresses that console.
//!
//! Every `Command` in this crate must be built through [`command()`] rather
//! than `Command::new` so the flag can never be forgotten at a call site.

use std::ffi::OsStr;
use std::process::Command;

/// Don't allocate a console for the child (winbase.h CREATE_NO_WINDOW).
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// `Command::new` plus the platform hardening every ClawBox child needs.
/// On non-Windows this is exactly `Command::new`.
pub fn command<S: AsRef<OsStr>>(program: S) -> Command {
    #[allow(unused_mut)]
    let mut cmd = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

#[cfg(test)]
mod tests {
    #[test]
    fn command_runs_a_child_like_command_new() {
        // Behaviour parity is the whole contract: the flag must not change how
        // the child runs, only whether it gets a console.
        let prog = if cfg!(windows) { "cmd" } else { "sh" };
        let args: &[&str] = if cfg!(windows) { &["/C", "exit 0"] } else { &["-c", "exit 0"] };
        let status = super::command(prog).args(args).status().expect("spawn");
        assert!(status.success());
    }
}
