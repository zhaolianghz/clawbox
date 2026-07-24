//! 服务商(模型)配置统一下发 — MCP 下发的姊妹功能。
//!
//! ClawBox 持有服务商注册表(`Config::providers` + `agent_providers` 绑定表);
//! 各 agent 适配器把它翻译成 agent 的原生配置,合并写、只动自己管理的键。
//! `Config::providers_managed` 记录上次同步各 agent 收到的键名,remove 只
//! 作用于我们写过的键。
//!
//! 支持矩阵与 MCP 不同(claude-code/codex 单激活切换,opencode 多服务商
//! 原生支持),所以是独立的 `ProviderAdapter` trait,不塞进 `ConfigAdapter`。
//!
//! 安全铁律:所有路径以显式 `home: &Path` 解析;ChangeItem.detail 绝不含
//! apiKey 明文。

use super::{backup_target, diff_changes, AgentPlan, ApplyResult, ChangeItem};
use crate::commands::config::ProviderSpec;
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use toml_edit::{value, DocumentMut, Item, Table};

pub trait ProviderAdapter: Send + Sync {
    fn agent_id(&self) -> &'static str;
    fn supported(&self) -> bool {
        true
    }
    /// 该适配器写的主配置文件(apply 前备份的对象)。
    fn config_path(&self, home: &Path) -> PathBuf;
    fn plan(
        &self,
        home: &Path,
        providers: &[ProviderSpec],
        active_id: Option<&str>,
        managed: &[String],
    ) -> Result<Vec<ChangeItem>, String>;
    fn apply(
        &self,
        home: &Path,
        providers: &[ProviderSpec],
        active_id: Option<&str>,
        managed: &[String],
    ) -> Result<usize, String>;
    /// 本次会实际下发的键名 —— apply 成功后写入 providers_managed。
    fn deployed_names(&self, providers: &[ProviderSpec], active_id: Option<&str>) -> Vec<String>;
}

// ---- 端点槽位选择 -----------------------------------------------------------

/// 服务商的协议端点槽位。ProviderSpec 是双端点模型(anthropicBaseUrl /
/// openaiBaseUrl 可同时配置,如 MiniMax);各 agent 按自己的协议偏好取槽,
/// 不再看 provider 级 flavor(该字段只剩 load_config 迁移旧配置的用途)。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Slot {
    Anthropic,
    Openai,
}

fn slot_url(spec: &ProviderSpec, slot: Slot) -> Option<&str> {
    let url = match slot {
        Slot::Anthropic => spec.anthropic_base_url.trim(),
        Slot::Openai => spec.openai_base_url.trim(),
    };
    (!url.is_empty()).then_some(url)
}

/// 按优先级取第一个已配置的端点槽。
pub(crate) fn pick_endpoint<'a>(spec: &'a ProviderSpec, order: &[Slot]) -> Option<(&'a str, Slot)> {
    order.iter().find_map(|s| slot_url(spec, *s).map(|u| (u, *s)))
}

/// 激活服务商:active_id 指向的、且仍启用的条目。
fn active_spec<'a>(providers: &'a [ProviderSpec], active_id: Option<&str>) -> Option<&'a ProviderSpec> {
    let id = active_id?;
    providers.iter().find(|p| p.id == id && p.enabled)
}

/// 单激活语义 agent 的目标解析:要下发的服务商 + 选中端点,或 skip 原因。
enum Target<'a> {
    Deploy { spec: &'a ProviderSpec, url: &'a str },
    Skip { name: String, reason: String },
}

/// `order`:该 agent 的端点槽位偏好(claude-code 只认 Anthropic、codex 只认
/// OpenAI、hermes Anthropic 优先 OpenAI 兜底);无一命中 → skip(missing_reason)。
fn resolve_single_active<'a>(
    providers: &'a [ProviderSpec],
    active_id: Option<&str>,
    order: &[Slot],
    missing_reason: &str,
) -> Target<'a> {
    match active_spec(providers, active_id) {
        None => Target::Skip {
            name: "(active)".to_string(),
            reason: "No provider bound (pick one on the Agents page)".to_string(),
        },
        Some(spec) => {
            let Some((url, _)) = pick_endpoint(spec, order) else {
                return Target::Skip {
                    name: spec.name.clone(),
                    reason: missing_reason.to_string(),
                };
            };
            if spec.api_key.trim().is_empty() {
                return Target::Skip {
                    name: spec.name.clone(),
                    reason: "API key not configured".to_string(),
                };
            }
            Target::Deploy { spec, url }
        }
    }
}

// ---- JSON 文件通用小工具 ----------------------------------------------------

fn load_json(path: &Path) -> Result<Value, String> {
    if !path.exists() {
        return Ok(json!({}));
    }
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("failed to read {}: {}", path.display(), e))?;
    let doc: Value = serde_json::from_str(&content)
        .map_err(|e| format!("failed to parse {}: {}", path.display(), e))?;
    if !doc.is_object() {
        return Err(format!("{}: root is not a JSON object", path.display()));
    }
    Ok(doc)
}

fn write_json(path: &Path, doc: &Value) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)
            .map_err(|e| format!("failed to create {}: {}", dir.display(), e))?;
    }
    let content = serde_json::to_string_pretty(doc)
        .map_err(|e| format!("failed to serialize {}: {}", path.display(), e))?;
    std::fs::write(path, content + "\n")
        .map_err(|e| format!("failed to write {}: {}", path.display(), e))
}

// ---- settings.json env 三键(claude-code / codebuddy,单激活切换) ----------
//
// 两家的原生配置同构:`~/<dir>/settings.json` 的 env 节放 BASE_URL/KEY/MODEL
// 三个环境变量,合并写、只动我们管理的键(json_file.rs 的 MCP 工厂同款
// 模式)。实例差异全在字段里:目录、键名、端点槽、skip 文案。
//
// codebuddy 文档查证(2026-07,写码前确认):
// * https://www.codebuddy.ai/docs/zh/cli/settings —— 用户设置 =
//   ~/.codebuddy/settings.json;`env` 字段为 {"env": {"KEY": "value"}},
//   "应用于每个会话的环境变量","所有环境变量也可以在 settings.json 的
//   env 字段中配置"。
// * https://www.codebuddy.ai/docs/zh/cli/env-vars —— CODEBUDDY_BASE_URL
//   "覆盖 API 端点地址,通常与 CODEBUDDY_API_KEY 配合使用";
//   CODEBUDDY_API_KEY "API 密钥,用于模型接口调用";CODEBUDDY_MODEL
//   "覆盖默认代理模型"。官方接入示例均为 OpenAI 兼容端点 → 只认 OpenAI 槽。

pub struct EnvSettingsProviderAdapter {
    id: &'static str,
    /// home 下的配置目录(".claude" / ".codebuddy"),文件名固定 settings.json。
    dir: &'static str,
    /// env 节里我们管理的三键:[BASE_URL 键, KEY 键, MODEL 键]。用户其它键绝不碰。
    keys: [&'static str; 3],
    slots: &'static [Slot],
    missing: &'static str,
    /// remove 变更项的展示名("ANTHROPIC_*" / "CODEBUDDY_*")。
    remove_label: &'static str,
}

/// providers_managed 里的标记:表示我们管理过 env 节的三键。
const ENV_MANAGED_MARK: &str = "env";

/// claude-code:~/.claude/settings.json,ANTHROPIC_* 三键,只认 Anthropic 槽。
pub fn claude_code() -> EnvSettingsProviderAdapter {
    EnvSettingsProviderAdapter {
        id: "claude-code",
        dir: ".claude",
        keys: ["ANTHROPIC_BASE_URL", "ANTHROPIC_AUTH_TOKEN", "ANTHROPIC_MODEL"],
        slots: &[Slot::Anthropic],
        missing: "Anthropic endpoint not configured",
        remove_label: "ANTHROPIC_*",
    }
}

/// codebuddy:~/.codebuddy/settings.json,CODEBUDDY_* 三键,只认 OpenAI 槽
/// (文档查证见模块注释)。
pub fn codebuddy() -> EnvSettingsProviderAdapter {
    EnvSettingsProviderAdapter {
        id: "codebuddy",
        dir: ".codebuddy",
        keys: ["CODEBUDDY_BASE_URL", "CODEBUDDY_API_KEY", "CODEBUDDY_MODEL"],
        slots: &[Slot::Openai],
        missing: "OpenAI endpoint not configured",
        remove_label: "CODEBUDDY_*",
    }
}

impl EnvSettingsProviderAdapter {
    /// 期望写入的键值(defaultModel 为空则不含 MODEL 键)。
    fn desired_env(&self, spec: &ProviderSpec, url: &str) -> BTreeMap<&'static str, String> {
        let [base_key, api_key, model_key] = self.keys;
        let mut m = BTreeMap::new();
        m.insert(base_key, url.to_string());
        m.insert(api_key, spec.api_key.trim().to_string());
        let model = spec.default_model.trim();
        if !model.is_empty() {
            m.insert(model_key, model.to_string());
        }
        m
    }

    /// 当前文件里三个管理键的投影(其它键忽略)。
    fn current_env(&self, doc: &Value) -> Result<BTreeMap<&'static str, String>, String> {
        let env = match doc.get("env") {
            None => return Ok(BTreeMap::new()),
            Some(v) => v
                .as_object()
                .ok_or_else(|| "\"env\" is not a JSON object".to_string())?,
        };
        let mut m = BTreeMap::new();
        for key in self.keys {
            if let Some(v) = env.get(key) {
                // 非字符串值也纳入比较(转为显示形式),保证 apply 会覆写。
                m.insert(key, v.as_str().map(|s| s.to_string()).unwrap_or_else(|| v.to_string()));
            }
        }
        Ok(m)
    }

    /// 不含敏感值的变更摘要。
    fn detail(&self, spec: &ProviderSpec, url: &str) -> String {
        let model = spec.default_model.trim();
        format!(
            "{}={} · model={}",
            self.keys[0],
            url,
            if model.is_empty() { "(not set)" } else { model }
        )
    }
}

impl ProviderAdapter for EnvSettingsProviderAdapter {
    fn agent_id(&self) -> &'static str {
        self.id
    }

    fn config_path(&self, home: &Path) -> PathBuf {
        home.join(self.dir).join("settings.json")
    }

    fn plan(
        &self,
        home: &Path,
        providers: &[ProviderSpec],
        active_id: Option<&str>,
        managed: &[String],
    ) -> Result<Vec<ChangeItem>, String> {
        let doc = load_json(&self.config_path(home))?;
        let current = self.current_env(&doc)?;
        let mut changes = Vec::new();
        match resolve_single_active(providers, active_id, self.slots, self.missing) {
            Target::Deploy { spec, url } => {
                let desired = self.desired_env(spec, url);
                let action = if current == desired {
                    "unchanged"
                } else if current.is_empty() {
                    "add"
                } else {
                    "update"
                };
                changes.push(ChangeItem {
                    name: spec.name.clone(),
                    action: action.into(),
                    detail: if action == "unchanged" { String::new() } else { self.detail(spec, url) },
                });
            }
            Target::Skip { name, reason } => {
                changes.push(ChangeItem {
                    name,
                    action: "skip".into(),
                    detail: reason,
                });
                if managed.iter().any(|m| m == ENV_MANAGED_MARK) && !current.is_empty() {
                    changes.push(ChangeItem {
                        name: self.remove_label.into(),
                        action: "remove".into(),
                        detail: "no longer managed by ClawBox".into(),
                    });
                }
            }
        }
        Ok(changes)
    }

    fn apply(
        &self,
        home: &Path,
        providers: &[ProviderSpec],
        active_id: Option<&str>,
        managed: &[String],
    ) -> Result<usize, String> {
        let path = self.config_path(home);
        let mut doc = load_json(&path)?;
        let current = self.current_env(&doc)?; // 提前校验 env 节形状
        let desired: BTreeMap<&'static str, String> =
            match resolve_single_active(providers, active_id, self.slots, self.missing) {
                Target::Deploy { spec, url } => self.desired_env(spec, url),
                Target::Skip { .. } => {
                    // 无可下发目标:只有曾管理过才执行清理。
                    if managed.iter().any(|m| m == ENV_MANAGED_MARK) {
                        BTreeMap::new()
                    } else {
                        current.clone()
                    }
                }
            };
        if current == desired {
            return Ok(0);
        }

        let root = doc.as_object_mut().unwrap();
        let env = root
            .entry("env".to_string())
            .or_insert_with(|| Value::Object(Map::new()))
            .as_object_mut()
            .unwrap();
        for key in self.keys {
            match desired.get(key) {
                Some(v) => {
                    env.insert(key.to_string(), json!(v));
                }
                None => {
                    env.remove(key);
                }
            }
        }
        write_json(&path, &doc)?;
        Ok(1)
    }

    fn deployed_names(&self, providers: &[ProviderSpec], active_id: Option<&str>) -> Vec<String> {
        match resolve_single_active(providers, active_id, self.slots, self.missing) {
            Target::Deploy { .. } => vec![ENV_MANAGED_MARK.to_string()],
            Target::Skip { .. } => vec![],
        }
    }
}

// ---- codex:config.toml [model_providers.clawbox] + auth.json(单激活) -----

pub struct CodexProviderAdapter;

/// codex 里我们管理的 provider 表名(providers_managed 标记同名)。
const CODEX_PROVIDER_KEY: &str = "clawbox";
/// codex 只认 OpenAI 端点槽。
const CODEX_SLOTS: [Slot; 1] = [Slot::Openai];
const CODEX_MISSING: &str = "OpenAI endpoint not configured";
/// 模型目录文件名(相对 CODEX_HOME)。写入它并在 config.toml 用
/// model_catalog_json 引用,Codex 桌面模型选择器才会列出我们的模型;
/// 否则选择器只显示内置模型,用户配的模型不可见(codex issue #19694)。
const CODEX_CATALOG_FILE: &str = "clawbox-model-catalog.json";

impl CodexProviderAdapter {
    fn auth_path(&self, home: &Path) -> PathBuf {
        home.join(".codex").join("auth.json")
    }

    fn catalog_path(&self, home: &Path) -> PathBuf {
        home.join(".codex").join(CODEX_CATALOG_FILE)
    }

    /// 该服务商要在选择器里列出的模型 id。models 为空时回退到 defaultModel;
    /// 都为空则返回空(不写目录,选择器沿用内置模型)。defaultModel 若不在
    /// models 里也补进去,保证 config.toml 里 model= 指向的项目录中存在。
    fn catalog_slugs(spec: &ProviderSpec) -> Vec<String> {
        let mut slugs: Vec<String> = Vec::new();
        for m in &spec.models {
            let m = m.trim();
            if !m.is_empty() && !slugs.iter().any(|s| s == m) {
                slugs.push(m.to_string());
            }
        }
        let dm = spec.default_model.trim();
        if !dm.is_empty() && !slugs.iter().any(|s| s == dm) {
            slugs.push(dm.to_string());
        }
        slugs
    }

    /// 构造一个 Codex 模型目录条目。字段取 codex 解析所需的最小集合;
    /// base_instructions 留空,Codex 回退到内置默认系统提示。
    fn catalog_entry(slug: &str, priority: i64) -> Value {
        json!({
            "slug": slug,
            "display_name": slug,
            "description": slug,
            "context_window": 200000,
            "max_context_window": 200000,
            "effective_context_window_percent": 95,
            "default_reasoning_level": "medium",
            "supported_reasoning_levels": [
                {"effort": "low", "description": "Fast responses with lighter reasoning"},
                {"effort": "medium", "description": "Balances speed and reasoning depth"},
                {"effort": "high", "description": "Greater reasoning depth for complex problems"},
                {"effort": "xhigh", "description": "Extra high reasoning depth"}
            ],
            "default_reasoning_summary": "none",
            "default_verbosity": "low",
            "input_modalities": ["text", "image"],
            "visibility": "list",
            "supported_in_api": true,
            "priority": priority,
            "shell_type": "shell_command",
            "base_instructions": "",
            "apply_patch_tool_type": "freeform",
            "web_search_tool_type": "text_and_image",
            "support_verbosity": false,
            "supports_reasoning_summaries": true,
            "supports_parallel_tool_calls": true,
            "supports_search_tool": false,
            "supports_image_detail_original": false,
            "use_responses_lite": false,
            "truncation_policy": {"limit": 10000, "mode": "tokens"},
            "additional_speed_tiers": [],
            "service_tiers": [],
            "experimental_supported_tools": [],
            "availability_nux": null,
            "upgrade": null
        })
    }

