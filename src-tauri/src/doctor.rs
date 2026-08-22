//! 一键体检(spec: docs/superpowers/specs/2026-08-14-doctor-design.md)。
//!
//! 只读报告:PATH / 运行时依赖 / 绑定孤儿 / 配置漂移(本地四项,本模块
//! `local_checks` 全量可测)+ Provider 拨测与后端网关(`network_checks` /
//! `backend_checks`,真实网络与 CLI,保持薄)。UI 层按 check id 走 i18n,
//! 后端 title/detail 为兜底文案。

use crate::agents::AgentStatus;
use crate::commands::config::Config;
use crate::commands::sync::agent_sync_overview_at;
use crate::path_env::PathInitStatus;
use serde::Serialize;
use std::path::Path;

/// 单项体检结果。status 驱动前端图标与着色;hint 为修复提示(可空)。
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct DoctorCheck {
    pub id: String,
    /// 兜底标题(前端优先用 i18n,键 = `agents.doctor.<id>.title`)。
    pub title: String,
    /// ok | warn | error | info
    pub status: String,
    /// 动态明细(缺依赖清单、漂移 agent 名等),原样展示。
    pub detail: String,
    pub hint: Option<String>,
}

impl DoctorCheck {
    fn new(id: &str, title: &str, status: &str, detail: impl Into<String>) -> Self {
        Self {
            id: id.to_string(),
            title: title.to_string(),
            status: status.to_string(),
            detail: detail.into(),
            hint: None,
        }
    }

    fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }
}

/// 完整体检报告。
#[derive(Serialize, Debug)]
pub struct DoctorReport {
    pub checks: Vec<DoctorCheck>,
    /// RFC3339,前端原样展示。
    pub ran_at: String,
}

/// PATH 初始化状态 → 检查项。ShellFailed 时 agent 可能被误报未安装。
pub fn path_check(status: PathInitStatus) -> DoctorCheck {
    match status {
        PathInitStatus::ShellResolved => {
            DoctorCheck::new("path", "PATH", "ok", "interactive shell PATH resolved")
        }
        PathInitStatus::ShellFailed => DoctorCheck::new(
            "path",
            "PATH",
            "warn",
            "interactive shell PATH not resolved; using well-known dirs fallback",
        )
        .with_hint("installed agents may be reported missing; check your shell startup files"),
    }
}

/// 已安装 agent 的运行时依赖(node 等)。未安装的 agent 不查(装时会查)。
pub fn deps_check(statuses: &[AgentStatus]) -> DoctorCheck {
    let missing: Vec<String> = statuses
        .iter()
        .filter(|a| a.installed)
        .flat_map(|a| a.missing_deps.iter().map(move |d| format!("{}: {}", a.label, d)))
        .collect();
    if missing.is_empty() {
        DoctorCheck::new("deps", "Runtime dependencies", "ok", "all satisfied")
    } else {
        DoctorCheck::new("deps", "Runtime dependencies", "error", missing.join("\n"))
            .with_hint("install the missing runtimes, then refresh")
    }
}

/// 绑定孤儿:agent_providers 指向未安装的 agent(绑定还在,人走了)。
pub fn orphan_bindings_check(config: &Config, statuses: &[AgentStatus]) -> DoctorCheck {
    let installed = |id: &str| statuses.iter().any(|a| a.id == id && a.installed);
    let orphans: Vec<String> = config
        .agent_providers
        .keys()
        .filter(|id| !installed(id))
        .cloned()
        .collect();
    if orphans.is_empty() {
        DoctorCheck::new("orphans", "Provider bindings", "ok", "no orphan bindings")
    } else {
        DoctorCheck::new("orphans", "Provider bindings", "warn", orphans.join(", "))
            .with_hint("install the agent or rebind on the Agents page")
    }
}

/// 配置漂移:绑定了服务商的 agent,其下发内容与 ClawBox 记录不一致
/// (state outdated/removing)。重新同步即可修复,且有快照兜底。
pub fn drift_check(home: &Path, config: &Config) -> DoctorCheck {
    let mut drifted: Vec<String> = Vec::new();
    for o in agent_sync_overview_at(home, config) {
        if !config.agent_providers.contains_key(&o.agent_id) {
            continue; // 未绑定 = 不管理,漂移不归我们管
        }
        if o.providers.iter().any(|p| p.state == "outdated" || p.state == "removing") {
            drifted.push(o.agent_id);
        }
    }
    if drifted.is_empty() {
        DoctorCheck::new("drift", "Config drift", "ok", "all synced configs match")
    } else {
        DoctorCheck::new("drift", "Config drift", "warn", drifted.join(", "))
            .with_hint("re-sync on the Agents page (snapshots are taken automatically)")
    }
}

