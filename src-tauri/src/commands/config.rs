use crate::sync::ApplyResult;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

fn default_true() -> bool {
    true
}

/// Canonical MCP server spec — ClawBox's single source of truth. Adapters in
/// `crate::sync` translate this into each agent's native config format.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct McpServerSpec {
    /// "stdio" | "http"
    pub kind: String,
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

/// Model provider entry. camelCase on the wire so the frontend `ModelProvider`
/// type maps field-for-field with zero conversion.
///
/// 双端点模型:一家服务商可同时有 Anthropic/OpenAI 兼容端点(如 MiniMax),
/// 各 agent 适配器按自己的协议取槽(见 `crate::sync::providers`)。旧单端点
/// 字段 baseUrl/flavor 仅保留读兼容,`load_config` 归一化后恒为空、不落盘。
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderSpec {
    pub id: String,
    pub name: String,
    pub api_key: String,
    /// 旧单端点字段(读兼容;归一化后恒空,序列化省略)。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub base_url: String,
    /// Anthropic 兼容端点;空 = 未配置。
    #[serde(default)]
    pub anthropic_base_url: String,
    /// OpenAI 兼容端点;空 = 未配置。
    #[serde(default)]
    pub openai_base_url: String,
    #[serde(default)]
    pub default_model: String,
    /// Configured model ids for this provider. Absent in pre-models configs.
    #[serde(default)]
    pub models: Vec<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// 旧协议风格字段 "openai" | "anthropic"(读兼容,仅迁移时定槽;归一化
    /// 后恒 None,序列化省略)。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flavor: Option<String>,
}

/// 旧单端点(baseUrl+flavor)→ 双端点槽位迁移。两槽皆空且旧 baseUrl 非空时
/// 按 flavor 定槽(缺失则启发式:id=="anthropic" 或 URL 含 "anthropic" →
/// anthropic,否则 openai);旧字段随后一律清空(skip_serializing_if 保证
/// 下次落盘不再出现)。幂等:新格式条目原样通过。
fn normalize_provider_endpoints(p: &mut ProviderSpec) {
    let legacy = p.base_url.trim().to_string();
    if p.anthropic_base_url.trim().is_empty()
        && p.openai_base_url.trim().is_empty()
        && !legacy.is_empty()
    {
        let is_anthropic = match p.flavor.as_deref() {
            Some("anthropic") => true,
            Some("openai") => false,
            _ => p.id == "anthropic" || legacy.to_lowercase().contains("anthropic"),
        };
        if is_anthropic {
            p.anthropic_base_url = legacy;
        } else {
            p.openai_base_url = legacy;
        }
    }
    p.base_url = String::new();
    p.flavor = None;
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Config {
    pub models: HashMap<String, serde_json::Value>,
    pub channels: HashMap<String, serde_json::Value>,
    pub agents: HashMap<String, serde_json::Value>,
    pub skills: HashMap<String, serde_json::Value>,
    /// Canonical MCP server registry (name -> spec). BTreeMap for stable order.
    #[serde(default)]
    pub mcp_servers: BTreeMap<String, McpServerSpec>,
    /// agent_id -> server names deployed by the last successful sync. Drives
    /// remove-detection: only names we previously wrote may be removed.
    #[serde(default)]
    pub mcp_managed: HashMap<String, Vec<String>>,
    /// Configured model providers. Managed via config_providers_get/set.
    #[serde(default)]
    pub providers: Vec<ProviderSpec>,
    /// 废弃:旧「全局激活(默认)服务商」。仅为迁移保留可反序列化;
    /// load_config 迁移到 agent_providers 后清空,不再落盘。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_provider_id: Option<String>,
    /// agent_id -> 绑定的服务商 id。无条目 = ClawBox 不管理该 agent 的
    /// 服务商配置。绑定/切换/解绑经 agent_provider_bind,选中即写入生效。
    #[serde(default)]
    pub agent_providers: HashMap<String, String>,
    /// agent_id -> 上次服务商同步成功后我们写入的键名(各 agent 语义不同,
    /// 见 sync::providers 各适配器)。驱动 remove 检测:只删我们写过的。
    #[serde(default)]
    pub providers_managed: HashMap<String, Vec<String>>,
    /// agent_id -> 上次技能同步成功后我们建链的技能名(见 sync::skills)。
    /// remove 只删我们建的、仍指向库内的软链。
    #[serde(default)]
    pub skills_managed: HashMap<String, Vec<String>>,
    /// 库内技能名 -> Git 安装来源(仓库安装的才有;手动导入/收编的没有)。
    /// 驱动检查更新/覆盖更新;`skills_library_remove` 联动删除。
    #[serde(default)]
    pub skills_sources: HashMap<String, SkillSource>,
    /// agent_id -> 上次记忆同步成功后的标记(["block"] = 我们在该 agent
    /// 指令文件里维护着托管区块,见 sync::memory)。
    #[serde(default)]
    pub memory_managed: HashMap<String, Vec<String>>,
}

/// 技能的 Git 安装来源记录。
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SkillSource {
    /// 归一化后的仓库 URL(owner/repo 简写归一为 GitHub https;全 URL 原样)。
    pub repo: String,
    /// 仓内相对路径;根目录技能为 ""。
    pub subdir: String,
    /// 安装/更新时的 HEAD commit。
    pub commit: String,
    /// ISO8601(UTC)。
    pub installed_at: String,
}

/// ClawBox config path resolved against an explicit home dir so tests can
/// point it at a tempdir without touching the real user config.
pub fn clawbox_config_path(home: &Path) -> PathBuf {
    home.join(".clawbox").join("config.json")
}

pub fn real_home() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))
}