    /// 完整目录 JSON(models 数组)。空 slug 列表返回 Null 表示不管理目录。
    fn catalog_doc(slugs: &[String]) -> Value {
        if slugs.is_empty() {
            return Value::Null;
        }
        let models: Vec<Value> = slugs
            .iter()
            .enumerate()
            // priority 越大越靠前;按列表顺序递减,首个模型排最前。
            .map(|(i, s)| Self::catalog_entry(s, 1000 - i as i64))
            .collect();
        json!({ "models": models })
    }

    fn load_toml(&self, home: &Path) -> Result<DocumentMut, String> {
        let path = self.config_path(home);
        if !path.exists() {
            return Ok(DocumentMut::new());
        }
        let content = std::fs::read_to_string(&path)
            .map_err(|e| format!("failed to read {}: {}", path.display(), e))?;
        content
            .parse::<DocumentMut>()
            .map_err(|e| format!("failed to parse {}: {}", path.display(), e))
    }

    /// 当前状态投影:clawbox 表 + 顶层 model_provider/model + auth key。
    /// 仅用于内部等价比较,绝不进入 ChangeItem.detail。
    fn current_state(&self, home: &Path) -> Result<Value, String> {
        let doc = self.load_toml(home)?;
        let provider = doc
            .get("model_providers")
            .and_then(|i| i.as_table())
            .and_then(|t| t.get(CODEX_PROVIDER_KEY))
            .map(super::codex::item_to_json)
            .unwrap_or(Value::Null);
        let model_provider = doc
            .get("model_provider")
            .and_then(|i| i.as_str())
            .map(|s| json!(s))
            .unwrap_or(Value::Null);
        let model = doc
            .get("model")
            .and_then(|i| i.as_str())
            .map(|s| json!(s))
            .unwrap_or(Value::Null);
        let auth = load_json(&self.auth_path(home))?;
        let key = auth.get("OPENAI_API_KEY").cloned().unwrap_or(Value::Null);
        // 目录文件当前内容(缺失=Null),纳入等价比较:模型增删会触发 update。
        let catalog = {
            let p = self.catalog_path(home);
            if p.exists() {
                std::fs::read_to_string(&p)
                    .ok()
                    .and_then(|c| serde_json::from_str::<Value>(&c).ok())
                    .unwrap_or(Value::Null)
            } else {
                Value::Null
            }
        };
        Ok(json!({
            "provider": provider,
            "model_provider": model_provider,
            "model": model,
            "OPENAI_API_KEY": key,
            "catalog": catalog,
        }))
    }

    /// 期望状态投影。defaultModel 为空时 model 沿用现值(不管理该键)。
    fn desired_state(&self, spec: &ProviderSpec, url: &str, current: &Value) -> Value {
        let model = spec.default_model.trim();
        json!({
            "provider": {
                "name": spec.name,
                "base_url": url,
                "wire_api": "responses",
            },
            "model_provider": CODEX_PROVIDER_KEY,
            "model": if model.is_empty() { current["model"].clone() } else { json!(model) },
            "OPENAI_API_KEY": json!(spec.api_key.trim()),
            "catalog": Self::catalog_doc(&Self::catalog_slugs(spec)),
        })
    }

    fn detail(spec: &ProviderSpec, url: &str) -> String {
        let model = spec.default_model.trim();
        format!(
            "base_url={} · model={}",
            url,
            if model.is_empty() { "(not set)" } else { model }
        )
    }

    /// remove 是否有事可做:clawbox 表存在,或顶层 model_provider 指向我们。
    fn has_our_entries(&self, home: &Path) -> Result<bool, String> {
        let doc = self.load_toml(home)?;
        let has_table = doc
            .get("model_providers")
            .and_then(|i| i.as_table())
            .map(|t| t.contains_key(CODEX_PROVIDER_KEY))
            .unwrap_or(false);
        let points_to_us = doc.get("model_provider").and_then(|i| i.as_str()) == Some(CODEX_PROVIDER_KEY);
        Ok(has_table || points_to_us)
    }
}

impl ProviderAdapter for CodexProviderAdapter {
    fn agent_id(&self) -> &'static str {
        "codex"
    }

    fn config_path(&self, home: &Path) -> PathBuf {
        home.join(".codex").join("config.toml")
    }

    fn plan(
        &self,
        home: &Path,
        providers: &[ProviderSpec],
        active_id: Option<&str>,
        managed: &[String],
    ) -> Result<Vec<ChangeItem>, String> {
        let mut changes = Vec::new();
        match resolve_single_active(providers, active_id, &CODEX_SLOTS, CODEX_MISSING) {
            Target::Deploy { spec, url } => {
                let current = self.current_state(home)?;
                let desired = self.desired_state(spec, url, &current);
                let action = if current == desired {
                    "unchanged"
                } else if current["provider"].is_null() {
                    "add"
                } else {
                    "update"
                };
                changes.push(ChangeItem {
                    name: spec.name.clone(),
                    action: action.into(),
                    detail: if action == "unchanged" { String::new() } else { Self::detail(spec, url) },
                });
            }
            Target::Skip { name, reason } => {
                changes.push(ChangeItem {
                    name,
                    action: "skip".into(),
                    detail: reason,
                });
                if managed.iter().any(|m| m == CODEX_PROVIDER_KEY) && self.has_our_entries(home)? {
                    changes.push(ChangeItem {
                        name: CODEX_PROVIDER_KEY.into(),
                        action: "remove".into(),
                        detail: "no longer managed by ClawBox".into(),
                    });
                }
            }
        }
        Ok(changes)
    }

    fn apply(
        &self,
        home: &Path,
        providers: &[ProviderSpec],
        active_id: Option<&str>,
        managed: &[String],
    ) -> Result<usize, String> {
        match resolve_single_active(providers, active_id, &CODEX_SLOTS, CODEX_MISSING) {
            Target::Deploy { spec, url } => {
                let current = self.current_state(home)?;
                let desired = self.desired_state(spec, url, &current);
                if current == desired {
                    return Ok(0);
                }
                let mut doc = self.load_toml(home)?;
                if doc.get("model_providers").is_some() && doc["model_providers"].as_table().is_none() {
                    return Err("\"model_providers\" is not a TOML table".to_string());
                }
                if doc.get("model_providers").is_none() {
                    let mut parent = Table::new();
                    parent.set_implicit(true); // 只渲染 [model_providers.clawbox] 头
                    doc.insert("model_providers", Item::Table(parent));
                }
                let mut t = Table::new();
                t["name"] = value(spec.name.as_str());
                t["base_url"] = value(url);
                // codex 0.5x 起移除了 chat completions,只认 responses
                // (https://github.com/openai/codex/discussions/7782;写 "chat" 会让 codex 启动即退出)
                t["wire_api"] = value("responses");
                doc["model_providers"][CODEX_PROVIDER_KEY] = Item::Table(t);
                doc["model_provider"] = value(CODEX_PROVIDER_KEY);
                let model = spec.default_model.trim();
                if !model.is_empty() {
                    doc["model"] = value(model);
                }

                // 模型目录:有模型则写目录文件并用 model_catalog_json 引用,
                // 让桌面选择器列出这些模型;没有则删目录键(回退内置模型)。
                let catalog = Self::catalog_doc(&Self::catalog_slugs(spec));
                let catalog_path = self.catalog_path(home);
                if catalog.is_null() {
                    doc.remove("model_catalog_json");
                    let _ = std::fs::remove_file(&catalog_path);
                } else {
                    write_json(&catalog_path, &catalog)?;
                    // codex 接受相对 CODEX_HOME 的路径;用文件名保持可移植。
                    doc["model_catalog_json"] = value(CODEX_CATALOG_FILE);
                }

                let path = self.config_path(home);
                if let Some(dir) = path.parent() {
                    std::fs::create_dir_all(dir)
                        .map_err(|e| format!("failed to create {}: {}", dir.display(), e))?;
                }
                std::fs::write(&path, doc.to_string())
                    .map_err(|e| format!("failed to write {}: {}", path.display(), e))?;

                // API key 合并写入 auth.json,只动 OPENAI_API_KEY 一个键。
                let auth_path = self.auth_path(home);
                let mut auth = load_json(&auth_path)?;
                auth.as_object_mut()
                    .unwrap()
                    .insert("OPENAI_API_KEY".to_string(), json!(spec.api_key.trim()));
                write_json(&auth_path, &auth)?;
                Ok(1)
            }
            Target::Skip { .. } => {
                if !managed.iter().any(|m| m == CODEX_PROVIDER_KEY) || !self.has_our_entries(home)? {
                    return Ok(0);
                }
                let mut doc = self.load_toml(home)?;
                let mut removed = false;
                if let Some(t) = doc.get_mut("model_providers").and_then(|i| i.as_table_mut()) {
                    removed |= t.remove(CODEX_PROVIDER_KEY).is_some();
                }
                // 空掉的 [model_providers] 隐式化,避免留下空表头。
                if let Some(t) = doc.get_mut("model_providers").and_then(|i| i.as_table_mut()) {
                    if t.is_empty() {
                        t.set_implicit(true);
                    }
                }
                if doc.get("model_provider").and_then(|i| i.as_str()) == Some(CODEX_PROVIDER_KEY) {
                    doc.remove("model_provider");
                    doc.remove("model");
                    removed = true;
                }
                // 目录键与目录文件一并移除,但只动我们下发的那份:值等于
                // CODEX_CATALOG_FILE 才删,用户自配的 model_catalog_json 保留。
                if doc.get("model_catalog_json").and_then(|i| i.as_str()) == Some(CODEX_CATALOG_FILE) {
                    doc.remove("model_catalog_json");
                    let _ = std::fs::remove_file(self.catalog_path(home));
                    removed = true;
                }
                if removed {
                    let path = self.config_path(home);
                    if let Some(dir) = path.parent() {
                        std::fs::create_dir_all(dir)
                            .map_err(|e| format!("failed to create {}: {}", dir.display(), e))?;
                    }
                    std::fs::write(&path, doc.to_string())
                        .map_err(|e| format!("failed to write {}: {}", path.display(), e))?;
                }
                Ok(removed as usize)
            }
        }
    }

    fn deployed_names(&self, providers: &[ProviderSpec], active_id: Option<&str>) -> Vec<String> {
        match resolve_single_active(providers, active_id, &CODEX_SLOTS, CODEX_MISSING) {
            Target::Deploy { .. } => vec![CODEX_PROVIDER_KEY.to_string()],
            Target::Skip { .. } => vec![],
        }
    }
}

// ---- opencode:opencode.json 的 provider 节(多服务商全量下发) -------------

pub struct OpencodeProviderAdapter;

/// opencode 端点偏好:OpenAI 优先(@ai-sdk/openai-compatible 通用性最好),
/// Anthropic 兜底(@ai-sdk/anthropic)。npm 包跟随选中的槽。
const OPENCODE_SLOTS: [Slot; 2] = [Slot::Openai, Slot::Anthropic];

impl OpencodeProviderAdapter {
    /// 一个服务商 → provider.<id> 的原生值;Err(reason) 变成 skip 项。
    fn render(spec: &ProviderSpec) -> Result<Value, String> {
        let Some((base_url, slot)) = pick_endpoint(spec, &OPENCODE_SLOTS) else {
            return Err("No endpoint configured".to_string());
        };
        let npm = match slot {
            Slot::Anthropic => "@ai-sdk/anthropic",
            Slot::Openai => "@ai-sdk/openai-compatible",
        };
        let mut options = Map::new();
        options.insert("baseURL".into(), json!(base_url));
        if !spec.api_key.trim().is_empty() {
            options.insert("apiKey".into(), json!(spec.api_key.trim()));
        }
        let mut o = Map::new();
        o.insert("npm".into(), json!(npm));
        o.insert("name".into(), json!(spec.name));
        o.insert("options".into(), Value::Object(options));
        if !spec.models.is_empty() {
            let mut models = Map::new();
            for m in &spec.models {
                models.insert(m.clone(), json!({}));
            }
            o.insert("models".into(), Value::Object(models));
        }
        Ok(Value::Object(o))
    }

    /// 全部 enabled 服务商(不依赖激活)按 id 渲染。
    fn mapped(providers: &[ProviderSpec]) -> BTreeMap<String, Result<Value, String>> {
        providers
            .iter()
            .filter(|p| p.enabled)
            .map(|p| (p.id.clone(), Self::render(p)))
            .collect()
    }

    fn provider_node<'a>(doc: &'a Value) -> Result<Option<&'a Map<String, Value>>, String> {
        match doc.get("provider") {
            None => Ok(None),
            Some(v) => v
                .as_object()
                .map(Some)
                .ok_or_else(|| "\"provider\" is not a JSON object".to_string()),
        }
    }
}

impl ProviderAdapter for OpencodeProviderAdapter {
    fn agent_id(&self) -> &'static str {
        "opencode"
    }

    fn config_path(&self, home: &Path) -> PathBuf {
        home.join(".config").join("opencode").join("opencode.json")
    }

    fn plan(
        &self,
        home: &Path,
        providers: &[ProviderSpec],
        _active_id: Option<&str>,
        managed: &[String],
    ) -> Result<Vec<ChangeItem>, String> {
        let doc = load_json(&self.config_path(home))?;
        let node = Self::provider_node(&doc)?;
        let mapped = Self::mapped(providers);
        // diff_changes 以 id 为条目名;换成人类可读的服务商名展示。
        let name_of = |id: &str| {
            providers
                .iter()
                .find(|p| p.id == id)
                .map(|p| p.name.clone())
                .unwrap_or_else(|| id.to_string())
        };
        Ok(diff_changes(&mapped, managed, |id| node.and_then(|n| n.get(id).cloned()))
            .into_iter()
            .map(|mut c| {
                c.name = name_of(&c.name);
                c
            })
            .collect())
    }

    fn apply(
        &self,
        home: &Path,
        providers: &[ProviderSpec],
        _active_id: Option<&str>,
        managed: &[String],
    ) -> Result<usize, String> {
        let path = self.config_path(home);
        let mut doc = load_json(&path)?;
        Self::provider_node(&doc)?; // 先校验形状再改
        if !doc.as_object().unwrap().contains_key("$schema") && !path.exists() {
            doc.as_object_mut()
                .unwrap()
                .insert("$schema".into(), json!("https://opencode.ai/config.json"));
        }
        let mapped = Self::mapped(providers);

        let root = doc.as_object_mut().unwrap();
        let node = root
            .entry("provider".to_string())
            .or_insert_with(|| Value::Object(Map::new()))
            .as_object_mut()
            .unwrap();

        let mut applied = 0;
        for (id, rendered) in &mapped {
            if let Ok(v) = rendered {
                if node.get(id) != Some(v) {
                    node.insert(id.clone(), v.clone());
                    applied += 1;
                }
            }
        }
        // remove:managed 差集(曾写、现不再下发),用户自有 provider 键不碰。
        for id in managed {
            let still = matches!(mapped.get(id), Some(Ok(_)));
            if !still && node.remove(id).is_some() {
                applied += 1;
            }
        }
        write_json(&path, &doc)?;
        Ok(applied)
    }

    fn deployed_names(&self, providers: &[ProviderSpec], _active_id: Option<&str>) -> Vec<String> {
        Self::mapped(providers)
            .into_iter()
            .filter(|(_, r)| r.is_ok())
            .map(|(id, _)| id)
            .collect()
    }
}

