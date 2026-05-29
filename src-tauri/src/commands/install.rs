use serde::Serialize;
use std::process::Command;

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
    let output = Command::new(cmd).arg(version_arg).output();

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
pub fn check_system() -> SystemCheck {
    SystemCheck {
        nodejs: check_command_version("node", "--version"),
        openclaw: check_command_version("openclaw", "--version"),
        platform: std::env::consts::OS.to_string(),
        is_china: check_china_network(),
    }
}

#[tauri::command]
pub fn install_openclaw(use_mirror: bool) -> Result<String, String> {
    let mut args = vec!["install", "-g", "openclaw"];

    if use_mirror {
        args.push("--registry=https://registry.npmmirror.com");
    }

    let output = Command::new("npm").args(&args).output();

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

#[derive(Serialize)]
pub struct UpdateCheck {
    pub has_update: bool,
    pub current_version: String,
    pub latest_version: Option<String>,
    pub message: String,
}

// About 页面用：检查 ClawBox 应用更新
#[tauri::command]
pub fn check_update() -> UpdateCheck {
    let current = env!("CARGO_PKG_VERSION");

    // TODO: 替换为你自己的 GitHub 仓库 URL，例如 "openclaw/clawbox"
    // 目前先显示当前版本，没有远程检查
    UpdateCheck {
        has_update: false,
        current_version: current.to_string(),
        latest_version: None,
        message: format!("Current version: {}", current),
    }
}

// 首页用：检查 openclaw CLI 更新
#[tauri::command]
pub fn check_openclaw_update() -> UpdateCheck {
    // 获取已安装的 openclaw CLI 版本
    let installed_output = Command::new("openclaw")
        .arg("--version")
        .output();

    let installed_version = installed_output
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "not installed".to_string());

    // 检查 npm 上的 openclaw 最新版本
    let latest_output = Command::new("npm")
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
