//! Install execution for registry agents. Pure command construction is split
//! from execution so it can be unit-tested without touching the system.

use super::{AgentDef, InstallMethod};

pub fn build_install_args(def: &AgentDef) -> Result<(String, Vec<String>), String> {
    match def.install {
        InstallMethod::Npm { package, force } => {
            let mut args = vec!["install".to_string(), "-g".to_string()];
            if force {
                args.push("--force".to_string());
            }
            args.push(package.to_string());
            Ok(("npm".to_string(), args))
        }
        InstallMethod::Script { unix, windows } => {
            if cfg!(windows) {
                // Windows: PowerShell 下载并执行安装脚本
                Ok((
                    "powershell".to_string(),
                    vec![
                        "-NoProfile".to_string(),
                        "-Command".to_string(),
                        format!("irm {} | iex", windows),
                    ],
                ))
            } else {
                // Unix: pipefail 保证 curl 失败时 bash 不会因空 stdin 误报成功
                Ok((
                    "bash".to_string(),
                    vec![
                        "-c".to_string(),
                        format!("set -o pipefail; curl -fsSL {} | bash", unix),
                    ],
                ))
            }
        }
        InstallMethod::PlatformPkg => Err(format!(
            "{} installs via the platform package manager (handled by install_nodejs)",
            def.id
        )),
        InstallMethod::DetectOnly => Err(format!(
            "{} cannot be auto-installed; install it manually",
            def.id
        )),
    }
}

/// Blocking install; callers wrap in spawn_blocking. PATH was already fixed
/// process-wide by path_env::init(), so npm/bash resolve like a user shell.
pub fn run_install(def: &AgentDef) -> Result<String, String> {
    let (cmd, args) = build_install_args(def)?;
    let out = std::process::Command::new(&cmd)
        .args(&args)
        .output()
        .map_err(|e| format!("failed to run {}: {}", cmd, e))?;
    if out.status.success() {
        Ok(format!("Installed {}", def.label))
    } else {
        // npm/installer errors (incl. EACCES permission hints) go back verbatim.
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::find_agent;

    #[test]
    fn npm_install_args() {
        let (cmd, args) = build_install_args(find_agent("claude-code").unwrap()).unwrap();
        assert_eq!(cmd, "npm");
        assert_eq!(args, vec!["install", "-g", "@anthropic-ai/claude-code"]);
    }

    #[test]
    fn npm_force_install_args() {
        // No registry entry uses force anymore; construct a local def so the
        // force branch stays covered.
        let def = AgentDef {
            id: "force-test", label: "Force Test", binary: "force-test",
            kind: crate::agents::AgentKind::NativeCli,
            install: InstallMethod::Npm { package: "some-forced-pkg", force: true },
            check_probe: &["--version"], depends_on: &[],
            docs_url: None,
        };
        let (cmd, args) = build_install_args(&def).unwrap();
        assert_eq!(cmd, "npm");
        assert_eq!(args, vec!["install", "-g", "--force", "some-forced-pkg"]);
    }

    #[test]
    fn script_install_pipes_curl_to_bash() {
        let (cmd, args) = build_install_args(find_agent("kimi").unwrap()).unwrap();
        assert_eq!(cmd, "bash");
        assert_eq!(args, vec![
            "-c".to_string(),
            "set -o pipefail; curl -fsSL https://code.kimi.com/kimi-code/install.sh | bash".to_string(),
        ]);
    }

    #[test]
    fn platform_pkg_is_not_buildable() {
        // node 走平台包管理器 (install_nodejs),不经 run_install。
        assert!(build_install_args(find_agent("node").unwrap()).is_err());
    }
}