// ---- hermes:~/.hermes/config.yaml model 三键 + ~/.hermes/.env(单激活) -----
//
// 源码验证结论(hermes-agent 0.8.0,本机 ~/.hermes/hermes-agent 可编辑安装):
//
// * 协议:hermes 同时支持 anthropic 与 openai 协议。custom provider 的
//   api_mode ∈ {chat_completions, codex_responses, anthropic_messages}
//   (hermes_cli/main.py:3640-3676 交互选项);api_mode 未配置时按 URL 自动
//   检测 —— path 以 /anthropic(/v1) 结尾或 api.anthropic.com →
//   anthropic_messages,否则默认 chat_completions
//   (hermes_cli/runtime_provider.py:95-134 `_detect_api_mode_for_url`、
//   136-157 `_resolve_plain_custom_api_mode`)。base_url 语义 = 完整端点
//   前缀(如 https://api.minimaxi.com/anthropic),协议由后缀推断。
//   因此取槽策略:Anthropic 优先、OpenAI 兜底 —— 写哪个 URL,hermes 就按
//   该 URL 自检出对应协议。
//
// * 写入路径:`hermes config set model.provider <v>` 非交互写 config.yaml
//   嵌套键并原子写回(hermes_cli/config.py:8027 `set_config_value` →
//   atomic_yaml_write)。以 _API_KEY/_TOKEN 结尾的键会被 config set 改道
//   .env(config.py:8061);CUSTOM_PROVIDER_*_KEY 仅以 _KEY 结尾、不会被
//   改道,故 .env 由我们直接行级 merge(存在则替换该行,否则追加,其余行
//   一字不动)。
//
// * CUSTOM_PROVIDER_{ID大写、非字母数字换_}_KEY 命名:hermes 0.8.0 源码不
//   自动推导该名(全源码二进制检索仅命中 config.py:5195/5205 的无关常量
//   _VALID_CUSTOM_PROVIDER_FIELDS/_CUSTOM_PROVIDER_LIKE_FIELDS);它是本机
//   ~/.hermes/.env 既有的 key_env 约定。hermes 消费该 env 的两条途径:
//   custom_providers[].key_env 指向它(hermes_cli/providers.py:658-661
//   `resolve_custom_provider`;runtime_provider.py:1007 key 解析链),或
//   config 值内 ${VAR} 展开(hermes_cli/config.py:6264 `_expand_env_vars`)。
//   我们写该行以保持本机既有约定下的 key 新鲜;hermes 侧 custom_providers
//   条目若用内联 api_key,则该行只作镜像,无副作用。
//
// * 无 remove 语义:model.{default,provider,base_url} 被清空会破坏 hermes
//   自身运行(它总需要一个模型配置),取消激活时保留历史值,只产出 skip。
//
// * 测试铁律:`hermes config set` 固定读写真实 ~/.hermes(CLI 不吃 home
//   参数),所以 CLI 段只测参数构造(`cli_sets` 纯函数),文件段
//   (yaml 读取 diff、.env 行级 merge)全部 home 参数化 + TempHome。

pub struct HermesProviderAdapter;

/// hermes 端点偏好:Anthropic 优先、OpenAI 兜底(它按 URL 自检协议,见上)。
const HERMES_SLOTS: [Slot; 2] = [Slot::Anthropic, Slot::Openai];
const HERMES_MISSING: &str = "No endpoint configured";

impl HermesProviderAdapter {
    fn env_path(home: &Path) -> PathBuf {
        home.join(".hermes").join(".env")
    }

    /// 服务商 id → .env 键名:CUSTOM_PROVIDER_{ID 大写、非字母数字换 _}_KEY。
    fn env_key(id: &str) -> String {
        let sanitized: String = id
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_uppercase() } else { '_' })
            .collect();
        format!("CUSTOM_PROVIDER_{}_KEY", sanitized)
    }

    /// config.yaml 文本 → (model.default, model.provider, model.base_url)。
    fn model_keys(yaml_text: &str) -> Result<(Option<String>, Option<String>, Option<String>), String> {
        let doc: serde_yaml::Value = serde_yaml::from_str(yaml_text)
            .map_err(|e| format!("failed to parse config.yaml: {}", e))?;
        let model = doc.get("model");
        let get = |key: &str| -> Option<String> {
            model?
                .get(key)
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        };
        Ok((get("default"), get("provider"), get("base_url")))
    }

    /// 读 <home>/.hermes/config.yaml 的三个管理键;文件不存在 = 全 None。
    fn read_model_keys(&self, home: &Path) -> Result<(Option<String>, Option<String>, Option<String>), String> {
        let path = self.config_path(home);
        if !path.exists() {
            return Ok((None, None, None));
        }
        let text = std::fs::read_to_string(&path)
            .map_err(|e| format!("failed to read {}: {}", path.display(), e))?;
        Self::model_keys(&text)
    }

    /// .env 内容里 `KEY=value` 行的值(取第一条命中;剥掉两侧成对引号)。
    fn env_line_value(content: &str, key: &str) -> Option<String> {
        for line in content.lines() {
            let trimmed = line.trim_start();
            if let Some(rest) = trimmed.strip_prefix(key) {
                if let Some(v) = rest.strip_prefix('=') {
                    let v = v.trim();
                    let v = v
                        .strip_prefix('"')
                        .and_then(|s| s.strip_suffix('"'))
                        .or_else(|| v.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')))
                        .unwrap_or(v);
                    return Some(v.to_string());
                }
            }
        }
        None
    }

    /// 行级 merge:`KEY=` 行存在则整行替换,否则追加;其余行一字不动。
    fn merge_env_line(content: &str, key: &str, value: &str) -> String {
        let new_line = format!("{}={}", key, value);
        let mut replaced = false;
        let mut lines: Vec<String> = content
            .lines()
            .map(|line| {
                let is_ours = line
                    .trim_start()
                    .strip_prefix(key)
                    .map(|rest| rest.starts_with('='))
                    .unwrap_or(false);
                if is_ours && !replaced {
                    replaced = true;
                    new_line.clone()
                } else {
                    line.to_string()
                }
            })
            .collect();
        if !replaced {
            lines.push(new_line);
        }
        let mut out = lines.join("\n");
        out.push('\n');
        out
    }

    /// apply 的 CLI 段:`hermes config set <key> <value>` 参数组(纯函数,
    /// 可测)。`url` = 选中的端点槽;defaultModel 为空时不下发 model.default
    /// (沿用现值)。
    fn cli_sets(spec: &ProviderSpec, url: &str) -> Vec<Vec<String>> {
        let set = |key: &str, value: &str| -> Vec<String> {
            vec!["config".into(), "set".into(), key.into(), value.into()]
        };
        let mut cmds = vec![
            set("model.provider", &spec.id),
            set("model.base_url", url),
        ];
        let model = spec.default_model.trim();
        if !model.is_empty() {
            cmds.push(set("model.default", model));
        }
        cmds
    }

    /// 期望状态是否已落盘(config.yaml 三键 + .env key 行)。
    fn is_unchanged(
        spec: &ProviderSpec,
        url: &str,
        current: &(Option<String>, Option<String>, Option<String>),
        env_value: Option<&str>,
    ) -> bool {
        let (cur_default, cur_provider, cur_base) = current;
        let model = spec.default_model.trim();
        cur_provider.as_deref() == Some(spec.id.as_str())
            && cur_base.as_deref() == Some(url)
            && (model.is_empty() || cur_default.as_deref() == Some(model))
            && env_value == Some(spec.api_key.trim())
    }

    fn read_env(&self, home: &Path) -> Result<String, String> {
        let path = Self::env_path(home);
        if !path.exists() {
            return Ok(String::new());
        }
        std::fs::read_to_string(&path)
            .map_err(|e| format!("failed to read {}: {}", path.display(), e))
    }

    /// 不含敏感值的变更摘要。
    fn detail(spec: &ProviderSpec, url: &str) -> String {
        let model = spec.default_model.trim();
        format!(
            "model.provider={} · base_url={} · model={} · env {}",
            spec.id,
            url,
            if model.is_empty() { "(not set)" } else { model },
            Self::env_key(&spec.id)
        )
    }

    fn run_cli(args: &[String]) -> Result<(), String> {
        let output = std::process::Command::new("hermes")
            .args(args)
            .output()
            .map_err(|e| format!("failed to run hermes CLI: {}", e))?;
        if !output.status.success() {
            return Err(format!(
                "hermes {} failed: {}",
                args.join(" "),
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
        Ok(())
    }
}

impl ProviderAdapter for HermesProviderAdapter {
    fn agent_id(&self) -> &'static str {
        "hermes"
    }

    fn config_path(&self, home: &Path) -> PathBuf {
        home.join(".hermes").join("config.yaml")
    }

    fn plan(
        &self,
        home: &Path,
        providers: &[ProviderSpec],
        active_id: Option<&str>,
        _managed: &[String],
    ) -> Result<Vec<ChangeItem>, String> {
        let current = self.read_model_keys(home)?;
        let env = self.read_env(home)?;
        let mut changes = Vec::new();
        match resolve_single_active(providers, active_id, &HERMES_SLOTS, HERMES_MISSING) {
            Target::Deploy { spec, url } => {
                let env_value = Self::env_line_value(&env, &Self::env_key(&spec.id));
                let action = if Self::is_unchanged(spec, url, &current, env_value.as_deref()) {
                    "unchanged"
                } else if current.1.is_none() && env_value.is_none() {
                    "add"
                } else {
                    "update"
                };
                changes.push(ChangeItem {
                    name: spec.name.clone(),
                    action: action.into(),
                    detail: if action == "unchanged" { String::new() } else { Self::detail(spec, url) },
                });
            }
            Target::Skip { name, reason } => {
                // 无 remove:清空 model.* 会破坏 hermes 自身运行(见模块注释)。
                changes.push(ChangeItem {
                    name,
                    action: "skip".into(),
                    detail: reason,
                });
            }
        }
        Ok(changes)
    }

    fn apply(
        &self,
        home: &Path,
        providers: &[ProviderSpec],
        active_id: Option<&str>,
        _managed: &[String],
    ) -> Result<usize, String> {
        let (spec, url) = match resolve_single_active(providers, active_id, &HERMES_SLOTS, HERMES_MISSING) {
            Target::Deploy { spec, url } => (spec, url),
            Target::Skip { .. } => return Ok(0),
        };
        let current = self.read_model_keys(home)?;
        let env = self.read_env(home)?;
        let key = Self::env_key(&spec.id);
        if Self::is_unchanged(spec, url, &current, Self::env_line_value(&env, &key).as_deref()) {
            return Ok(0);
        }
        // config.yaml 三键走 hermes 自己的 CLI(生产路径;hermes 保证原子
        // 性与校验)。注意:CLI 固定写真实 ~/.hermes,不吃 home 参数 ——
        // 调用方(commands/sync.rs)只以 real_home() 调 apply。
        for args in Self::cli_sets(spec, url) {
            Self::run_cli(&args)?;
        }
        // .env 的 key 行:行级文本 merge 直接写文件。
        let merged = Self::merge_env_line(&env, &key, spec.api_key.trim());
        let env_path = Self::env_path(home);
        if let Some(dir) = env_path.parent() {
            std::fs::create_dir_all(dir)
                .map_err(|e| format!("failed to create {}: {}", dir.display(), e))?;
        }
        std::fs::write(&env_path, merged)
            .map_err(|e| format!("failed to write {}: {}", env_path.display(), e))?;
        Ok(1)
    }

    fn deployed_names(&self, providers: &[ProviderSpec], active_id: Option<&str>) -> Vec<String> {
        match resolve_single_active(providers, active_id, &HERMES_SLOTS, HERMES_MISSING) {
            Target::Deploy { spec, .. } => vec![Self::env_key(&spec.id)],
            Target::Skip { .. } => vec![],
        }
    }
}

// ---- openclaw:openclaw.json models.providers 节(多服务商全量下发) --------
//
// schema 查证结论(OpenClaw 2026.6.11 e085fa1,`openclaw config schema`):
//
// * 配置文件:`openclaw config file` = ~/.openclaw/openclaw.json;本机可以
//   不存在,merge-write 从无到有创建。
// * `models.providers` 为 object(additionalProperties),条目字段:
//   baseUrl(string)、apiKey(string 或 {source:"env"|"file"|"exec",
//   provider, id})、auth("api-key"|"aws-sdk"|"oauth"|"token")、api(枚举:
//   openai-completions | openai-responses | openai-chatgpt-responses |
//   anthropic-messages | google-generative-ai | google-vertex |
//   github-copilot | bedrock-converse-stream | ollama |
//   azure-openai-responses)、models(数组,元素 {id, name, ...},required
//   = [id, name],additionalProperties=false)等。
// * 端点槽 → 协议字段映射:Anthropic 槽 → api="anthropic-messages";
//   OpenAI 槽 → api="openai-completions"。
// * 默认模型键:`agents.defaults.model`,string("provider/model")或
//   {primary, fallbacks};`openclaw models status --json` 的 defaultModel
//   = "openai/gpt-5.5" 证实 "provider/model" 引用格式。
//
// 语义与 opencode 相同:写全部 enabled 服务商(openclaw 原生多服务商),
// 默认模型来自激活服务商的 defaultModel;managed 差集 remove,用户其它键
// 不碰。

pub struct OpenclawProviderAdapter;

/// openclaw 端点偏好:Anthropic 优先(anthropic-messages 是其最成熟的
/// adapter),OpenAI 兜底(openai-completions)。api 枚举值跟随选中的槽。
const OPENCLAW_SLOTS: [Slot; 2] = [Slot::Anthropic, Slot::Openai];

impl OpenclawProviderAdapter {
    /// 一个服务商 → models.providers.<id> 的原生值;Err(reason) 变成 skip 项。
    fn render(spec: &ProviderSpec) -> Result<Value, String> {
        let Some((base_url, slot)) = pick_endpoint(spec, &OPENCLAW_SLOTS) else {
            return Err("No endpoint configured".to_string());
        };
        let api = match slot {
            Slot::Anthropic => "anthropic-messages",
            Slot::Openai => "openai-completions",
        };
        let mut o = Map::new();
        o.insert("baseUrl".into(), json!(base_url));
        o.insert("api".into(), json!(api));
        if !spec.api_key.trim().is_empty() {
            o.insert("apiKey".into(), json!(spec.api_key.trim()));
        }
        if !spec.models.is_empty() {
            let models: Vec<Value> = spec
                .models
                .iter()
                .map(|m| json!({"id": m, "name": m}))
                .collect();
            o.insert("models".into(), Value::Array(models));
        }
        Ok(Value::Object(o))
    }

    /// 全部 enabled 服务商(不依赖激活)按 id 渲染。
    fn mapped(providers: &[ProviderSpec]) -> BTreeMap<String, Result<Value, String>> {
        providers
            .iter()
            .filter(|p| p.enabled)
            .map(|p| (p.id.clone(), Self::render(p)))
            .collect()
    }

    /// models.providers 节(校验 models 与 providers 的形状)。
    fn providers_node<'a>(doc: &'a Value) -> Result<Option<&'a Map<String, Value>>, String> {
        let models = match doc.get("models") {
            None => return Ok(None),
            Some(v) => v
                .as_object()
                .ok_or_else(|| "\"models\" is not a JSON object".to_string())?,
        };
        match models.get("providers") {
            None => Ok(None),
            Some(v) => v
                .as_object()
                .map(Some)
                .ok_or_else(|| "\"models.providers\" is not a JSON object".to_string()),
        }
    }

    /// 期望的默认模型引用("<id>/<model>"):激活服务商可下发(至少配置了
    /// 一个端点槽)且 defaultModel 非空时才管理,否则沿用现值。
    fn desired_default(providers: &[ProviderSpec], active_id: Option<&str>) -> Option<String> {
        let spec = active_spec(providers, active_id)?;
        pick_endpoint(spec, &OPENCLAW_SLOTS)?;
        let model = spec.default_model.trim();
        if model.is_empty() {
            return None;
        }
        Some(format!("{}/{}", spec.id, model))
    }

    /// 现有 agents.defaults.model(string 或 {primary} 的 primary)。
    fn current_default(doc: &Value) -> Option<String> {
        let model = doc.get("agents")?.get("defaults")?.get("model")?;
        match model {
            Value::String(s) => Some(s.clone()),
            Value::Object(o) => o.get("primary").and_then(|v| v.as_str()).map(|s| s.to_string()),
            _ => None,
        }
    }

    /// 现默认模型引用的 provider id 是否属于本次 remove 差集。
    fn default_points_to_removed(
        current: &str,
        mapped: &BTreeMap<String, Result<Value, String>>,
        managed: &[String],
    ) -> bool {
        let pid = current.split('/').next().unwrap_or("");
        managed.iter().any(|m| m == pid) && !matches!(mapped.get(pid), Some(Ok(_)))
    }
}