/// Load config from `<home>/.clawbox/config.json`. A missing file is the
/// only case that falls back to defaults; read/parse failures return Err.
///
/// Never silently default on a corrupt file: every write path is
/// load-modify-save on the whole Config, so a defaulted load followed by
/// any save would wipe the entire user config (data loss).
pub fn load_config(home: &Path) -> Result<Config, String> {
    let path = clawbox_config_path(home);
    if !path.exists() {
        return Ok(Config::default());
    }
    let content = fs::read_to_string(&path)
        .map_err(|e| format!("Failed to read config file {}: {}", path.display(), e))?;
    let mut config: Config = serde_json::from_str(&content)
        .map_err(|e| format!("Config file {} is corrupt: {}", path.display(), e))?;
    for p in &mut config.providers {
        normalize_provider_endpoints(p);
    }
    // 旧「全局星标」→ per-agent 绑定迁移(一次性、幂等):有星标且尚无任何
    // 绑定时,给每个之前同步过(providers_managed 非空)的 agent 生成绑定;
    // 星标一律清空。悬空星标(服务商已删)只清不迁。
    if let Some(active) = config.active_provider_id.take() {
        if config.agent_providers.is_empty()
            && config.providers.iter().any(|p| p.id == active)
        {
            for (agent_id, managed) in &config.providers_managed {
                if !managed.is_empty() {
                    config.agent_providers.insert(agent_id.clone(), active.clone());
                }
            }
        }
    }
    Ok(config)
}

pub fn save_config(home: &Path, config: &Config) -> Result<(), String> {
    let path = clawbox_config_path(home);
    let dir = path.parent().unwrap();
    if !dir.exists() {
        fs::create_dir_all(dir).map_err(|e| format!("Failed to create config dir: {}", e))?;
    }
    let content = serde_json::to_string_pretty(config)
        .map_err(|e| format!("Failed to serialize config: {}", e))?;
    fs::write(&path, content).map_err(|e| format!("Failed to write config: {}", e))
}

#[tauri::command]
pub async fn get_config() -> Result<Config, String> {
    load_config(&real_home())
}