/// 本地四项(PATH / 依赖 / 孤儿绑定 / 漂移)。纯本地计算,全量可测。
pub fn local_checks(home: &Path, config: &Config, statuses: &[AgentStatus]) -> Vec<DoctorCheck> {
    vec![
        path_check(crate::path_env::init_status()),
        deps_check(statuses),
        orphan_bindings_check(config, statuses),
        drift_check(home, config),
    ]
}

/// Provider 连通性:每个启用 provider 的已配端点(anthropic/openai 各一)
/// 并发拨测(单项 8s 超时在 `test_endpoint` 内)。离线(全部网络错误)时
/// 整体降级 info;无已配端点 → info;任一真失败 → error。
/// 真实网络,不单测,保持薄。
pub async fn network_checks(config: &Config) -> Vec<DoctorCheck> {
    // (provider 名, 端点, key, flavor)
    let mut targets: Vec<(String, String, String, String)> = Vec::new();
    for p in config.providers.iter().filter(|p| p.enabled) {
        if !p.anthropic_base_url.trim().is_empty() {
            targets.push((
                p.name.clone(),
                p.anthropic_base_url.clone(),
                p.api_key.clone(),
                "anthropic".into(),
            ));
        }
        if !p.openai_base_url.trim().is_empty() {
            targets.push((
                p.name.clone(),
                p.openai_base_url.clone(),
                p.api_key.clone(),
                "openai".into(),
            ));
        }
    }
    if targets.is_empty() {
        return vec![DoctorCheck::new(
            "providers",
            "Provider connectivity",
            "info",
            "no enabled provider endpoints configured",
        )];
    }

    let tasks: Vec<_> = targets
        .iter()
        .map(|(name, url, key, flavor)| {
            let (name, url, key, flavor) = (name.clone(), url.clone(), key.clone(), flavor.clone());
            tokio::spawn(async move {
                let r = crate::commands::provider_test::test_endpoint(&url, &key, &flavor)
                    .await
                    .unwrap_or_else(|e| ProviderTestResult {
                        ok: false,
                        latency_ms: 0,
                        models: vec![],
                        error: Some(e),
                    });
                (name, url, flavor, r)
            })
        })
        .collect();
    let mut ok_lines = Vec::new();
    let mut fail_lines = Vec::new();
    let mut network_error = false;
    let mut real_failure = false;
    for t in tasks {
        let Ok((name, url, flavor, r)) = t.await else { continue };
        let line = format!("{} · {}: {} ({}ms)", name, flavor, url, r.latency_ms);
        if r.ok {
            ok_lines.push(line);
        } else {
            let err = r.error.unwrap_or_default();
            let is_net = err.starts_with("Network error") || err.starts_with("Request timed out");
            network_error |= is_net;
            real_failure |= !is_net;
            fail_lines.push(format!("{} — {}", line, err));
        }
    }

    let check = if real_failure {
        DoctorCheck::new(
            "providers",
            "Provider connectivity",
            "error",
            fail_lines.join("\n"),
        )
        .with_hint("check Base URL / API key; retest on the Providers page")
    } else if network_error {
        // 全部网络错误 → 离线,整体降级跳过
        DoctorCheck::new(
            "providers",
            "Provider connectivity",
            "info",
            "network unreachable, skipped",
        )
    } else {
        DoctorCheck::new("providers", "Provider connectivity", "ok", ok_lines.join("\n"))
    };
    vec![check]
}

/// 后端网关:已安装后端的 gateway_status Err → warn;无已安装后端 →
/// info。真实 CLI 探测,不单测,保持薄。
pub fn backend_checks() -> Vec<DoctorCheck> {
    let installed: Vec<_> = crate::backends::backends()
        .iter()
        .filter(|b| b.is_installed())
        .collect();
    if installed.is_empty() {
        return vec![DoctorCheck::new(
            "gateways",
            "Backend gateways",
            "info",
            "no backends installed",
        )];
    }
    let mut errors = Vec::new();
    let mut ok_lines = Vec::new();
    for b in installed {
        match b.gateway_status() {
            Ok(s) => ok_lines.push(format!("{}: {}", b.display_name(), s.status)),
            Err(e) => errors.push(format!("{}: {}", b.display_name(), e)),
        }
    }
    if errors.is_empty() {
        vec![DoctorCheck::new("gateways", "Backend gateways", "ok", ok_lines.join(", "))]
    } else {
        vec![DoctorCheck::new("gateways", "Backend gateways", "warn", errors.join("\n"))
            .with_hint("the openclaw/hermes gateway is not running or its CLI failed")]
    }
}