impl ProviderAdapter for OpenclawProviderAdapter {
    fn agent_id(&self) -> &'static str {
        "openclaw"
    }

    fn config_path(&self, home: &Path) -> PathBuf {
        home.join(".openclaw").join("openclaw.json")
    }

    fn plan(
        &self,
        home: &Path,
        providers: &[ProviderSpec],
        active_id: Option<&str>,
        managed: &[String],
    ) -> Result<Vec<ChangeItem>, String> {
        let doc = load_json(&self.config_path(home))?;
        let node = Self::providers_node(&doc)?;
        let mapped = Self::mapped(providers);
        let name_of = |id: &str| {
            providers
                .iter()
                .find(|p| p.id == id)
                .map(|p| p.name.clone())
                .unwrap_or_else(|| id.to_string())
        };
        let mut changes: Vec<ChangeItem> =
            diff_changes(&mapped, managed, |id| node.and_then(|n| n.get(id).cloned()))
                .into_iter()
                .map(|mut c| {
                    c.name = name_of(&c.name);
                    c
                })
                .collect();
        // 默认模型键(agents.defaults.model)单独一条变更。
        let current = Self::current_default(&doc);
        match Self::desired_default(providers, active_id) {
            Some(desired) if current.as_deref() != Some(desired.as_str()) => {
                changes.push(ChangeItem {
                    name: "agents.defaults.model".into(),
                    action: if current.is_none() { "add" } else { "update" }.into(),
                    detail: desired,
                });
            }
            None => {
                if let Some(cur) = &current {
                    if Self::default_points_to_removed(cur, &mapped, managed) {
                        changes.push(ChangeItem {
                            name: "agents.defaults.model".into(),
                            action: "remove".into(),
                            detail: "no longer managed by ClawBox".into(),
                        });
                    }
                }
            }
            _ => {}
        }
        Ok(changes)
    }

    fn apply(
        &self,
        home: &Path,
        providers: &[ProviderSpec],
        active_id: Option<&str>,
        managed: &[String],
    ) -> Result<usize, String> {
        let path = self.config_path(home);
        let mut doc = load_json(&path)?;
        Self::providers_node(&doc)?; // 先校验形状再改
        let current_default = Self::current_default(&doc);
        let mapped = Self::mapped(providers);

        let root = doc.as_object_mut().unwrap();
        let node = root
            .entry("models".to_string())
            .or_insert_with(|| Value::Object(Map::new()))
            .as_object_mut()
            .unwrap()
            .entry("providers".to_string())
            .or_insert_with(|| Value::Object(Map::new()))
            .as_object_mut()
            .unwrap();

        let mut applied = 0;
        for (id, rendered) in &mapped {
            if let Ok(v) = rendered {
                if node.get(id) != Some(v) {
                    node.insert(id.clone(), v.clone());
                    applied += 1;
                }
            }
        }
        // remove:managed 差集(曾写、现不再下发),用户自有 provider 键不碰。
        for id in managed {
            let still = matches!(mapped.get(id), Some(Ok(_)));
            if !still && node.remove(id).is_some() {
                applied += 1;
            }
        }

        // 默认模型键:string 直接写;object 形态只动 primary,保留 fallbacks。
        match Self::desired_default(providers, active_id) {
            Some(desired) => {
                if current_default.as_deref() != Some(desired.as_str()) {
                    let defaults = root
                        .entry("agents".to_string())
                        .or_insert_with(|| Value::Object(Map::new()))
                        .as_object_mut()
                        .ok_or_else(|| "\"agents\" is not a JSON object".to_string())?
                        .entry("defaults".to_string())
                        .or_insert_with(|| Value::Object(Map::new()))
                        .as_object_mut()
                        .ok_or_else(|| "\"agents.defaults\" is not a JSON object".to_string())?;
                    match defaults.get_mut("model") {
                        Some(Value::Object(o)) => {
                            o.insert("primary".into(), json!(desired));
                        }
                        _ => {
                            defaults.insert("model".into(), json!(desired));
                        }
                    }
                    applied += 1;
                }
            }
            None => {
                if let Some(cur) = &current_default {
                    if Self::default_points_to_removed(cur, &mapped, managed) {
                        if let Some(defaults) = root
                            .get_mut("agents")
                            .and_then(|a| a.get_mut("defaults"))
                            .and_then(|d| d.as_object_mut())
                        {
                            match defaults.get_mut("model") {
                                Some(Value::Object(o)) => {
                                    o.remove("primary");
                                    applied += 1;
                                }
                                Some(_) => {
                                    defaults.remove("model");
                                    applied += 1;
                                }
                                None => {}
                            }
                        }
                    }
                }
            }
        }

        write_json(&path, &doc)?;
        Ok(applied)
    }

    fn deployed_names(&self, providers: &[ProviderSpec], _active_id: Option<&str>) -> Vec<String> {
        Self::mapped(providers)
            .into_iter()
            .filter(|(_, r)| r.is_ok())
            .map(|(id, _)| id)
            .collect()
    }
}

// ---- kimi:~/.kimi/config.toml providers/models 表(多服务商全量下发) ------
//
// 文档查证(kimi-cli 官方文档,2026-07 写码前抓取确认):
//
// * https://moonshotai.github.io/kimi-cli/en/configuration/providers.md ——
//   `[providers.<名>]` 字段:type + base_url + api_key(另有可选
//   custom_headers/env);type 确切枚举:kimi | openai_legacy(OpenAI Chat
//   Completions API)| openai_responses(OpenAI Responses API)| anthropic
//   (Anthropic Claude API)| gemini | vertexai。`[models.<名>]` 字段:
//   provider(指 providers 条目名)+ model(上游模型 id),可选
//   max_context_size / capabilities。
// * https://moonshotai.github.io/kimi-cli/en/configuration/config-files.md ——
//   配置文件 = ~/.kimi/config.toml;顶层 `default_model`(string)"必须是
//   models 里定义的模型条目名"。
//
// 槽位映射:OpenAI 槽 → type="openai_legacy"(kimi 自家生态 openai 系,
// OpenAI 优先),Anthropic 槽兜底 → type="anthropic"。
//
// 写入布局:条目名一律加 clawbox- 前缀防撞(providers 与 models 同名成
// 对);每个 enabled 服务商写 [providers.clawbox-<id>],defaultModel 非空时
// 配 [models.clawbox-<id>]{provider 指回自身, model=defaultModel};顶层
// default_model = 激活服务商的条目名。managed 记条目名,差集 remove 同时
// 删两张表,顶层 default_model 指向被删条目时一并移除。toml_edit
// merge-write,用户注释与其它键不碰(codex 同款技术)。

pub struct KimiProviderAdapter;

/// kimi 端点偏好:OpenAI 优先(自家协议 openai 系),Anthropic 兜底。
const KIMI_SLOTS: [Slot; 2] = [Slot::Openai, Slot::Anthropic];
/// 条目名前缀:providers/models 两张表同名成对,防撞用户自有条目。
const KIMI_PREFIX: &str = "clawbox-";
/// 顶层默认模型键的变更项名(status 归并把它归激活服务商)。
const KIMI_DEFAULT_MODEL_ITEM: &str = "default_model";

impl KimiProviderAdapter {
    fn entry_name(id: &str) -> String {
        format!("{}{}", KIMI_PREFIX, id)
    }

    fn load_toml(&self, home: &Path) -> Result<DocumentMut, String> {
        let path = self.config_path(home);
        if !path.exists() {
            return Ok(DocumentMut::new());
        }
        let content = std::fs::read_to_string(&path)
            .map_err(|e| format!("failed to read {}: {}", path.display(), e))?;
        content
            .parse::<DocumentMut>()
            .map_err(|e| format!("failed to parse {}: {}", path.display(), e))
    }

    /// providers/models 若存在必须是标准 TOML table。
    fn check_shape(doc: &DocumentMut) -> Result<(), String> {
        for key in ["providers", "models"] {
            if doc.get(key).is_some() && doc[key].as_table().is_none() {
                return Err(format!("\"{}\" is not a TOML table", key));
            }
        }
        Ok(())
    }

    /// 期望投影:{"provider": {...}, "model": {...}|null}(model = defaultModel
    /// 非空时的 [models.<名>] 条目)。Err(reason) 变成 skip 项。
    fn render(spec: &ProviderSpec) -> Result<Value, String> {
        let Some((url, slot)) = pick_endpoint(spec, &KIMI_SLOTS) else {
            return Err("No endpoint configured".to_string());
        };
        let kind = match slot {
            Slot::Openai => "openai_legacy",
            Slot::Anthropic => "anthropic",
        };
        let name = Self::entry_name(&spec.id);
        let model = spec.default_model.trim();
        Ok(json!({
            "provider": {"type": kind, "base_url": url, "api_key": spec.api_key.trim()},
            "model": if model.is_empty() {
                Value::Null
            } else {
                json!({"provider": name, "model": model})
            },
        }))
    }

    /// 全部 enabled 服务商按条目名(clawbox-<id>)渲染。
    fn mapped(providers: &[ProviderSpec]) -> BTreeMap<String, Result<Value, String>> {
        providers
            .iter()
            .filter(|p| p.enabled)
            .map(|p| (Self::entry_name(&p.id), Self::render(p)))
            .collect()
    }

    /// 现状投影,与 render 同构;两张表都无此条目 → None。
    fn current_entry(doc: &DocumentMut, name: &str) -> Option<Value> {
        let get = |table: &str| {
            doc.get(table)
                .and_then(|i| i.as_table())
                .and_then(|t| t.get(name))
                .map(super::codex::item_to_json)
        };
        let provider = get("providers");
        let model = get("models");
        if provider.is_none() && model.is_none() {
            return None;
        }
        Some(json!({
            "provider": provider.unwrap_or(Value::Null),
            "model": model.unwrap_or(Value::Null),
        }))
    }

    fn current_default(doc: &DocumentMut) -> Option<String> {
        doc.get(KIMI_DEFAULT_MODEL_ITEM).and_then(|i| i.as_str()).map(|s| s.to_string())
    }

    /// 期望的顶层 default_model:激活服务商可下发且 defaultModel 非空 →
    /// 其条目名(default_model 必须指向 models 里定义的条目,见文档注释)。
    fn desired_default(providers: &[ProviderSpec], active_id: Option<&str>) -> Option<String> {
        let spec = active_spec(providers, active_id)?;
        pick_endpoint(spec, &KIMI_SLOTS)?;
        if spec.default_model.trim().is_empty() {
            return None;
        }
        Some(Self::entry_name(&spec.id))
    }

    /// 确保 [providers]/[models] 父表存在且隐式(只渲染子表头)。
    fn ensure_parent(doc: &mut DocumentMut, key: &str) {
        if doc.get(key).is_none() {
            let mut parent = Table::new();
            parent.set_implicit(true);
            doc.insert(key, Item::Table(parent));
        }
    }
}

impl ProviderAdapter for KimiProviderAdapter {
    fn agent_id(&self) -> &'static str {
        "kimi"
    }

    fn config_path(&self, home: &Path) -> PathBuf {
        home.join(".kimi").join("config.toml")
    }

    fn plan(
        &self,
        home: &Path,
        providers: &[ProviderSpec],
        active_id: Option<&str>,
        managed: &[String],
    ) -> Result<Vec<ChangeItem>, String> {
        let doc = self.load_toml(home)?;
        Self::check_shape(&doc)?;
        let mapped = Self::mapped(providers);
        // 条目名(clawbox-<id>)→ 服务商显示名;已消失的 id 保留原名。
        let name_of = |entry: &str| {
            entry
                .strip_prefix(KIMI_PREFIX)
                .and_then(|id| providers.iter().find(|p| p.id == id))
                .map(|p| p.name.clone())
                .unwrap_or_else(|| entry.to_string())
        };
        let mut changes: Vec<ChangeItem> =
            diff_changes(&mapped, managed, |name| Self::current_entry(&doc, name))
                .into_iter()
                .map(|mut c| {
                    c.name = name_of(&c.name);
                    c
                })
                .collect();
        // 顶层 default_model 单独一条变更。
        let current = Self::current_default(&doc);
        match Self::desired_default(providers, active_id) {
            Some(desired) if current.as_deref() != Some(desired.as_str()) => {
                changes.push(ChangeItem {
                    name: KIMI_DEFAULT_MODEL_ITEM.into(),
                    action: if current.is_none() { "add" } else { "update" }.into(),
                    detail: desired,
                });
            }
            None => {
                if let Some(cur) = &current {
                    let still = matches!(mapped.get(cur), Some(Ok(_)));
                    if managed.iter().any(|m| m == cur) && !still {
                        changes.push(ChangeItem {
                            name: KIMI_DEFAULT_MODEL_ITEM.into(),
                            action: "remove".into(),
                            detail: "no longer managed by ClawBox".into(),
                        });
                    }
                }
            }
            _ => {}
        }
        Ok(changes)
    }

    fn apply(
        &self,
        home: &Path,
        providers: &[ProviderSpec],
        active_id: Option<&str>,
        managed: &[String],
    ) -> Result<usize, String> {
        let mut doc = self.load_toml(home)?;
        Self::check_shape(&doc)?;
        let mapped = Self::mapped(providers);
        let mut applied = 0;

        for (name, rendered) in &mapped {
            let Ok(desired) = rendered else { continue };
            if Self::current_entry(&doc, name).as_ref() == Some(desired) {
                continue;
            }
            Self::ensure_parent(&mut doc, "providers");
            let mut p = Table::new();
            p["type"] = value(desired["provider"]["type"].as_str().unwrap());
            p["base_url"] = value(desired["provider"]["base_url"].as_str().unwrap());
            p["api_key"] = value(desired["provider"]["api_key"].as_str().unwrap());
            doc["providers"][name] = Item::Table(p);
            if desired["model"].is_null() {
                // defaultModel 清空:同名 models 条目不再由我们维护,移除。
                if let Some(t) = doc.get_mut("models").and_then(|i| i.as_table_mut()) {
                    t.remove(name);
                }
            } else {
                Self::ensure_parent(&mut doc, "models");
                let mut m = Table::new();
                m["provider"] = value(desired["model"]["provider"].as_str().unwrap());
                m["model"] = value(desired["model"]["model"].as_str().unwrap());
                doc["models"][name] = Item::Table(m);
            }
            applied += 1;
        }

        // remove:managed 差集(曾写、现不再下发),两张表同名成对删;
        // 顶层 default_model 指向被删条目时一并移除。用户自有条目不碰。
        for name in managed {
            let still = matches!(mapped.get(name), Some(Ok(_)));
            if still {
                continue;
            }
            let mut removed = false;
            for table in ["providers", "models"] {
                if let Some(t) = doc.get_mut(table).and_then(|i| i.as_table_mut()) {
                    removed |= t.remove(name).is_some();
                }
            }
            if Self::current_default(&doc).as_deref() == Some(name.as_str()) {
                doc.remove(KIMI_DEFAULT_MODEL_ITEM);
                removed = true;
            }
            if removed {
                applied += 1;
            }
        }
        // 空掉的父表隐式化,避免留下空表头。
        for table in ["providers", "models"] {
            if let Some(t) = doc.get_mut(table).and_then(|i| i.as_table_mut()) {
                if t.is_empty() {
                    t.set_implicit(true);
                }
            }
        }

        if let Some(desired) = Self::desired_default(providers, active_id) {
            if Self::current_default(&doc).as_deref() != Some(desired.as_str()) {
                doc[KIMI_DEFAULT_MODEL_ITEM] = value(desired);
                applied += 1;
            }
        }

        if applied == 0 {
            return Ok(0); // 全等/全 skip:不碰文件(也不无中生有建空文件)
        }
        let path = self.config_path(home);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)
                .map_err(|e| format!("failed to create {}: {}", dir.display(), e))?;
        }
        std::fs::write(&path, doc.to_string())
            .map_err(|e| format!("failed to write {}: {}", path.display(), e))?;
        Ok(applied)
    }

    fn deployed_names(&self, providers: &[ProviderSpec], _active_id: Option<&str>) -> Vec<String> {
        Self::mapped(providers)
            .into_iter()
            .filter(|(_, r)| r.is_ok())
            .map(|(name, _)| name)
            .collect()
    }
}

// ---- 占位适配器与注册表 ------------------------------------------------------

/// v1 暂不管理服务商配置的 agent(配置格式未确认,不猜)。
struct UnsupportedProviderAdapter {
    id: &'static str,
}

impl ProviderAdapter for UnsupportedProviderAdapter {
    fn agent_id(&self) -> &'static str {
        self.id
    }
    fn supported(&self) -> bool {
        false
    }
    fn config_path(&self, _home: &Path) -> PathBuf {
        PathBuf::new()
    }
    fn plan(
        &self,
        _home: &Path,
        _providers: &[ProviderSpec],
        _active_id: Option<&str>,
        _managed: &[String],
    ) -> Result<Vec<ChangeItem>, String> {
        Ok(vec![])
    }
    fn apply(
        &self,
        _home: &Path,
        _providers: &[ProviderSpec],
        _active_id: Option<&str>,
        _managed: &[String],
    ) -> Result<usize, String> {
        Err(format!("{} provider sync is not supported yet", self.id))
    }
    fn deployed_names(&self, _providers: &[ProviderSpec], _active_id: Option<&str>) -> Vec<String> {
        vec![]
    }
}