#[tauri::command]
pub async fn set_config(path: String, value: serde_json::Value) -> Result<(), String> {
    let home = real_home();
    let mut config = load_config(&home)?;

    let parts: Vec<&str> = path.split('.').collect();
    if parts.is_empty() {
        return Err("Invalid path".to_string());
    }

    match parts[0] {
        "models" => {
            if parts.len() > 1 {
                config.models.insert(parts[1].to_string(), value);
            }
        }
        "channels" => {
            if parts.len() > 1 {
                config.channels.insert(parts[1].to_string(), value);
            }
        }
        "agents" => {
            if parts.len() > 1 {
                config.agents.insert(parts[1].to_string(), value);
            }
        }
        "skills" => {
            if parts.len() > 1 {
                config.skills.insert(parts[1].to_string(), value);
            }
        }
        "providers" => {
            return Err(
                "providers is not editable via set_config; use config_providers_set".to_string(),
            )
        }
        _ => return Err(format!("Unknown config section: {}", parts[0])),
    }

    save_config(&home, &config)?;
    Ok(())
}

#[tauri::command]
pub async fn config_providers_get() -> Result<Vec<ProviderSpec>, String> {
    Ok(load_config(&real_home())?.providers)
}

/// Whole-table overwrite 的 home 参数化核心。
///
/// 1. 被删除的服务商 → 自动解绑相关 agent(只解绑,不写 agent 配置文件)。
/// 2. 内容有变更的服务商 → 自动重推到绑定它的 agent(「配置好即同步」)。
///    保存不因个别 agent 推送失败回滚;失败逐条落在返回的 ApplyResult 里,
///    前端 toast 提示,agent 页重选一次即重试。
pub fn providers_set_at(
    home: &Path,
    providers: Vec<ProviderSpec>,
) -> Result<Vec<ApplyResult>, String> {
    let mut config = load_config(home)?;
    let old = std::mem::replace(&mut config.providers, providers);
    let ids: HashSet<String> = config.providers.iter().map(|p| p.id.clone()).collect();
    config.agent_providers.retain(|_, pid| ids.contains(pid));

    // 变更集:新列表里与旧条目不等(含新增)的服务商 id
    let changed: HashSet<String> = config
        .providers
        .iter()
        .filter(|p| old.iter().find(|o| o.id == p.id) != Some(*p))
        .map(|p| p.id.clone())
        .collect();
    let to_repush: Vec<(String, String)> = config
        .agent_providers
        .iter()
        .filter(|(_, pid)| changed.contains(*pid))
        .map(|(a, p)| (a.clone(), p.clone()))
        .collect();
    save_config(home, &config)?;

    // 重推 = 对该 agent 重新绑定一次(bind_at 自己 load/save,故先落盘上面的状态)
    let mut results = Vec::new();
    for (agent_id, pid) in to_repush {
        match crate::commands::sync::agent_provider_bind_at(home, &agent_id, Some(pid)) {
            Ok(r) => results.push(r),
            Err(e) => results.push(ApplyResult {
                agent_id,
                ok: false,
                backup_path: None,
                applied: 0,
                error: Some(e),
            }),
        }
    }
    Ok(results)
}

