use serde::Serialize;

#[derive(Serialize)]
pub struct SystemCheck {
    pub nodejs: ComponentStatus,
    pub openclaw: ComponentStatus,
    pub platform: String,
    pub is_china: bool,
}

#[derive(Serialize)]
pub struct ComponentStatus {
    pub installed: bool,
    pub version: Option<String>,
}

fn check_command_version(cmd: &str, version_arg: &str) -> ComponentStatus {
    let output = crate::proc::command(cmd).arg(version_arg).output();

    match output {
        Ok(output) if output.status.success() => {
            let version = String::from_utf8_lossy(&output.stdout)
                .trim()
                .lines()
                .next()
                .map(|s| s.to_string());
            ComponentStatus {
                installed: true,
                version,
            }
        }
        _ => ComponentStatus {
            installed: false,
            version: None,
        },
    }
}

fn check_china_network() -> bool {
    std::env::var("LANG")
        .map(|l| l.starts_with("zh"))
        .unwrap_or(false)
}

#[tauri::command]
pub async fn check_system() -> SystemCheck {
    SystemCheck {
        nodejs: check_command_version("node", "--version"),
        openclaw: check_command_version("openclaw", "--version"),
        platform: std::env::consts::OS.to_string(),
        is_china: check_china_network(),
    }
}

#[tauri::command]
pub async fn install_openclaw(use_mirror: bool) -> Result<String, String> {
    let mut args = vec!["install", "-g", "openclaw"];

    if use_mirror {
        args.push("--registry=https://registry.npmmirror.com");
    }

    let output = crate::proc::command("npm").args(&args).output();

    match output {
        Ok(output) if output.status.success() => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            Ok(format!("Installed openclaw: {}", stdout.trim()))
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(format!("Failed to install openclaw: {}", stderr))
        }
        Err(e) => Err(format!("Failed to install openclaw: {}", e)),
    }
}

/// Install Node.js via the platform package manager. Elevation-free managers
/// only (brew/winget) — anything else needs a manual install from nodejs.org.
#[tauri::command]
pub async fn install_nodejs() -> Result<String, String> {
    let (cmd, args): (&str, &[&str]) = match std::env::consts::OS {
        "macos" => ("brew", &["install", "node"]),
        "windows" => (
            "winget",
            &["install", "--id", "OpenJS.NodeJS.LTS", "--silent"],
        ),
        _ => {
            return Err(
                "Automatic Node.js install is not supported on this platform. \
                 Please install Node.js from https://nodejs.org"
                    .to_string(),
            )
        }
    };

    let output = crate::proc::command(cmd).args(args).output();

    match output {
        Ok(output) if output.status.success() => {
            let version = check_command_version("node", "--version")
                .version
                .unwrap_or_else(|| "unknown".to_string());
            Ok(format!("Installed Node.js {}", version))
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(format!("Failed to install Node.js: {}", stderr.trim()))
        }
        Err(_) => Err(format!(
            "Failed to install Node.js: `{}` is not available. \
             Please install Node.js from https://nodejs.org",
            cmd
        )),
    }
}

#[derive(Serialize)]
pub struct UpdateCheck {
    pub has_update: bool,
    pub current_version: String,
    pub latest_version: Option<String>,
    pub message: String,
}

// About 页面用：检查 ClawBox 应用更新
//
// Set to Some("owner/repo") once ClawBox has a public GitHub repo; None keeps
// the check local-only (just reports the current version).
const GITHUB_REPO: Option<&str> = None;

#[tauri::command]
pub async fn check_update() -> UpdateCheck {
    let current = env!("CARGO_PKG_VERSION");

    let Some(repo) = GITHUB_REPO else {
        return UpdateCheck {
            has_update: false,
            current_version: current.to_string(),
            latest_version: None,
            message: format!("Current version: {}", current),
        };
    };

    match fetch_latest_release_tag(repo) {
        Some(latest) => {
            let latest_clean = latest.trim_start_matches('v').to_string();
            let has_update = latest_clean != current;
            UpdateCheck {
                has_update,
                current_version: current.to_string(),
                latest_version: Some(latest_clean.clone()),
                message: if has_update {
                    format!("New version {} available! (Current: {})", latest_clean, current)
                } else {
                    "You're up to date!".to_string()
                },
            }
        }
        None => UpdateCheck {
            has_update: false,
            current_version: current.to_string(),
            latest_version: None,
            message: "Unable to check for updates".to_string(),
        },
    }
}

// GitHub Releases 最新 tag（用系统 curl，避免引入 HTTP 客户端依赖）
fn fetch_latest_release_tag(repo: &str) -> Option<String> {
    let url = format!("https://api.github.com/repos/{}/releases/latest", repo);
    let output = crate::proc::command("curl")
        .args(["-fsSL", "--max-time", "10", &url])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let body: serde_json::Value = serde_json::from_slice(&output.stdout).ok()?;
    body.get("tag_name")?.as_str().map(|s| s.to_string())
}

// 首页用：检查 openclaw CLI 更新
#[tauri::command]
pub async fn check_openclaw_update() -> UpdateCheck {
    // 获取已安装的 openclaw CLI 版本
    let installed_output = crate::proc::command("openclaw")
        .arg("--version")
        .output();

    let installed_version = installed_output
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "not installed".to_string());

    // 检查 npm 上的 openclaw 最新版本
    let latest_output = crate::proc::command("npm")
        .args(["view", "openclaw", "version"])
        .output();

    match latest_output {
        Ok(output) if output.status.success() => {
            let latest = String::from_utf8_lossy(&output.stdout).trim().to_string();
            // 提取版本号进行比较 (去掉 "OpenClaw " 前缀)
            let installed_clean = installed_version
                .strip_prefix("OpenClaw ")
                .unwrap_or(&installed_version)
                .split(' ')
                .next()
                .unwrap_or(&installed_version)
                .trim()
                .to_string();
            let has_update = installed_clean != latest && installed_version != "not installed";
            let not_installed = installed_version == "not installed";
            UpdateCheck {
                has_update,
                current_version: installed_version.clone(),
                latest_version: Some(latest.clone()),
                message: if has_update {
                    format!("New version {} available! (Current: {})", latest, installed_clean)
                } else if not_installed {
                    "OpenClaw CLI is not installed".to_string()
                } else {
                    "You're up to date!".to_string()
                },
            }
        }
        _ => UpdateCheck {
            has_update: false,
            current_version: installed_version.clone(),
            latest_version: None,
            message: if installed_version == "not installed" {
                "OpenClaw CLI is not installed".to_string()
            } else {
                "Unable to check for updates".to_string()
            },
        },
    }
}