/// 所有注册的服务商适配器;id 集合与 MCP 适配器注册表一致(agent 注册表减 node)。
pub fn adapters() -> &'static [Box<dyn ProviderAdapter>] {
    static INSTANCES: std::sync::OnceLock<Vec<Box<dyn ProviderAdapter>>> = std::sync::OnceLock::new();
    INSTANCES
        .get_or_init(|| {
            vec![
                Box::new(claude_code()),
                Box::new(CodexProviderAdapter),
                Box::new(OpenclawProviderAdapter),
                Box::new(OpencodeProviderAdapter),
                Box::new(codebuddy()),
                Box::new(UnsupportedProviderAdapter { id: "cursor-agent" }),
                Box::new(KimiProviderAdapter),
                Box::new(UnsupportedProviderAdapter { id: "qodercli" }),
                Box::new(HermesProviderAdapter),
            ]
        })
        .as_slice()
}

pub fn find_adapter(id: &str) -> Option<&'static dyn ProviderAdapter> {
    adapters().iter().find(|a| a.agent_id() == id).map(|a| a.as_ref())
}

/// 按 per-agent 绑定表为每个注册适配器生成计划。未绑定的 agent 不管理即
/// 不看(changes 空);绑定的 agent 只围绕绑定服务商展开(单元素列表——与
/// agent_provider_bind 的下发口径一致)。单个 agent 的解析失败落在
/// AgentPlan::error。
pub fn plan_all(
    home: &Path,
    providers: &[ProviderSpec],
    bindings: &std::collections::HashMap<String, String>,
    managed: &std::collections::HashMap<String, Vec<String>>,
) -> Vec<AgentPlan> {
    adapters()
        .iter()
        .map(|a| {
            let agent_id = a.agent_id().to_string();
            let config_path = a.config_path(home).to_string_lossy().to_string();
            if !a.supported() {
                return AgentPlan { agent_id, supported: false, config_path, changes: vec![], error: None };
            }
            let bound = bindings
                .get(a.agent_id())
                .and_then(|pid| providers.iter().find(|p| p.id == *pid));
            let Some(spec) = bound else {
                // 未绑定(或绑定悬空):不管理该 agent,不出条目
                return AgentPlan { agent_id, supported: true, config_path, changes: vec![], error: None };
            };
            let empty = vec![];
            let m = managed.get(a.agent_id()).unwrap_or(&empty);
            let single = vec![spec.clone()];
            match a.plan(home, &single, Some(&spec.id), m) {
                Ok(changes) => AgentPlan { agent_id, supported: true, config_path, changes, error: None },
                Err(e) => AgentPlan { agent_id, supported: true, config_path, changes: vec![], error: Some(e) },
            }
        })
        .collect()
}