/// Whole-table overwrite: the frontend always sends the full provider list.
/// 返回自动重推结果(无绑定受影响时为空数组)。
#[tauri::command]
pub async fn config_providers_set(
    providers: Vec<ProviderSpec>,
) -> Result<Vec<ApplyResult>, String> {
    providers_set_at(&real_home(), providers)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sync::test_util::TempHome;

    fn spec(id: &str, name: &str) -> ProviderSpec {
        ProviderSpec {
            id: id.to_string(),
            name: name.to_string(),
            api_key: "sk-test".to_string(),
            base_url: String::new(),
            anthropic_base_url: "https://api.example.com/anthropic".to_string(),
            openai_base_url: "https://api.example.com/v1".to_string(),
            default_model: "model-x".to_string(),
            models: vec!["model-x".to_string(), "model-y".to_string()],
            enabled: true,
            flavor: None,
        }
    }

    #[test]
    fn providers_roundtrip() {
        let home = TempHome::new();
        let mut config = load_config(home.path()).unwrap();
        config.providers = vec![spec("a", "Alpha"), spec("b", "Beta")];
        save_config(home.path(), &config).unwrap();

        let loaded = load_config(home.path()).unwrap();
        assert_eq!(loaded.providers, vec![spec("a", "Alpha"), spec("b", "Beta")]);
    }

    #[test]
    fn providers_serialize_camel_case_and_omit_legacy_fields() {
        let json = serde_json::to_value(spec("a", "Alpha")).unwrap();
        assert!(json.get("apiKey").is_some());
        assert!(json.get("anthropicBaseUrl").is_some());
        assert!(json.get("openaiBaseUrl").is_some());
        assert!(json.get("defaultModel").is_some());
        assert!(json.get("api_key").is_none());
        // 归一化后的条目不再落盘旧字段
        assert!(json.get("baseUrl").is_none());
        assert!(json.get("flavor").is_none());
        // 旧字段非空时仍可序列化(读兼容对称性;正常路径不会出现)
        let mut legacy = spec("a", "Alpha");
        legacy.base_url = "https://x".into();
        legacy.flavor = Some("openai".into());
        let json = serde_json::to_value(legacy).unwrap();
        assert!(json.get("baseUrl").is_some());
        assert!(json.get("flavor").is_some());
    }

    #[test]
    fn legacy_config_without_providers_loads() {
        let home = TempHome::new();
        let path = clawbox_config_path(home.path());
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        // Old-format config.json written before the providers field existed.
        fs::write(
            &path,
            r#"{"models":{},"channels":{},"agents":{},"skills":{}}"#,
        )
        .unwrap();

        let loaded = load_config(home.path()).unwrap();
        assert!(loaded.providers.is_empty());
        assert!(loaded.active_provider_id.is_none());
        assert!(loaded.providers_managed.is_empty());

        // Defaulted optional fields also deserialize from sparse entries.
        // 裸反序列化不做迁移:旧 baseUrl 原样保留(归一化只在 load_config)。
        let sparse: ProviderSpec = serde_json::from_str(
            r#"{"id":"x","name":"X","apiKey":"k","baseUrl":"https://x"}"#,
        )
        .unwrap();
        assert_eq!(sparse.base_url, "https://x");
        assert_eq!(sparse.anthropic_base_url, "");
        assert_eq!(sparse.openai_base_url, "");
        assert_eq!(sparse.default_model, "");
        assert!(sparse.models.is_empty());
        assert!(sparse.enabled);
        assert!(sparse.flavor.is_none());
    }

    // ---- 旧单端点 → 双端点迁移 ----

    #[test]
    fn legacy_single_endpoint_providers_migrate_on_load() {
        let home = TempHome::new();
        let path = clawbox_config_path(home.path());
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        // 旧格式:baseUrl+flavor(或缺 flavor 走启发式)
        fs::write(
            &path,
            r#"{
              "models": {"keep": {"x": 1}}, "channels": {}, "agents": {}, "skills": {},
              "providers": [
                {"id": "p1", "name": "Explicit Anth", "apiKey": "k1", "baseUrl": "https://gw.example.com/x", "flavor": "anthropic"},
                {"id": "p2", "name": "Explicit OA", "apiKey": "k2", "baseUrl": "https://api.deepseek.com/anthropic", "flavor": "openai"},
                {"id": "p3", "name": "Heuristic Anth", "apiKey": "k3", "baseUrl": "https://api.minimaxi.com/Anthropic"},
                {"id": "anthropic", "name": "By Id", "apiKey": "k4", "baseUrl": "https://gw.example.com/y"},
                {"id": "p5", "name": "Heuristic OA", "apiKey": "k5", "baseUrl": "https://api.oa.example.com/v1"}
              ],
              "active_provider_id": "p1"
            }"#,
        )
        .unwrap();

        let loaded = load_config(home.path()).unwrap();
        let by_id = |id: &str| loaded.providers.iter().find(|p| p.id == id).unwrap();
        // 显式 flavor 优先(即使 URL 含 "anthropic" 也听 flavor 的)
        assert_eq!(by_id("p1").anthropic_base_url, "https://gw.example.com/x");
        assert_eq!(by_id("p1").openai_base_url, "");
        assert_eq!(by_id("p2").openai_base_url, "https://api.deepseek.com/anthropic");
        assert_eq!(by_id("p2").anthropic_base_url, "");
        // 启发式:URL 含 anthropic(大小写不敏感)/ id=="anthropic" → anthropic 槽
        assert_eq!(by_id("p3").anthropic_base_url, "https://api.minimaxi.com/Anthropic");
        assert_eq!(by_id("anthropic").anthropic_base_url, "https://gw.example.com/y");
        assert_eq!(by_id("p5").openai_base_url, "https://api.oa.example.com/v1");
        // 旧字段清空
        for p in &loaded.providers {
            assert_eq!(p.base_url, "", "{}", p.id);
            assert!(p.flavor.is_none(), "{}", p.id);
        }
        // 旧星标经迁移清空;该配置无 providers_managed → 不生成绑定
        assert!(loaded.active_provider_id.is_none());
        assert!(loaded.agent_providers.is_empty());

        // 回写:旧字段从文件里消失,其它节不丢
        save_config(home.path(), &loaded).unwrap();
        let raw: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        for p in raw["providers"].as_array().unwrap() {
            assert!(p.get("baseUrl").is_none(), "{}", p);
            assert!(p.get("flavor").is_none(), "{}", p);
            assert!(p.get("anthropicBaseUrl").is_some() || p.get("openaiBaseUrl").is_some());
        }
        assert_eq!(raw["models"]["keep"]["x"], serde_json::json!(1));
        // 再次载入幂等
        let reloaded = load_config(home.path()).unwrap();
        assert_eq!(reloaded.providers, loaded.providers);
    }

    #[test]
    fn new_format_slots_win_over_stray_legacy_fields() {
        let home = TempHome::new();
        let path = clawbox_config_path(home.path());
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        // 槽位已填时,残留的旧 baseUrl 不迁移、只被清掉
        fs::write(
            &path,
            r#"{
              "models": {}, "channels": {}, "agents": {}, "skills": {},
              "providers": [
                {"id": "p1", "name": "New", "apiKey": "k", "baseUrl": "https://stale.example.com",
                 "flavor": "anthropic", "openaiBaseUrl": "https://api.oa.example.com/v1"}
              ]
            }"#,
        )
        .unwrap();
        let loaded = load_config(home.path()).unwrap();
        assert_eq!(loaded.providers[0].openai_base_url, "https://api.oa.example.com/v1");
        assert_eq!(loaded.providers[0].anthropic_base_url, "");
        assert_eq!(loaded.providers[0].base_url, "");
        assert!(loaded.providers[0].flavor.is_none());
    }

    #[test]
    fn missing_config_file_loads_default() {
        let home = TempHome::new();
        let loaded = load_config(home.path()).unwrap();
        assert!(loaded.providers.is_empty());
        assert!(loaded.mcp_servers.is_empty());
    }

    #[test]
    fn corrupt_config_file_is_an_error_not_default() {
        let home = TempHome::new();
        let path = clawbox_config_path(home.path());
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, r#"{"models": TRUNCATED"#).unwrap();

        let err = load_config(home.path()).unwrap_err();
        assert!(err.contains("corrupt"), "unexpected error: {}", err);
        assert!(
            err.contains(&path.display().to_string()),
            "error should name the file: {}",
            err
        );
    }

    #[test]
    fn migrates_star_to_per_agent_bindings_idempotently() {
        let home = TempHome::new();
        let mut c = Config::default();
        c.providers = vec![spec("p1", "One")];
        c.active_provider_id = Some("p1".to_string());
        // claude-code 之前同步过(managed 非空);codex 从未同步(空)
        c.providers_managed.insert("claude-code".to_string(), vec!["env".to_string()]);
        c.providers_managed.insert("codex".to_string(), vec![]);
        save_config(home.path(), &c).unwrap();

        let loaded = load_config(home.path()).unwrap();
        assert_eq!(loaded.agent_providers.get("claude-code").map(String::as_str), Some("p1"));
        assert!(!loaded.agent_providers.contains_key("codex"));
        assert!(loaded.active_provider_id.is_none());

        // 幂等:落盘再加载,绑定不变
        save_config(home.path(), &loaded).unwrap();
        let again = load_config(home.path()).unwrap();
        assert_eq!(again.agent_providers, loaded.agent_providers);
        assert!(again.active_provider_id.is_none());
    }

    #[test]
    fn migration_ignores_dangling_star() {
        let home = TempHome::new();
        let mut c = Config::default();
        // 星标指向已不存在的服务商 → 不生成任何绑定,星标清空
        c.active_provider_id = Some("gone".to_string());
        c.providers_managed.insert("claude-code".to_string(), vec!["env".to_string()]);
        save_config(home.path(), &c).unwrap();

        let loaded = load_config(home.path()).unwrap();
        assert!(loaded.agent_providers.is_empty());
        assert!(loaded.active_provider_id.is_none());
    }

    #[test]
    fn providers_set_drops_bindings_of_deleted_providers() {
        let home = TempHome::new();
        let mut c = Config::default();
        c.providers = vec![spec("p1", "One"), spec("p2", "Two")];
        c.agent_providers.insert("claude-code".to_string(), "p1".to_string());
        c.agent_providers.insert("opencode".to_string(), "p2".to_string());
        save_config(home.path(), &c).unwrap();

        // 删掉 p1 → claude-code 解绑;p2 未动 → opencode 绑定保留
        providers_set_at(home.path(), vec![spec("p2", "Two")]).unwrap();
        let loaded = load_config(home.path()).unwrap();
        assert!(!loaded.agent_providers.contains_key("claude-code"));
        assert_eq!(loaded.agent_providers.get("opencode").map(String::as_str), Some("p2"));
    }

    #[test]
    fn providers_set_repushes_to_agents_bound_to_changed_provider() {
        let home = TempHome::new();
        let mut p1 = spec("p1", "One");
        p1.anthropic_base_url = "https://v1.example.com/anthropic".to_string();
        let mut c = Config::default();
        c.providers = vec![p1.clone()];
        save_config(home.path(), &c).unwrap();
        crate::commands::sync::agent_provider_bind_at(home.path(), "claude-code", Some("p1".to_string()))
            .unwrap();

        // 端点变更 → 自动重推,claude-code 配置文件跟着更新
        p1.anthropic_base_url = "https://v2.example.com/anthropic".to_string();
        let results = providers_set_at(home.path(), vec![p1]).unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].ok, "{:?}", results[0].error);
        assert_eq!(results[0].agent_id, "claude-code");

        let settings = std::fs::read_to_string(home.path().join(".claude").join("settings.json")).unwrap();
        assert!(settings.contains("https://v2.example.com/anthropic"));
    }

    #[test]
    fn providers_set_untouched_provider_triggers_no_repush() {
        let home = TempHome::new();
        let mut c = Config::default();
        c.providers = vec![spec("p1", "One")];
        save_config(home.path(), &c).unwrap();
        crate::commands::sync::agent_provider_bind_at(home.path(), "claude-code", Some("p1".to_string()))
            .unwrap();

        // 原样保存(未变) → 无重推
        let results = providers_set_at(home.path(), vec![spec("p1", "One")]).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn providers_set_delete_bound_provider_unbinds_without_touching_agent_file() {
        let home = TempHome::new();
        let mut c = Config::default();
        c.providers = vec![spec("p1", "One")];
        save_config(home.path(), &c).unwrap();
        crate::commands::sync::agent_provider_bind_at(home.path(), "claude-code", Some("p1".to_string()))
            .unwrap();
        let before = std::fs::read_to_string(home.path().join(".claude").join("settings.json")).unwrap();

        let results = providers_set_at(home.path(), vec![]).unwrap();
        assert!(results.is_empty()); // 删除 = 解绑,不重推、不清文件
        let after = std::fs::read_to_string(home.path().join(".claude").join("settings.json")).unwrap();
        assert_eq!(before, after);
        assert!(load_config(home.path()).unwrap().agent_providers.is_empty());
    }
}