use crate::commands::provider_test::ProviderTestResult;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::AgentKind;
    use crate::sync::test_util::TempHome;
    use std::collections::HashMap;

    fn status(id: &str, installed: bool, missing: &[&str]) -> AgentStatus {
        AgentStatus {
            id: id.to_string(),
            label: id.to_string(),
            kind: AgentKind::NativeCli,
            installed,
            version: None,
            deps_satisfied: missing.is_empty(),
            missing_deps: missing.iter().map(|s| s.to_string()).collect(),
            install_command: None,
            docs_url: None,
        }
    }

    #[test]
    fn path_check_maps_both_states() {
        assert_eq!(path_check(PathInitStatus::ShellResolved).status, "ok");
        let w = path_check(PathInitStatus::ShellFailed);
        assert_eq!(w.status, "warn");
        assert!(w.hint.is_some());
    }

    #[test]
    fn deps_check_only_looks_at_installed_agents() {
        let sts = vec![
            status("claude-code", true, &[]),
            status("codex", false, &["node"]), // 未安装不查
        ];
        assert_eq!(deps_check(&sts).status, "ok");

        let sts = vec![status("claude-code", true, &["node"])];
        let c = deps_check(&sts);
        assert_eq!(c.status, "error");
        assert!(c.detail.contains("claude-code: node"), "{}", c.detail);
    }

    #[test]
    fn orphan_bindings_flag_uninstalled_bound_agents() {
        let mut config = Config::default();
        config.agent_providers.insert("kimi".to_string(), "p1".to_string());
        // kimi 未安装 → 孤儿;claude-code 已装且绑了 → 不算
        config
            .agent_providers
            .insert("claude-code".to_string(), "p1".to_string());
        let sts = vec![status("claude-code", true, &[]), status("kimi", false, &[])];
        let c = orphan_bindings_check(&config, &sts);
        assert_eq!(c.status, "warn");
        assert_eq!(c.detail, "kimi");

        config.agent_providers.clear();
        assert_eq!(orphan_bindings_check(&config, &sts).status, "ok");
    }

    #[test]
    fn drift_check_detects_tampered_provider_config() {
        let home = TempHome::new();
        let mut config = Config::default();
        config.providers = vec!(crate::commands::config::ProviderSpec {
            id: "p-oa".to_string(),
            name: "OA".to_string(),
            api_key: "sk-x".to_string(),
            base_url: String::new(),
            anthropic_base_url: String::new(),
            openai_base_url: "https://api.oa.example.com/v1".to_string(),
            default_model: "gpt-test".to_string(),
            models: vec![],
            enabled: true,
            flavor: None,
        });
        config.agent_providers.insert("codex".to_string(), "p-oa".to_string());

        // 未下发过 → overview 无 drift 条目(unsynced 不算漂移)
        assert_eq!(drift_check(home.path(), &config).status, "ok");

        // 下发并记录 managed → synced;随后清空 config.toml → outdated
        crate::sync::providers::adapters()
            .iter()
            .find(|a| a.agent_id() == "codex")
            .unwrap()
            .apply(home.path(), &config.providers, Some("p-oa"), &[])
            .unwrap();
        let mut managed: HashMap<String, Vec<String>> = HashMap::new();
        managed.insert("codex".to_string(), vec!["clawbox".to_string()]);
        config.providers_managed = managed;
        assert_eq!(drift_check(home.path(), &config).status, "ok");

        // 手改 base_url(表还在、值变了)→ update → outdated;整体清空会
        // 被判为 add(unsynced),不算漂移,不归 doctor 管
        let cfg_path = home.path().join(".codex").join("config.toml");
        let tampered = std::fs::read_to_string(&cfg_path)
            .unwrap()
            .replace("https://api.oa.example.com/v1", "https://tampered.example.com/v1");
        std::fs::write(&cfg_path, tampered).unwrap();
        let c = drift_check(home.path(), &config);
        assert_eq!(c.status, "warn", "{}", c.detail);
        assert!(c.detail.contains("codex"), "{}", c.detail);
    }
}