/// 对单个 agent 应用:备份主配置文件、写入、汇报。调用方在成功后更新
/// providers_managed。
pub fn apply_one(
    home: &Path,
    adapter: &dyn ProviderAdapter,
    providers: &[ProviderSpec],
    active_id: Option<&str>,
    managed: &[String],
) -> ApplyResult {
    let agent_id = adapter.agent_id().to_string();
    if !adapter.supported() {
        return ApplyResult {
            agent_id,
            ok: false,
            backup_path: None,
            applied: 0,
            error: Some("agent not supported for provider sync".to_string()),
        };
    }
    let backup_path = match backup_target(home, adapter.agent_id(), &adapter.config_path(home)) {
        Ok(p) => p,
        Err(e) => {
            return ApplyResult {
                agent_id,
                ok: false,
                backup_path: None,
                applied: 0,
                error: Some(e),
            }
        }
    };
    match adapter.apply(home, providers, active_id, managed) {
        Ok(applied) => ApplyResult {
            agent_id,
            ok: true,
            backup_path,
            applied,
            error: None,
        },
        Err(e) => ApplyResult {
            agent_id,
            ok: false,
            backup_path,
            applied: 0,
            error: Some(e),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sync::test_util::*;

    /// 双端点 fixture:anthropic_url / openai_url 任一为空 = 该槽未配置。
    fn provider(id: &str, name: &str, anthropic_url: &str, openai_url: &str) -> ProviderSpec {
        ProviderSpec {
            id: id.to_string(),
            name: name.to_string(),
            api_key: "sk-secret-123".to_string(),
            base_url: String::new(),
            anthropic_base_url: anthropic_url.to_string(),
            openai_base_url: openai_url.to_string(),
            default_model: "model-a".to_string(),
            models: vec!["model-a".to_string(), "model-b".to_string()],
            enabled: true,
            flavor: None,
        }
    }

    /// 只有 Anthropic 端点。
    fn anthropic_provider() -> ProviderSpec {
        provider("p-anth", "Anthro Relay", "https://relay.example.com/anthropic", "")
    }

    /// 只有 OpenAI 端点。
    fn openai_provider() -> ProviderSpec {
        provider("p-oa", "OA Relay", "", "https://api.oa.example.com/v1")
    }

    /// 双端点齐全(MiniMax 型服务商)。
    fn dual_provider() -> ProviderSpec {
        provider(
            "p-dual",
            "Dual Relay",
            "https://api.dual.example.com/anthropic",
            "https://api.dual.example.com/v1",
        )
    }

    fn read_json(home: &Path, rel: &[&str]) -> Value {
        let p = rel.iter().fold(home.to_path_buf(), |p, s| p.join(s));
        serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap()
    }

    // ---- 端点槽位选择纯函数 ----

    #[test]
    fn pick_endpoint_respects_order_and_skips_blank_slots() {
        let dual = dual_provider();
        // 顺序决定优先级
        assert_eq!(
            pick_endpoint(&dual, &[Slot::Anthropic, Slot::Openai]),
            Some(("https://api.dual.example.com/anthropic", Slot::Anthropic))
        );
        assert_eq!(
            pick_endpoint(&dual, &[Slot::Openai, Slot::Anthropic]),
            Some(("https://api.dual.example.com/v1", Slot::Openai))
        );
        // 空槽(含纯空白)回落到次选
        let mut anth_only = anthropic_provider();
        anth_only.openai_base_url = "  ".into();
        assert_eq!(
            pick_endpoint(&anth_only, &[Slot::Openai, Slot::Anthropic]),
            Some(("https://relay.example.com/anthropic", Slot::Anthropic))
        );
        // 两槽皆空 → None
        let blank = provider("b", "B", "", "  ");
        assert_eq!(pick_endpoint(&blank, &[Slot::Anthropic, Slot::Openai]), None);
        // 单槽 order 不兜底
        assert_eq!(pick_endpoint(&anthropic_provider(), &[Slot::Openai]), None);
    }

    // ---- claude-code ----

    #[test]
    fn claude_empty_file_plans_add_and_apply_writes_three_keys() {
        let home = TempHome::new();
        let providers = vec![anthropic_provider()];
        let a = claude_code();
        let changes = a.plan(home.path(), &providers, Some("p-anth"), &[]).unwrap();
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].action, "add");
        assert!(!changes[0].detail.contains("sk-secret"), "detail must not leak apiKey");

        assert_eq!(a.apply(home.path(), &providers, Some("p-anth"), &[]).unwrap(), 1);
        let doc = read_json(home.path(), &[".claude", "settings.json"]);
        assert_eq!(doc["env"]["ANTHROPIC_BASE_URL"], json!("https://relay.example.com/anthropic"));
        assert_eq!(doc["env"]["ANTHROPIC_AUTH_TOKEN"], json!("sk-secret-123"));
        assert_eq!(doc["env"]["ANTHROPIC_MODEL"], json!("model-a"));
        // 再次 plan → unchanged;再次 apply → 0
        let managed = vec!["env".to_string()];
        let changes = a.plan(home.path(), &providers, Some("p-anth"), &managed).unwrap();
        assert_eq!(changes[0].action, "unchanged");
        assert_eq!(a.apply(home.path(), &providers, Some("p-anth"), &managed).unwrap(), 0);
    }

    #[test]
    fn claude_merge_keeps_user_env_and_other_top_keys() {
        let home = TempHome::new();
        write_file(
            home.path(),
            &PathBuf::from(".claude").join("settings.json"),
            r#"{
              "model": "opus",
              "env": {
                "ANTHROPIC_AUTH_TOKEN": "user-own-token",
                "MY_VAR": "keep-me"
              },
              "permissions": {"allow": ["Bash"]}
            }"#,
        );
        let providers = vec![anthropic_provider()];
        let a = claude_code();
        let changes = a.plan(home.path(), &providers, Some("p-anth"), &[]).unwrap();
        assert_eq!(changes[0].action, "update"); // 已有我们管理的键之一

        a.apply(home.path(), &providers, Some("p-anth"), &[]).unwrap();
        let doc = read_json(home.path(), &[".claude", "settings.json"]);
        assert_eq!(doc["env"]["MY_VAR"], json!("keep-me"));
        assert_eq!(doc["model"], json!("opus"));
        assert_eq!(doc["permissions"]["allow"], json!(["Bash"]));
        assert_eq!(doc["env"]["ANTHROPIC_AUTH_TOKEN"], json!("sk-secret-123"));
    }

    #[test]
    fn claude_skips_when_anthropic_endpoint_missing() {
        // 激活服务商只有 OpenAI 端点 → claude-code 无槽可用,skip
        let home = TempHome::new();
        let providers = vec![openai_provider()];
        let a = claude_code();
        let changes = a.plan(home.path(), &providers, Some("p-oa"), &[]).unwrap();
        assert_eq!(changes[0].action, "skip");
        assert!(changes[0].detail.contains("Anthropic endpoint not configured"), "{}", changes[0].detail);
        assert_eq!(a.apply(home.path(), &providers, Some("p-oa"), &[]).unwrap(), 0);
        assert!(a.deployed_names(&providers, Some("p-oa")).is_empty());
        // 目标文件从未被创建
        assert!(!home.path().join(".claude").join("settings.json").exists());
    }

    #[test]
    fn claude_no_active_provider_plans_skip() {
        let home = TempHome::new();
        let providers = vec![anthropic_provider()];
        let a = claude_code();
        let changes = a.plan(home.path(), &providers, None, &[]).unwrap();
        assert_eq!(changes[0].action, "skip");
        // 激活的是被禁用的服务商 → 同样 skip
        let mut disabled = anthropic_provider();
        disabled.enabled = false;
        let changes = a.plan(home.path(), &[disabled], Some("p-anth"), &[]).unwrap();
        assert_eq!(changes[0].action, "skip");
    }

    #[test]
    fn claude_remove_deletes_only_managed_three_keys() {
        let home = TempHome::new();
        let providers = vec![anthropic_provider()];
        let a = claude_code();
        // 先部署,再在同一文件里塞一个用户自己的 env 键
        a.apply(home.path(), &providers, Some("p-anth"), &[]).unwrap();
        let mut doc = read_json(home.path(), &[".claude", "settings.json"]);
        doc["env"]["USER_KEY"] = json!("mine");
        write_file(
            home.path(),
            &PathBuf::from(".claude").join("settings.json"),
            &serde_json::to_string_pretty(&doc).unwrap(),
        );

        // 取消激活 → remove(managed 里有 env 标记)
        let managed = vec!["env".to_string()];
        let changes = a.plan(home.path(), &providers, None, &managed).unwrap();
        assert!(changes.iter().any(|c| c.action == "remove"));
        assert_eq!(a.apply(home.path(), &providers, None, &managed).unwrap(), 1);
        let doc = read_json(home.path(), &[".claude", "settings.json"]);
        let env = doc["env"].as_object().unwrap();
        assert!(env.get("ANTHROPIC_BASE_URL").is_none());
        assert!(env.get("ANTHROPIC_AUTH_TOKEN").is_none());
        assert!(env.get("ANTHROPIC_MODEL").is_none());
        assert_eq!(env["USER_KEY"], json!("mine"));

        // 未曾管理(managed 空)时,即使文件里有同名键也不动
        write_file(
            home.path(),
            &PathBuf::from(".claude").join("settings.json"),
            r#"{"env": {"ANTHROPIC_AUTH_TOKEN": "user-own"}}"#,
        );
        assert_eq!(a.apply(home.path(), &providers, None, &[]).unwrap(), 0);
        let doc = read_json(home.path(), &[".claude", "settings.json"]);
        assert_eq!(doc["env"]["ANTHROPIC_AUTH_TOKEN"], json!("user-own"));
    }

    #[test]
    fn claude_empty_default_model_omits_model_key_and_removes_stale() {
        let home = TempHome::new();
        let mut providers = vec![anthropic_provider()];
        let a = claude_code();
        a.apply(home.path(), &providers, Some("p-anth"), &[]).unwrap();
        // 清空 defaultModel → ANTHROPIC_MODEL 应被移除,另两键保留
        providers[0].default_model = String::new();
        let managed = vec!["env".to_string()];
        let changes = a.plan(home.path(), &providers, Some("p-anth"), &managed).unwrap();
        assert_eq!(changes[0].action, "update");
        a.apply(home.path(), &providers, Some("p-anth"), &managed).unwrap();
        let doc = read_json(home.path(), &[".claude", "settings.json"]);
        let env = doc["env"].as_object().unwrap();
        assert!(env.get("ANTHROPIC_MODEL").is_none());
        assert!(env.get("ANTHROPIC_BASE_URL").is_some());
    }

    #[test]
    fn claude_corrupt_settings_plans_error_not_panic() {
        let home = TempHome::new();
        write_file(
            home.path(),
            &PathBuf::from(".claude").join("settings.json"),
            "{ not json",
        );
        let err = claude_code()
            .plan(home.path(), &[anthropic_provider()], Some("p-anth"), &[])
            .unwrap_err();
        assert!(err.contains("parse"), "{}", err);
    }

    // ---- codex ----

    fn codex_rel() -> PathBuf {
        PathBuf::from(".codex").join("config.toml")
    }

    #[test]
    fn codex_writes_provider_table_top_keys_and_auth() {
        let home = TempHome::new();
        let providers = vec![openai_provider()];
        let a = CodexProviderAdapter;
        let changes = a.plan(home.path(), &providers, Some("p-oa"), &[]).unwrap();
        assert_eq!(changes[0].action, "add");
        assert!(!changes[0].detail.contains("sk-secret"), "detail must not leak apiKey");

        assert_eq!(a.apply(home.path(), &providers, Some("p-oa"), &[]).unwrap(), 1);
        let text = std::fs::read_to_string(home.path().join(codex_rel())).unwrap();
        assert!(text.contains("[model_providers.clawbox]"), "{}", text);
        assert!(text.contains("base_url = \"https://api.oa.example.com/v1\""), "{}", text);
        assert!(text.contains("wire_api = \"responses\""), "{}", text);
        assert!(text.contains("model_provider = \"clawbox\""), "{}", text);
        assert!(text.contains("model = \"model-a\""), "{}", text);
        assert!(!text.contains("sk-secret"), "config.toml must not contain the key");
        let auth = read_json(home.path(), &[".codex", "auth.json"]);
        assert_eq!(auth["OPENAI_API_KEY"], json!("sk-secret-123"));

        // 幂等
        let managed = vec!["clawbox".to_string()];
        let changes = a.plan(home.path(), &providers, Some("p-oa"), &managed).unwrap();
        assert_eq!(changes[0].action, "unchanged");
        assert_eq!(a.apply(home.path(), &providers, Some("p-oa"), &managed).unwrap(), 0);
    }

    #[test]
    fn codex_preserves_comments_and_merges_auth() {
        let home = TempHome::new();
        write_file(
            home.path(),
            &codex_rel(),
            "# my comment\nsandbox_mode = \"workspace-write\" # inline\n\n[profiles.fast]\nmodel_reasoning_effort = \"low\"\n",
        );
        write_file(
            home.path(),
            &PathBuf::from(".codex").join("auth.json"),
            r#"{"tokens": {"refresh": "keep-me"}}"#,
        );
        let providers = vec![openai_provider()];
        CodexProviderAdapter.apply(home.path(), &providers, Some("p-oa"), &[]).unwrap();
        let text = std::fs::read_to_string(home.path().join(codex_rel())).unwrap();
        assert!(text.contains("# my comment"));
        assert!(text.contains("# inline"));
        assert!(text.contains("[profiles.fast]"));
        assert!(text.contains("[model_providers.clawbox]"));
        let auth = read_json(home.path(), &[".codex", "auth.json"]);
        assert_eq!(auth["tokens"]["refresh"], json!("keep-me"));
        assert_eq!(auth["OPENAI_API_KEY"], json!("sk-secret-123"));
    }

    #[test]
    fn codex_skips_when_openai_endpoint_missing() {
        // 激活服务商只有 Anthropic 端点 → codex 无槽可用,skip
        let home = TempHome::new();
        let providers = vec![anthropic_provider()];
        let a = CodexProviderAdapter;
        let changes = a.plan(home.path(), &providers, Some("p-anth"), &[]).unwrap();
        assert_eq!(changes[0].action, "skip");
        assert!(changes[0].detail.contains("OpenAI endpoint not configured"), "{}", changes[0].detail);
        assert_eq!(a.apply(home.path(), &providers, Some("p-anth"), &[]).unwrap(), 0);
        assert!(a.deployed_names(&providers, Some("p-anth")).is_empty());
        assert!(!home.path().join(codex_rel()).exists());
    }

    #[test]
    fn dual_endpoint_provider_deploys_to_claude_and_codex_with_own_slots() {
        // 双端点服务商(MiniMax 型):claude-code 取 Anthropic 槽、codex 取
        // OpenAI 槽,两家同时可下发 —— 本次改造要解的核心场景。
        let home = TempHome::new();
        let providers = vec![dual_provider()];
        let claude = claude_code();
        let codex = CodexProviderAdapter;
        assert_eq!(claude.plan(home.path(), &providers, Some("p-dual"), &[]).unwrap()[0].action, "add");
        assert_eq!(codex.plan(home.path(), &providers, Some("p-dual"), &[]).unwrap()[0].action, "add");

        claude.apply(home.path(), &providers, Some("p-dual"), &[]).unwrap();
        codex.apply(home.path(), &providers, Some("p-dual"), &[]).unwrap();
        let doc = read_json(home.path(), &[".claude", "settings.json"]);
        assert_eq!(doc["env"]["ANTHROPIC_BASE_URL"], json!("https://api.dual.example.com/anthropic"));
        let text = std::fs::read_to_string(home.path().join(codex_rel())).unwrap();
        assert!(text.contains("base_url = \"https://api.dual.example.com/v1\""), "{}", text);
    }

    #[test]
    fn codex_remove_deletes_our_table_and_top_keys_only_when_ours() {
        let home = TempHome::new();
        let providers = vec![openai_provider()];
        let a = CodexProviderAdapter;
        a.apply(home.path(), &providers, Some("p-oa"), &[]).unwrap();

        let managed = vec!["clawbox".to_string()];
        let changes = a.plan(home.path(), &providers, None, &managed).unwrap();
        assert!(changes.iter().any(|c| c.action == "remove"));
        assert_eq!(a.apply(home.path(), &providers, None, &managed).unwrap(), 1);
        let text = std::fs::read_to_string(home.path().join(codex_rel())).unwrap();
        assert!(!text.contains("[model_providers.clawbox]"), "{}", text);
        assert!(!text.contains("model_provider"), "{}", text);
        // auth.json 的 key 不在 remove 范围
        let auth = read_json(home.path(), &[".codex", "auth.json"]);
        assert_eq!(auth["OPENAI_API_KEY"], json!("sk-secret-123"));

        // 用户自己把 model_provider 指到别家:remove 不碰顶层键
        write_file(
            home.path(),
            &codex_rel(),
            "model_provider = \"ollama\"\nmodel = \"llama3\"\n\n[model_providers.clawbox]\nname = \"stale\"\n",
        );
        assert_eq!(a.apply(home.path(), &providers, None, &managed).unwrap(), 1);
        let text = std::fs::read_to_string(home.path().join(codex_rel())).unwrap();
        assert!(text.contains("model_provider = \"ollama\""));
        assert!(text.contains("model = \"llama3\""));
        assert!(!text.contains("clawbox"));
    }

    #[test]
    fn codex_empty_default_model_leaves_existing_model_untouched() {
        let home = TempHome::new();
        write_file(home.path(), &codex_rel(), "model = \"user-model\"\n");
        let mut p = openai_provider();
        p.default_model = String::new();
        let a = CodexProviderAdapter;
        a.apply(home.path(), &[p], Some("p-oa"), &[]).unwrap();
        let text = std::fs::read_to_string(home.path().join(codex_rel())).unwrap();
        assert!(text.contains("model = \"user-model\""), "{}", text);
        assert!(text.contains("model_provider = \"clawbox\""), "{}", text);
    }

    fn codex_catalog_rel() -> PathBuf {
        PathBuf::from(".codex").join(CODEX_CATALOG_FILE)
    }

    #[test]
    fn codex_writes_model_catalog_listing_configured_models() {
        let home = TempHome::new();
        let providers = vec![openai_provider()]; // models = [model-a, model-b]
        let a = CodexProviderAdapter;
        a.apply(home.path(), &providers, Some("p-oa"), &[]).unwrap();

        let text = std::fs::read_to_string(home.path().join(codex_rel())).unwrap();
        assert!(
            text.contains(&format!("model_catalog_json = \"{}\"", CODEX_CATALOG_FILE)),
            "{}",
            text
        );
        let cat = read_json(home.path(), &[".codex", CODEX_CATALOG_FILE]);
        let slugs: Vec<&str> = cat["models"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m["slug"].as_str().unwrap())
            .collect();
        assert_eq!(slugs, vec!["model-a", "model-b"]);
        // 首个模型 priority 最高(选择器排最前)。
        assert!(cat["models"][0]["priority"].as_i64().unwrap() > cat["models"][1]["priority"].as_i64().unwrap());
        // 必需但我们留空的字段:codex 会回退内置默认提示。
        assert_eq!(cat["models"][0]["base_instructions"], json!(""));
        assert_eq!(cat["models"][0]["shell_type"], json!("shell_command"));

        // 幂等:二次 apply 无变更。
        let managed = vec!["clawbox".to_string()];
        assert_eq!(a.apply(home.path(), &providers, Some("p-oa"), &managed).unwrap(), 0);
    }

    #[test]
    fn codex_catalog_includes_default_model_not_in_models_list() {
        let home = TempHome::new();
        let mut p = openai_provider();
        p.models = vec!["listed-a".to_string()];
        p.default_model = "solo-default".to_string();
        let a = CodexProviderAdapter;
        a.apply(home.path(), &[p], Some("p-oa"), &[]).unwrap();
        let cat = read_json(home.path(), &[".codex", CODEX_CATALOG_FILE]);
        let slugs: Vec<&str> = cat["models"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m["slug"].as_str().unwrap())
            .collect();
        assert!(slugs.contains(&"listed-a"), "{:?}", slugs);
        assert!(slugs.contains(&"solo-default"), "{:?}", slugs);
    }

    #[test]
    fn codex_no_models_writes_no_catalog() {
        let home = TempHome::new();
        let mut p = openai_provider();
        p.models = vec![];
        p.default_model = String::new();
        let a = CodexProviderAdapter;
        a.apply(home.path(), &[p], Some("p-oa"), &[]).unwrap();
        let text = std::fs::read_to_string(home.path().join(codex_rel())).unwrap();
        assert!(!text.contains("model_catalog_json"), "{}", text);
        assert!(!home.path().join(codex_catalog_rel()).exists());
    }

    #[test]
    fn codex_model_change_triggers_update_and_rewrites_catalog() {
        let home = TempHome::new();
        let a = CodexProviderAdapter;
        a.apply(home.path(), &[openai_provider()], Some("p-oa"), &[]).unwrap();

        let mut p2 = openai_provider();
        p2.models = vec!["model-a".to_string(), "model-c".to_string()];
        let managed = vec!["clawbox".to_string()];
        let changes = a.plan(home.path(), &[p2.clone()], Some("p-oa"), &managed).unwrap();
        assert_eq!(changes[0].action, "update");
        assert_eq!(a.apply(home.path(), &[p2], Some("p-oa"), &managed).unwrap(), 1);
        let cat = read_json(home.path(), &[".codex", CODEX_CATALOG_FILE]);
        let slugs: Vec<&str> = cat["models"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m["slug"].as_str().unwrap())
            .collect();
        assert_eq!(slugs, vec!["model-a", "model-c"]);
    }

    #[test]
    fn codex_remove_deletes_our_catalog_but_keeps_user_catalog() {
        // 我们下发过 → remove 时目录键与文件都清掉。
        let home = TempHome::new();
        let a = CodexProviderAdapter;
        let managed = vec!["clawbox".to_string()];
        a.apply(home.path(), &[openai_provider()], Some("p-oa"), &[]).unwrap();
        assert!(home.path().join(codex_catalog_rel()).exists());
        // 无激活服务商 → 走 remove。
        a.apply(home.path(), &[anthropic_provider()], Some("p-anth"), &managed).unwrap();
        let text = std::fs::read_to_string(home.path().join(codex_rel())).unwrap();
        assert!(!text.contains("model_catalog_json"), "{}", text);
        assert!(!home.path().join(codex_catalog_rel()).exists());

        // 用户自配的 model_catalog_json → remove 时保留。
        let home = TempHome::new();
        write_file(
            home.path(),
            &codex_rel(),
            "model_provider = \"clawbox\"\nmodel = \"m\"\nmodel_catalog_json = \"my-own.json\"\n\n[model_providers.clawbox]\nname = \"x\"\n",
        );
        a.apply(home.path(), &[anthropic_provider()], Some("p-anth"), &managed).unwrap();
        let text = std::fs::read_to_string(home.path().join(codex_rel())).unwrap();
        assert!(text.contains("model_catalog_json = \"my-own.json\""), "{}", text);
    }

    #[test]
    fn codex_corrupt_toml_or_auth_plans_error() {
        let home = TempHome::new();
        write_file(home.path(), &codex_rel(), "[broken\n");
        let err = CodexProviderAdapter
            .plan(home.path(), &[openai_provider()], Some("p-oa"), &[])
            .unwrap_err();
        assert!(err.contains("parse"), "{}", err);

        let home = TempHome::new();
        write_file(home.path(), &PathBuf::from(".codex").join("auth.json"), "{ nope");
        let err = CodexProviderAdapter
            .plan(home.path(), &[openai_provider()], Some("p-oa"), &[])
            .unwrap_err();
        assert!(err.contains("parse"), "{}", err);
    }

    // ---- opencode ----

    fn opencode_rel() -> PathBuf {
        PathBuf::from(".config").join("opencode").join("opencode.json")
    }

    #[test]
    fn opencode_writes_all_enabled_providers_with_slot_npm() {
        let home = TempHome::new();
        let mut disabled = provider("p-off", "Off", "", "https://off.example.com");
        disabled.enabled = false;
        let providers = vec![anthropic_provider(), openai_provider(), disabled];
        let a = OpencodeProviderAdapter;
        // 不依赖激活:active_id = None 也全量下发
        let changes = a.plan(home.path(), &providers, None, &[]).unwrap();
        assert_eq!(changes.iter().filter(|c| c.action == "add").count(), 2);

        assert_eq!(a.apply(home.path(), &providers, None, &[]).unwrap(), 2);
        let doc = read_json(home.path(), &[".config", "opencode", "opencode.json"]);
        // 只有 Anthropic 槽 → 兜底选中,npm 跟槽
        let anth = &doc["provider"]["p-anth"];
        assert_eq!(anth["npm"], json!("@ai-sdk/anthropic"));
        assert_eq!(anth["name"], json!("Anthro Relay"));
        assert_eq!(anth["options"]["baseURL"], json!("https://relay.example.com/anthropic"));
        assert_eq!(anth["options"]["apiKey"], json!("sk-secret-123"));
        assert_eq!(anth["models"]["model-a"], json!({}));
        assert_eq!(anth["models"]["model-b"], json!({}));
        let oa = &doc["provider"]["p-oa"];
        assert_eq!(oa["npm"], json!("@ai-sdk/openai-compatible"));
        assert!(doc["provider"].get("p-off").is_none());
        assert_eq!(
            a.deployed_names(&providers, None),
            vec!["p-anth".to_string(), "p-oa".to_string()]
        );
    }

    #[test]
    fn opencode_dual_endpoint_prefers_openai_slot() {
        let home = TempHome::new();
        let a = OpencodeProviderAdapter;
        a.apply(home.path(), &[dual_provider()], None, &[]).unwrap();
        let doc = read_json(home.path(), &[".config", "opencode", "opencode.json"]);
        let dual = &doc["provider"]["p-dual"];
        assert_eq!(dual["npm"], json!("@ai-sdk/openai-compatible"));
        assert_eq!(dual["options"]["baseURL"], json!("https://api.dual.example.com/v1"));
    }

    #[test]
    fn opencode_models_node_omitted_when_empty() {
        let home = TempHome::new();
        let mut p = openai_provider();
        p.models = vec![];
        let a = OpencodeProviderAdapter;
        a.apply(home.path(), &[p], None, &[]).unwrap();
        let doc = read_json(home.path(), &[".config", "opencode", "opencode.json"]);
        assert!(doc["provider"]["p-oa"].get("models").is_none());
    }

    #[test]
    fn opencode_keeps_user_providers_and_other_keys_and_removes_managed_diff() {
        let home = TempHome::new();
        write_file(
            home.path(),
            &opencode_rel(),
            r#"{
              "$schema": "https://opencode.ai/config.json",
              "mcp": {"srv": {"type": "remote", "url": "https://x/mcp"}},
              "provider": {
                "user-own": {"npm": "@ai-sdk/openai-compatible", "options": {"baseURL": "https://mine"}},
                "gone-id": {"npm": "@ai-sdk/openai-compatible", "options": {"baseURL": "https://old"}}
              }
            }"#,
        );
        let providers = vec![openai_provider()];
        let managed = vec!["gone-id".to_string()];
        let a = OpencodeProviderAdapter;
        let changes = a.plan(home.path(), &providers, None, &managed).unwrap();
        assert!(changes.iter().any(|c| c.action == "remove"));

        a.apply(home.path(), &providers, None, &managed).unwrap();
        let doc = read_json(home.path(), &[".config", "opencode", "opencode.json"]);
        assert!(doc["provider"].get("gone-id").is_none());
        assert!(doc["provider"].get("user-own").is_some());
        assert!(doc["provider"].get("p-oa").is_some());
        assert_eq!(doc["mcp"]["srv"]["url"], json!("https://x/mcp"));
        assert_eq!(doc["$schema"], json!("https://opencode.ai/config.json"));
    }

    #[test]
    fn opencode_no_endpoint_is_skipped() {
        let home = TempHome::new();
        let p = provider("p-none", "None", " ", "  "); // 两槽皆空白
        let a = OpencodeProviderAdapter;
        let changes = a.plan(home.path(), &[p.clone()], None, &[]).unwrap();
        assert_eq!(changes[0].action, "skip");
        assert!(changes[0].detail.contains("No endpoint configured"), "{}", changes[0].detail);
        assert!(a.deployed_names(&[p], None).is_empty());
    }

    // ---- hermes ----
    // 注意:hermes 的 CLI 段绝不在测试中运行(hermes 固定读写真实
    // ~/.hermes)。这里只测纯函数(env_key/cli_sets/merge_env_line/
    // env_line_value/model_keys)与 home 参数化的 plan/deployed_names。

    #[test]
    fn hermes_env_key_uppercases_and_replaces_non_alnum() {
        assert_eq!(
            HermesProviderAdapter::env_key("minimax-sky"),
            "CUSTOM_PROVIDER_MINIMAX_SKY_KEY"
        );
        assert_eq!(
            HermesProviderAdapter::env_key("glm.logan_2"),
            "CUSTOM_PROVIDER_GLM_LOGAN_2_KEY"
        );
    }

    #[test]
    fn hermes_cli_sets_builds_three_key_commands() {
        let spec = anthropic_provider();
        let url = "https://relay.example.com/anthropic";
        let cmds = HermesProviderAdapter::cli_sets(&spec, url);
        assert_eq!(
            cmds,
            vec![
                vec!["config", "set", "model.provider", "p-anth"],
                vec!["config", "set", "model.base_url", "https://relay.example.com/anthropic"],
                vec!["config", "set", "model.default", "model-a"],
            ]
            .into_iter()
            .map(|v| v.into_iter().map(String::from).collect::<Vec<_>>())
            .collect::<Vec<_>>()
        );
        // defaultModel 为空 → 不下发 model.default,沿用现值
        let mut no_model = anthropic_provider();
        no_model.default_model = String::new();
        let cmds = HermesProviderAdapter::cli_sets(&no_model, url);
        assert_eq!(cmds.len(), 2);
        assert!(!cmds.iter().any(|c| c.contains(&"model.default".to_string())));
        // 参数里绝不出现 apiKey
        for c in HermesProviderAdapter::cli_sets(&spec, url) {
            assert!(!c.iter().any(|a| a.contains("sk-secret")));
        }
    }

    #[test]
    fn hermes_merge_env_line_replaces_in_place_and_appends() {
        let key = "CUSTOM_PROVIDER_P_ANTH_KEY";
        // 已存在 → 整行替换,其余行一字不动
        let content = "A=1\nCUSTOM_PROVIDER_P_ANTH_KEY=old\nB=2 # keep\n";
        let merged = HermesProviderAdapter::merge_env_line(content, key, "new-key");
        assert_eq!(merged, "A=1\nCUSTOM_PROVIDER_P_ANTH_KEY=new-key\nB=2 # keep\n");
        // 不存在 → 追加(含无尾换行的输入)
        let merged = HermesProviderAdapter::merge_env_line("A=1", key, "v");
        assert_eq!(merged, "A=1\nCUSTOM_PROVIDER_P_ANTH_KEY=v\n");
        // 空文件 → 单行
        let merged = HermesProviderAdapter::merge_env_line("", key, "v");
        assert_eq!(merged, "CUSTOM_PROVIDER_P_ANTH_KEY=v\n");
        // 前缀相同的其它键(…_KEY_BACKUP)不受影响
        let content = "CUSTOM_PROVIDER_P_ANTH_KEY_BACKUP=x\n";
        let merged = HermesProviderAdapter::merge_env_line(content, key, "v");
        assert_eq!(merged, "CUSTOM_PROVIDER_P_ANTH_KEY_BACKUP=x\nCUSTOM_PROVIDER_P_ANTH_KEY=v\n");
    }

    #[test]
    fn hermes_env_line_value_parses_plain_and_quoted() {
        let v = HermesProviderAdapter::env_line_value("K=abc\nX=1\n", "K");
        assert_eq!(v.as_deref(), Some("abc"));
        let v = HermesProviderAdapter::env_line_value("K=\"abc\"\n", "K");
        assert_eq!(v.as_deref(), Some("abc"));
        assert!(HermesProviderAdapter::env_line_value("K2=abc\n", "K").is_none());
        // 前缀相同的长键不误配
        assert!(HermesProviderAdapter::env_line_value("K_LONG=abc\n", "K").is_none());
    }

    #[test]
    fn hermes_model_keys_reads_yaml_projection() {
        let (default, provider, base_url) = HermesProviderAdapter::model_keys(
            "model:\n  default: MiniMax-M3\n  provider: minimax-sky\n  base_url: https://api.minimaxi.com/anthropic\nagent:\n  max_turns: 90\n",
        )
        .unwrap();
        assert_eq!(default.as_deref(), Some("MiniMax-M3"));
        assert_eq!(provider.as_deref(), Some("minimax-sky"));
        assert_eq!(base_url.as_deref(), Some("https://api.minimaxi.com/anthropic"));
        // model 节缺失 → 全 None
        let keys = HermesProviderAdapter::model_keys("agent:\n  verbose: false\n").unwrap();
        assert_eq!(keys, (None, None, None));
        // 坏 YAML → 错误而非 panic
        let err = HermesProviderAdapter::model_keys("model: [unclosed").unwrap_err();
        assert!(err.contains("parse"), "{}", err);
    }

    #[test]
    fn hermes_plan_add_update_unchanged_and_no_key_leak() {
        let home = TempHome::new();
        let providers = vec![anthropic_provider()];
        let a = HermesProviderAdapter;
        // 文件不存在 → add
        let changes = a.plan(home.path(), &providers, Some("p-anth"), &[]).unwrap();
        assert_eq!(changes[0].action, "add");
        assert!(!changes[0].detail.contains("sk-secret"), "detail must not leak apiKey");

        // 模拟已下发状态(config.yaml + .env)→ unchanged
        write_file(
            home.path(),
            &PathBuf::from(".hermes").join("config.yaml"),
            "model:\n  default: model-a\n  provider: p-anth\n  base_url: https://relay.example.com/anthropic\n",
        );
        write_file(
            home.path(),
            &PathBuf::from(".hermes").join(".env"),
            "OTHER=1\nCUSTOM_PROVIDER_P_ANTH_KEY=sk-secret-123\n",
        );
        let changes = a.plan(home.path(), &providers, Some("p-anth"), &[]).unwrap();
        assert_eq!(changes[0].action, "unchanged");

        // 端点变更 → update
        let mut moved = anthropic_provider();
        moved.anthropic_base_url = "https://relay2.example.com/anthropic".into();
        let changes = a.plan(home.path(), &[moved], Some("p-anth"), &[]).unwrap();
        assert_eq!(changes[0].action, "update");

        // .env key 过期 → update
        write_file(
            home.path(),
            &PathBuf::from(".hermes").join(".env"),
            "OTHER=1\nCUSTOM_PROVIDER_P_ANTH_KEY=stale\n",
        );
        let changes = a.plan(home.path(), &providers, Some("p-anth"), &[]).unwrap();
        assert_eq!(changes[0].action, "update");
    }

    #[test]
    fn hermes_slot_fallback_and_skip_cases() {
        let home = TempHome::new();
        let a = HermesProviderAdapter;
        // 只有 OpenAI 端点 → 兜底槽同样可下发(hermes 支持双协议)
        let providers = vec![openai_provider()];
        let changes = a.plan(home.path(), &providers, Some("p-oa"), &[]).unwrap();
        assert_eq!(changes[0].action, "add");
        assert!(changes[0].detail.contains("https://api.oa.example.com/v1"), "{}", changes[0].detail);
        assert_eq!(a.deployed_names(&providers, Some("p-oa")), vec!["CUSTOM_PROVIDER_P_OA_KEY"]);
        // 双端点 → Anthropic 槽优先
        let dual = vec![dual_provider()];
        let changes = a.plan(home.path(), &dual, Some("p-dual"), &[]).unwrap();
        assert!(
            changes[0].detail.contains("https://api.dual.example.com/anthropic"),
            "{}",
            changes[0].detail
        );
        // 两槽皆空 → skip「No endpoint configured」
        let none = vec![provider("p-none", "None", "", "")];
        let changes = a.plan(home.path(), &none, Some("p-none"), &[]).unwrap();
        assert_eq!(changes[0].action, "skip");
        assert!(changes[0].detail.contains("No endpoint configured"), "{}", changes[0].detail);
        // 未选择激活 → skip,无 remove(hermes 无 remove 语义)
        let changes = a.plan(home.path(), &providers, None, &[]).unwrap();
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].action, "skip");
        assert!(a.deployed_names(&providers, None).is_empty());
        // apply 在 skip 下不做任何事(不触 CLI、不写文件)
        assert_eq!(a.apply(home.path(), &providers, None, &[]).unwrap(), 0);
        assert!(!home.path().join(".hermes").exists());
    }

    #[test]
    fn hermes_apply_unchanged_short_circuits_without_cli() {
        // 已落盘状态下 apply 必须在 CLI 段之前短路返回 0 —— 这也是该
        // 测试能安全运行的原因(绝不真正执行 hermes CLI)。
        let home = TempHome::new();
        let providers = vec![anthropic_provider()];
        write_file(
            home.path(),
            &PathBuf::from(".hermes").join("config.yaml"),
            "model:\n  default: model-a\n  provider: p-anth\n  base_url: https://relay.example.com/anthropic\n",
        );
        write_file(
            home.path(),
            &PathBuf::from(".hermes").join(".env"),
            "CUSTOM_PROVIDER_P_ANTH_KEY=sk-secret-123\n",
        );
        assert_eq!(
            HermesProviderAdapter.apply(home.path(), &providers, Some("p-anth"), &[]).unwrap(),
            0
        );
    }

    #[test]
    fn hermes_corrupt_yaml_plans_error_not_panic() {
        let home = TempHome::new();
        write_file(
            home.path(),
            &PathBuf::from(".hermes").join("config.yaml"),
            "model: [unclosed",
        );
        let err = HermesProviderAdapter
            .plan(home.path(), &[anthropic_provider()], Some("p-anth"), &[])
            .unwrap_err();
        assert!(err.contains("parse"), "{}", err);
    }

    // ---- openclaw ----

    fn openclaw_rel() -> PathBuf {
        PathBuf::from(".openclaw").join("openclaw.json")
    }

    #[test]
    fn openclaw_creates_file_from_scratch_with_flavor_api_and_default_model() {
        let home = TempHome::new();
        let providers = vec![anthropic_provider(), openai_provider()];
        let a = OpenclawProviderAdapter;
        let changes = a.plan(home.path(), &providers, Some("p-anth"), &[]).unwrap();
        assert_eq!(changes.iter().filter(|c| c.action == "add").count(), 3); // 两家 + 默认模型
        for c in &changes {
            assert!(!c.detail.contains("sk-secret"), "detail must not leak apiKey: {}", c.detail);
        }

        assert_eq!(a.apply(home.path(), &providers, Some("p-anth"), &[]).unwrap(), 3);
        let doc = read_json(home.path(), &[".openclaw", "openclaw.json"]);
        let anth = &doc["models"]["providers"]["p-anth"];
        assert_eq!(anth["baseUrl"], json!("https://relay.example.com/anthropic"));
        assert_eq!(anth["api"], json!("anthropic-messages"));
        assert_eq!(anth["apiKey"], json!("sk-secret-123"));
        assert_eq!(anth["models"], json!([{"id": "model-a", "name": "model-a"}, {"id": "model-b", "name": "model-b"}]));
        let oa = &doc["models"]["providers"]["p-oa"];
        assert_eq!(oa["api"], json!("openai-completions"));
        // 默认模型 = 激活服务商,"provider/model" 引用格式
        assert_eq!(doc["agents"]["defaults"]["model"], json!("p-anth/model-a"));
        assert_eq!(
            a.deployed_names(&providers, Some("p-anth")),
            vec!["p-anth".to_string(), "p-oa".to_string()]
        );

        // 幂等
        let managed = vec!["p-anth".to_string(), "p-oa".to_string()];
        let changes = a.plan(home.path(), &providers, Some("p-anth"), &managed).unwrap();
        assert!(changes.iter().all(|c| c.action == "unchanged"), "{:?}", changes);
        assert_eq!(a.apply(home.path(), &providers, Some("p-anth"), &managed).unwrap(), 0);
    }

    #[test]
    fn openclaw_keeps_user_keys_and_removes_managed_diff() {
        let home = TempHome::new();
        write_file(
            home.path(),
            &openclaw_rel(),
            r#"{
              "gateway": {"port": 18789},
              "models": {
                "mode": "merge",
                "providers": {
                  "user-own": {"baseUrl": "https://mine", "api": "openai-completions"},
                  "gone-id": {"baseUrl": "https://old", "api": "openai-completions"}
                }
              },
              "agents": {"defaults": {"model": "gone-id/old-model", "workspace": "~/w"}}
            }"#,
        );
        let providers = vec![openai_provider()];
        let managed = vec!["gone-id".to_string()];
        let a = OpenclawProviderAdapter;
        let changes = a.plan(home.path(), &providers, None, &managed).unwrap();
        // gone-id 的 provider 条目 + 指向它的默认模型都进入 remove
        assert_eq!(changes.iter().filter(|c| c.action == "remove").count(), 2);

        a.apply(home.path(), &providers, None, &managed).unwrap();
        let doc = read_json(home.path(), &[".openclaw", "openclaw.json"]);
        assert!(doc["models"]["providers"].get("gone-id").is_none());
        assert!(doc["models"]["providers"].get("user-own").is_some());
        assert!(doc["models"]["providers"].get("p-oa").is_some());
        assert!(doc["agents"]["defaults"].get("model").is_none());
        // 用户其它键原样保留
        assert_eq!(doc["gateway"]["port"], json!(18789));
        assert_eq!(doc["models"]["mode"], json!("merge"));
        assert_eq!(doc["agents"]["defaults"]["workspace"], json!("~/w"));
    }

    #[test]
    fn openclaw_dual_endpoint_prefers_anthropic_slot() {
        let home = TempHome::new();
        let a = OpenclawProviderAdapter;
        a.apply(home.path(), &[dual_provider()], Some("p-dual"), &[]).unwrap();
        let doc = read_json(home.path(), &[".openclaw", "openclaw.json"]);
        let dual = &doc["models"]["providers"]["p-dual"];
        assert_eq!(dual["api"], json!("anthropic-messages"));
        assert_eq!(dual["baseUrl"], json!("https://api.dual.example.com/anthropic"));
        assert_eq!(doc["agents"]["defaults"]["model"], json!("p-dual/model-a"));
    }

    #[test]
    fn openclaw_default_model_object_form_only_touches_primary() {
        let home = TempHome::new();
        write_file(
            home.path(),
            &openclaw_rel(),
            r#"{"agents": {"defaults": {"model": {"primary": "openai/gpt-5.5", "fallbacks": ["x/y"]}}}}"#,
        );
        let providers = vec![anthropic_provider()];
        let a = OpenclawProviderAdapter;
        a.apply(home.path(), &providers, Some("p-anth"), &[]).unwrap();
        let doc = read_json(home.path(), &[".openclaw", "openclaw.json"]);
        assert_eq!(doc["agents"]["defaults"]["model"]["primary"], json!("p-anth/model-a"));
        assert_eq!(doc["agents"]["defaults"]["model"]["fallbacks"], json!(["x/y"]));
    }

    #[test]
    fn openclaw_user_default_model_untouched_when_not_ours() {
        // 用户自设默认模型且不属于 managed 差集:没有激活服务商时不碰。
        let home = TempHome::new();
        write_file(
            home.path(),
            &openclaw_rel(),
            r#"{"agents": {"defaults": {"model": "openai/gpt-5.5"}}}"#,
        );
        let providers = vec![openai_provider()];
        let a = OpenclawProviderAdapter;
        let changes = a.plan(home.path(), &providers, None, &[]).unwrap();
        assert!(!changes.iter().any(|c| c.name == "agents.defaults.model"), "{:?}", changes);
        a.apply(home.path(), &providers, None, &[]).unwrap();
        let doc = read_json(home.path(), &[".openclaw", "openclaw.json"]);
        assert_eq!(doc["agents"]["defaults"]["model"], json!("openai/gpt-5.5"));
    }

    #[test]
    fn openclaw_empty_default_model_or_base_url_edge_cases() {
        let home = TempHome::new();
        let a = OpenclawProviderAdapter;
        // defaultModel 为空 → 不管理默认模型键
        let mut p = openai_provider();
        p.default_model = String::new();
        a.apply(home.path(), &[p.clone()], Some("p-oa"), &[]).unwrap();
        let doc = read_json(home.path(), &[".openclaw", "openclaw.json"]);
        assert!(doc.get("agents").is_none());
        assert!(doc["models"]["providers"].get("p-oa").is_some());
        // 两槽皆空 → skip,且不进入 deployed_names
        let mut blank = openai_provider();
        blank.openai_base_url = " ".into();
        let changes = a.plan(home.path(), &[blank.clone()], None, &[]).unwrap();
        assert_eq!(changes[0].action, "skip");
        assert!(a.deployed_names(&[blank], None).is_empty());
        // models 列表为空 → 不写 models 键
        let mut no_models = openai_provider();
        no_models.models = vec![];
        let home2 = TempHome::new();
        a.apply(home2.path(), &[no_models], Some("p-oa"), &[]).unwrap();
        let doc = read_json(home2.path(), &[".openclaw", "openclaw.json"]);
        assert!(doc["models"]["providers"]["p-oa"].get("models").is_none());
    }

    #[test]
    fn openclaw_corrupt_json_or_bad_shape_plans_error() {
        let home = TempHome::new();
        write_file(home.path(), &openclaw_rel(), "{ nope");
        let err = OpenclawProviderAdapter
            .plan(home.path(), &[openai_provider()], None, &[])
            .unwrap_err();
        assert!(err.contains("parse"), "{}", err);

        let home = TempHome::new();
        write_file(home.path(), &openclaw_rel(), r#"{"models": "not-an-object"}"#);
        let err = OpenclawProviderAdapter
            .plan(home.path(), &[openai_provider()], None, &[])
            .unwrap_err();
        assert!(err.contains("not a JSON object"), "{}", err);
    }

    // ---- codebuddy(EnvSettingsProviderAdapter 的 OpenAI 槽实例) ----

    fn codebuddy_rel() -> PathBuf {
        PathBuf::from(".codebuddy").join("settings.json")
    }

    #[test]
    fn codebuddy_writes_three_env_keys_openai_slot() {
        let home = TempHome::new();
        let providers = vec![openai_provider()];
        let a = codebuddy();
        let changes = a.plan(home.path(), &providers, Some("p-oa"), &[]).unwrap();
        assert_eq!(changes[0].action, "add");
        assert!(!changes[0].detail.contains("sk-secret"), "detail must not leak apiKey");

        assert_eq!(a.apply(home.path(), &providers, Some("p-oa"), &[]).unwrap(), 1);
        let doc = read_json(home.path(), &[".codebuddy", "settings.json"]);
        assert_eq!(doc["env"]["CODEBUDDY_BASE_URL"], json!("https://api.oa.example.com/v1"));
        assert_eq!(doc["env"]["CODEBUDDY_API_KEY"], json!("sk-secret-123"));
        assert_eq!(doc["env"]["CODEBUDDY_MODEL"], json!("model-a"));
        // 幂等
        let managed = vec!["env".to_string()];
        let changes = a.plan(home.path(), &providers, Some("p-oa"), &managed).unwrap();
        assert_eq!(changes[0].action, "unchanged");
        assert_eq!(a.apply(home.path(), &providers, Some("p-oa"), &managed).unwrap(), 0);
        // defaultModel 为空 → CODEBUDDY_MODEL 不写/被移除
        let mut no_model = openai_provider();
        no_model.default_model = String::new();
        a.apply(home.path(), &[no_model], Some("p-oa"), &managed).unwrap();
        let doc = read_json(home.path(), &[".codebuddy", "settings.json"]);
        assert!(doc["env"].get("CODEBUDDY_MODEL").is_none());
    }

    #[test]
    fn codebuddy_merge_keeps_user_env_and_remove_only_managed_keys() {
        let home = TempHome::new();
        write_file(
            home.path(),
            &codebuddy_rel(),
            r#"{"model": "gpt-5", "env": {"NODE_ENV": "development", "CODEBUDDY_API_KEY": "user-own"}}"#,
        );
        let providers = vec![openai_provider()];
        let a = codebuddy();
        let changes = a.plan(home.path(), &providers, Some("p-oa"), &[]).unwrap();
        assert_eq!(changes[0].action, "update"); // 已有我们管理的键之一

        a.apply(home.path(), &providers, Some("p-oa"), &[]).unwrap();
        let doc = read_json(home.path(), &[".codebuddy", "settings.json"]);
        assert_eq!(doc["env"]["NODE_ENV"], json!("development"));
        assert_eq!(doc["model"], json!("gpt-5"));
        assert_eq!(doc["env"]["CODEBUDDY_API_KEY"], json!("sk-secret-123"));

        // 取消激活 → remove 只删三键,用户键保留
        let managed = vec!["env".to_string()];
        let changes = a.plan(home.path(), &providers, None, &managed).unwrap();
        assert!(changes.iter().any(|c| c.action == "remove" && c.name == "CODEBUDDY_*"));
        assert_eq!(a.apply(home.path(), &providers, None, &managed).unwrap(), 1);
        let doc = read_json(home.path(), &[".codebuddy", "settings.json"]);
        let env = doc["env"].as_object().unwrap();
        assert!(env.get("CODEBUDDY_BASE_URL").is_none());
        assert!(env.get("CODEBUDDY_API_KEY").is_none());
        assert!(env.get("CODEBUDDY_MODEL").is_none());
        assert_eq!(env["NODE_ENV"], json!("development"));
    }

    #[test]
    fn codebuddy_skips_without_openai_endpoint() {
        let home = TempHome::new();
        let providers = vec![anthropic_provider()]; // 只有 Anthropic 槽
        let a = codebuddy();
        let changes = a.plan(home.path(), &providers, Some("p-anth"), &[]).unwrap();
        assert_eq!(changes[0].action, "skip");
        assert!(changes[0].detail.contains("OpenAI endpoint not configured"), "{}", changes[0].detail);
        assert_eq!(a.apply(home.path(), &providers, Some("p-anth"), &[]).unwrap(), 0);
        assert!(a.deployed_names(&providers, Some("p-anth")).is_empty());
        assert!(!home.path().join(codebuddy_rel()).exists());
        // 双端点 → OpenAI 槽可用,正常下发
        let dual = vec![dual_provider()];
        let changes = a.plan(home.path(), &dual, Some("p-dual"), &[]).unwrap();
        assert_eq!(changes[0].action, "add");
        assert!(changes[0].detail.contains("https://api.dual.example.com/v1"), "{}", changes[0].detail);
    }

    // ---- kimi ----

    fn kimi_rel() -> PathBuf {
        PathBuf::from(".kimi").join("config.toml")
    }

    #[test]
    fn kimi_writes_providers_models_and_default_model() {
        let home = TempHome::new();
        let mut disabled = provider("p-off", "Off", "", "https://off.example.com");
        disabled.enabled = false;
        let providers = vec![anthropic_provider(), openai_provider(), disabled];
        let a = KimiProviderAdapter;
        let changes = a.plan(home.path(), &providers, Some("p-oa"), &[]).unwrap();
        // 两家 provider add + default_model add
        assert_eq!(changes.iter().filter(|c| c.action == "add").count(), 3);
        for c in &changes {
            assert!(!c.detail.contains("sk-secret"), "detail must not leak apiKey: {}", c.detail);
        }
        // 显示名映射:条目名 clawbox-p-anth → "Anthro Relay"
        assert!(changes.iter().any(|c| c.name == "Anthro Relay"), "{:?}", changes);

        assert_eq!(a.apply(home.path(), &providers, Some("p-oa"), &[]).unwrap(), 3);
        let text = std::fs::read_to_string(home.path().join(kimi_rel())).unwrap();
        // 槽位 → type 映射:anthropic-only → anthropic;openai-only → openai_legacy
        assert!(text.contains("[providers.clawbox-p-anth]"), "{}", text);
        assert!(text.contains("type = \"anthropic\""), "{}", text);
        assert!(text.contains("[providers.clawbox-p-oa]"), "{}", text);
        assert!(text.contains("type = \"openai_legacy\""), "{}", text);
        assert!(text.contains("base_url = \"https://api.oa.example.com/v1\""), "{}", text);
        // models 条目成对 + provider 指回自身
        assert!(text.contains("[models.clawbox-p-oa]"), "{}", text);
        assert!(text.contains("provider = \"clawbox-p-oa\""), "{}", text);
        assert!(text.contains("model = \"model-a\""), "{}", text);
        // 顶层默认模型 = 激活服务商条目名(必须指向 models 条目)
        assert!(text.contains("default_model = \"clawbox-p-oa\""), "{}", text);
        assert!(!text.contains("p-off"), "{}", text);
        assert_eq!(
            a.deployed_names(&providers, Some("p-oa")),
            vec!["clawbox-p-anth".to_string(), "clawbox-p-oa".to_string()]
        );

        // 幂等
        let managed = vec!["clawbox-p-anth".to_string(), "clawbox-p-oa".to_string()];
        let changes = a.plan(home.path(), &providers, Some("p-oa"), &managed).unwrap();
        assert!(changes.iter().all(|c| c.action == "unchanged"), "{:?}", changes);
        assert_eq!(a.apply(home.path(), &providers, Some("p-oa"), &managed).unwrap(), 0);
    }

    #[test]
    fn kimi_dual_endpoint_prefers_openai_slot() {
        let home = TempHome::new();
        let a = KimiProviderAdapter;
        a.apply(home.path(), &[dual_provider()], None, &[]).unwrap();
        let text = std::fs::read_to_string(home.path().join(kimi_rel())).unwrap();
        assert!(text.contains("type = \"openai_legacy\""), "{}", text);
        assert!(text.contains("base_url = \"https://api.dual.example.com/v1\""), "{}", text);
        assert!(!text.contains("anthropic"), "{}", text);
    }

    #[test]
    fn kimi_preserves_user_comments_and_entries() {
        let home = TempHome::new();
        write_file(
            home.path(),
            &kimi_rel(),
            "# my kimi config\ndefault_model = \"my-model\" # keep pointing\ntheme = \"light\"\n\n[providers.mine]\ntype = \"kimi\"\nbase_url = \"https://api.kimi.com/coding/v1\"\napi_key = \"user-key\"\n\n[models.my-model]\nprovider = \"mine\"\nmodel = \"kimi-k2\"\n",
        );
        let providers = vec![openai_provider()];
        let a = KimiProviderAdapter;
        // 无激活:不动用户的 default_model
        a.apply(home.path(), &providers, None, &[]).unwrap();
        let text = std::fs::read_to_string(home.path().join(kimi_rel())).unwrap();
        assert!(text.contains("# my kimi config"), "{}", text);
        assert!(text.contains("# keep pointing"), "{}", text);
        assert!(text.contains("theme = \"light\""), "{}", text);
        assert!(text.contains("[providers.mine]"), "{}", text);
        assert!(text.contains("[models.my-model]"), "{}", text);
        assert!(text.contains("default_model = \"my-model\""), "{}", text);
        assert!(text.contains("[providers.clawbox-p-oa]"), "{}", text);
        // 激活我们的服务商 → 顶层 default_model 指到我们的条目
        a.apply(home.path(), &providers, Some("p-oa"), &[]).unwrap();
        let text = std::fs::read_to_string(home.path().join(kimi_rel())).unwrap();
        assert!(text.contains("default_model = \"clawbox-p-oa\""), "{}", text);
        assert!(text.contains("[providers.mine]"), "{}", text);
    }

    #[test]
    fn kimi_managed_diff_removes_paired_entries_and_default() {
        let home = TempHome::new();
        let providers = vec![openai_provider()];
        let a = KimiProviderAdapter;
        a.apply(home.path(), &providers, Some("p-oa"), &[]).unwrap();

        // 服务商被删光,managed 记着旧条目 → 成对 remove + default_model 移除
        let managed = vec!["clawbox-p-oa".to_string()];
        let changes = a.plan(home.path(), &[], None, &managed).unwrap();
        assert!(changes.iter().any(|c| c.action == "remove"), "{:?}", changes);
        assert!(a.apply(home.path(), &[], None, &managed).unwrap() >= 1);
        let text = std::fs::read_to_string(home.path().join(kimi_rel())).unwrap();
        assert!(!text.contains("clawbox-p-oa"), "{}", text);
        assert!(!text.contains("default_model"), "{}", text);

        // 用户条目 + 我们的条目共存时,remove 只删我们的
        write_file(
            home.path(),
            &kimi_rel(),
            "[providers.mine]\ntype = \"kimi\"\nbase_url = \"https://api.kimi.com/coding/v1\"\napi_key = \"k\"\n\n[providers.clawbox-gone]\ntype = \"openai_legacy\"\nbase_url = \"https://old\"\napi_key = \"k\"\n",
        );
        let managed = vec!["clawbox-gone".to_string()];
        assert_eq!(a.apply(home.path(), &[], None, &managed).unwrap(), 1);
        let text = std::fs::read_to_string(home.path().join(kimi_rel())).unwrap();
        assert!(text.contains("[providers.mine]"), "{}", text);
        assert!(!text.contains("clawbox-gone"), "{}", text);
    }

    #[test]
    fn kimi_no_endpoint_skipped_and_corrupt_toml_errors() {
        let home = TempHome::new();
        let a = KimiProviderAdapter;
        let p = provider("p-none", "None", "", "  ");
        let changes = a.plan(home.path(), &[p.clone()], None, &[]).unwrap();
        assert_eq!(changes[0].action, "skip");
        assert!(changes[0].detail.contains("No endpoint configured"), "{}", changes[0].detail);
        assert!(a.deployed_names(&[p.clone()], None).is_empty());
        // 全 skip 的 apply 不创建文件
        assert_eq!(a.apply(home.path(), &[p], None, &[]).unwrap(), 0);
        assert!(!home.path().join(kimi_rel()).exists());

        write_file(home.path(), &kimi_rel(), "[broken\n");
        let err = a.plan(home.path(), &[openai_provider()], None, &[]).unwrap_err();
        assert!(err.contains("parse"), "{}", err);
    }

    // ---- 注册表 / plan_all ----

    #[test]
    fn provider_adapter_registry_matches_agent_registry() {
        for a in adapters() {
            assert!(
                crate::agents::find_agent(a.agent_id()).is_some(),
                "adapter {} not in agent registry",
                a.agent_id()
            );
        }
        assert!(find_adapter("node").is_none());
        assert_eq!(adapters().len(), 9);
        let supported: Vec<&str> = adapters()
            .iter()
            .filter(|a| a.supported())
            .map(|a| a.agent_id())
            .collect();
        assert_eq!(
            supported,
            vec!["claude-code", "codex", "openclaw", "opencode", "codebuddy", "kimi", "hermes"]
        );
    }

    #[test]
    fn plan_all_marks_unsupported_and_collects_errors() {
        let home = TempHome::new();
        // claude-code 目标文件损坏 → 该 agent error,其它照常
        write_file(
            home.path(),
            &PathBuf::from(".claude").join("settings.json"),
            "{ nope",
        );
        let providers = vec![anthropic_provider()];
        let bindings =
            std::collections::HashMap::from([("claude-code".to_string(), "p-anth".to_string())]);
        let plans = plan_all(home.path(), &providers, &bindings, &Default::default());
        assert_eq!(plans.len(), 9);
        let cc = plans.iter().find(|p| p.agent_id == "claude-code").unwrap();
        assert!(cc.error.is_some());
        let oc = plans.iter().find(|p| p.agent_id == "opencode").unwrap();
        assert!(oc.error.is_none());
        let kimi = plans.iter().find(|p| p.agent_id == "kimi").unwrap();
        assert!(kimi.supported && kimi.error.is_none());
        for id in ["cursor-agent", "qodercli"] {
            let p = plans.iter().find(|p| p.agent_id == id).unwrap();
            assert!(!p.supported);
        }
    }

    #[test]
    fn plan_all_uses_per_agent_bindings() {
        let home = TempHome::new();
        let providers = vec![anthropic_provider(), openai_provider()];
        let mut bindings = std::collections::HashMap::new();
        bindings.insert("claude-code".to_string(), "p-anth".to_string());
        bindings.insert("codex".to_string(), "p-oa".to_string());
        let managed = std::collections::HashMap::new();

        let plans = plan_all(home.path(), &providers, &bindings, &managed);
        let of = |id: &str| plans.iter().find(|p| p.agent_id == id).unwrap();

        // 绑定的 agent:按各自绑定的服务商出计划(未写盘 → add)
        assert!(of("claude-code").changes.iter().any(|c| c.action == "add"));
        assert!(of("codex").changes.iter().any(|c| c.action == "add"));
        // 未绑定的 agent:不管理即不看,零条目、无错误
        assert!(of("opencode").changes.is_empty());
        assert!(of("opencode").error.is_none());
        assert!(of("hermes").changes.is_empty());
    }

    #[test]
    fn plan_all_dangling_binding_is_empty_not_error() {
        let home = TempHome::new();
        let mut bindings = std::collections::HashMap::new();
        bindings.insert("claude-code".to_string(), "gone".to_string());
        let plans = plan_all(home.path(), &[], &bindings, &std::collections::HashMap::new());
        let p = plans.iter().find(|p| p.agent_id == "claude-code").unwrap();
        assert!(p.changes.is_empty());
        assert!(p.error.is_none());
    }

    #[test]
    fn apply_one_backs_up_existing_target() {
        let home = TempHome::new();
        write_file(
            home.path(),
            &PathBuf::from(".claude").join("settings.json"),
            r#"{"env": {}}"#,
        );
        let providers = vec![anthropic_provider()];
        let r = apply_one(home.path(), &claude_code(), &providers, Some("p-anth"), &[]);
        assert!(r.ok);
        assert!(r.backup_path.is_some());
        assert!(r.backup_path.unwrap().contains("claude-code__settings.json"));
    }
}
