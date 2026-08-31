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

use super::{diff_changes, AgentPlan, ApplyResult, ChangeItem, snapshots};
use crate::commands::config::ProviderSpec;
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use toml_edit::{value, DocumentMut, Item, Table};

pub trait ProviderAdapter: Send + Sync {
    fn agent_id(&self) -> &'static str;
    fn supported(&self) -> bool {
        true
    }
    /// 该适配器写的主配置文件(apply 前快照的对象)。
    fn config_path(&self, home: &Path) -> PathBuf;
    /// apply 可能触碰的全部路径(apply 前快照的范围)。默认主配置文件;
    /// CLI 型(空 config_path)为空。写多文件的适配器(codex)覆写。
    fn touch_paths(&self, home: &Path) -> Vec<PathBuf> {
        let p = self.config_path(home);
        if p.as_os_str().is_empty() { vec![] } else { vec![p] }
    }
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

    // ---- fallback 链(可选;默认不支持)---------------------------------------
    //
    // primary 之外的兑底服务商链。只有原生支持 fallback 的 agent(目前仅
    // hermes:config.yaml 根级 fallback_providers + 每家一条 custom_providers
    // 条目)覆盖这些方法;其它 agent 用默认 no-op,绑定 fallback 会被
    // agent_fallbacks_set_at 以「不支持」拒绝、不会产生条目。

    /// 该 agent 是否原生支持 fallback 链。
    fn supports_fallback(&self) -> bool {
        false
    }
    /// 单个服务商能否作为该 agent 的 fallback 下发(端点槽 + key + model 就绪)。
    /// 默认 false(不支持 fallback 的 agent 恒不可);支持者覆盖。
    fn fallback_deployable(&self, _spec: &ProviderSpec) -> bool {
        false
    }
    /// fallback 链的 plan(漂移检测)。默认空 vec = 不支持/无条目。
    fn plan_fallbacks(
        &self,
        _home: &Path,
        _fallbacks: &[ProviderSpec],
        _managed: &[String],
    ) -> Result<Vec<ChangeItem>, String> {
        Ok(vec![])
    }
    /// 应用 fallback 链。默认 Ok(0) = 不支持的 agent no-op。
    fn apply_fallbacks(
        &self,
        _home: &Path,
        _fallbacks: &[ProviderSpec],
        _managed: &[String],
    ) -> Result<usize, String> {
        Ok(0)
    }
    /// 本次会下发的 fallback managed 键名(custom_providers 条目名),成功后
    /// 写入 providers_fallback_managed。
    fn deployed_fallback_names(&self, _fallbacks: &[ProviderSpec]) -> Vec<String> {
        vec![]
    }

    /// 反向解析 agent 当前在用的服务商(agent → ClawBox「领养」)。默认 None
    /// (不支持/读不出 = 没东西可领养,不报错)。成功返回的条目用于
    /// agent_provider_adopt:在 ClawBox 里 upsert 一条同名 ProviderSpec 并绑定。
    fn extract_active(&self, _home: &Path) -> Result<Option<AdoptedProvider>, String> {
        Ok(None)
    }

    /// 写后结构校验:apply/apply_fallbacks 写完后由 apply_one 调用,失败则
    /// 自动 rollback 到 backup。默认 Ok(成立);各 adapter 覆盖为 agent 启动
    /// 所需的不变量(如 hermes 的 model.provider 必须能解析到一条
    /// custom_providers 条目——正是当年的 UUID bug 栽的地方)。只校验「文件
    /// 语法 + 结构不变量」,不跑 agent 本身(避免副作用/性能)。
    fn validate(&self, _home: &Path) -> Result<(), String> {
        Ok(())
    }
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

/// 从 agent 原生配置里反向解析出的「当前激活服务商」——
/// 用于 adopt(agent → ClawBox):让 ClawBox 学会 agent 现在用的端点/key/model。
#[derive(Clone, Debug, PartialEq)]
pub struct AdoptedProvider {
    /// 显示名(hermes custom_providers[].name / codex 表名;无则按 host 推导)。
    pub name: String,
    pub api_key: String,
    /// 完整端点 URL。
    pub base_url: String,
    /// 该端点应写入 ProviderSpec 的哪个槽。
    pub slot: Slot,
    pub default_model: String,
    /// 额外已知模型(至少含 default_model)。
    pub models: Vec<String>,
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

/// 从端点 URL 推导一个人类可读的服务商名(取 host 去掉 api./www.,首字母大写)。
/// 用于 adopt 时 agent 配置里没有显式名字(claude-code env / codex 表)的场景。
fn host_label(url: &str) -> String {
    let host = url
        .split("://")
        .nth(1)
        .unwrap_or(url)
        .split('/')
        .next()
        .unwrap_or(url);
    let host = host.trim_start_matches("api.").trim_start_matches("www.");
    let label = host.split('.').next().unwrap_or(host);
    let mut c = label.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::from("Provider"),
    }
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

    fn validate(&self, home: &Path) -> Result<(), String> {
        let path = self.config_path(home);
        if !path.exists() {
            return Ok(());
        }
        let doc = load_json(&path)?;
        let Some(env) = doc.get("env").and_then(|v| v.as_object()) else {
            return Ok(());
        };
        // BASE_URL 与 API_KEY 必须同时在场(只写了 URL 没 key,agent 会 401)。
        let [base_key, api_key, _model_key] = self.keys;
        let has_base = env.contains_key(base_key);
        let has_key = env.contains_key(api_key);
        if has_base != has_key {
            return Err(format!(
                "{}: env has {} but not {} (or vice versa); both must be set together",
                path.display(),
                base_key,
                api_key
            ));
        }
        Ok(())
    }

    fn extract_active(&self, home: &Path) -> Result<Option<AdoptedProvider>, String> {
        let path = self.config_path(home);
        if !path.exists() {
            return Ok(None);
        }
        let doc = load_json(&path)?;
        let Some(env) = doc.get("env").and_then(|v| v.as_object()) else {
            return Ok(None);
        };
        let [base_key, api_key, model_key] = self.keys;
        let base_url = env.get(base_key).and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
        let api_key_v = env.get(api_key).and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
        if base_url.is_empty() || api_key_v.is_empty() {
            return Ok(None);
        }
        let default_model = env.get(model_key).and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
        // claude-code 只认 anthropic 槽;codebuddy 只认 openai 槽——取适配器的首个槽。
        let slot = self.slots.first().copied().unwrap_or(Slot::Anthropic);
        Ok(Some(AdoptedProvider {
            name: host_label(&base_url),
            api_key: api_key_v,
            base_url,
            slot,
            default_model: default_model.clone(),
            models: if default_model.is_empty() { vec![] } else { vec![default_model] },
        }))
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

    /// codex 的 provider 下发同时写 config.toml(env/model 引用)、
    /// auth.json(OPENAI_API_KEY)与模型目录文件 —— 快照需覆盖全部三个。
    fn touch_paths(&self, home: &Path) -> Vec<PathBuf> {
        vec![self.config_path(home), self.auth_path(home), self.catalog_path(home)]
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
                    // 必须绝对路径:codex 反序列化为 AbsolutePathBuf,
                    // 相对路径会让 codex(含桌面版)启动即报错。
                    doc["model_catalog_json"] = value(catalog_path.to_string_lossy().as_ref());
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
                // 目录键与目录文件一并移除,但只动我们下发的那份:路径的文件名
                // 等于 CODEX_CATALOG_FILE 才删,用户自配的 model_catalog_json 保留。
                let ours = doc
                    .get("model_catalog_json")
                    .and_then(|i| i.as_str())
                    .map(|p| Path::new(p).file_name() == Some(std::ffi::OsStr::new(CODEX_CATALOG_FILE)))
                    .unwrap_or(false);
                if ours {
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

    fn validate(&self, home: &Path) -> Result<(), String> {
        let path = self.config_path(home);
        if !path.exists() {
            return Ok(());
        }
        let doc = self.load_toml(home)?;
        let mp = doc.get("model_provider").and_then(|i| i.as_str()).unwrap_or("");
        if mp.is_empty() {
            return Ok(()); // 未设 model_provider:不是我们管的态,放行
        }
        let Some(table) = doc
            .get("model_providers")
            .and_then(|i| i.as_table())
            .and_then(|t| t.get(mp))
        else {
            return Err(format!(
                "{}: model_provider='{}' but no [model_providers.{}] table",
                path.display(),
                mp,
                mp
            ));
        };
        let base = table.get("base_url").and_then(|i| i.as_str()).unwrap_or("");
        if base.trim().is_empty() {
            return Err(format!(
                "{}: [model_providers.{}] has no base_url",
                path.display(),
                mp
            ));
        }
        Ok(())
    }

    fn extract_active(&self, home: &Path) -> Result<Option<AdoptedProvider>, String> {
        let path = self.config_path(home);
        if !path.exists() {
            return Ok(None);
        }
        let doc = self.load_toml(home)?;
        let mp = doc.get("model_provider").and_then(|i| i.as_str()).unwrap_or("");
        if mp.is_empty() {
            return Ok(None);
        }
        let Some(table) = doc
            .get("model_providers")
            .and_then(|i| i.as_table())
            .and_then(|t| t.get(mp))
        else {
            return Ok(None);
        };
        let base_url = table.get("base_url").and_then(|i| i.as_str()).unwrap_or("").trim().to_string();
        if base_url.is_empty() {
            return Ok(None);
        }
        let auth = load_json(&self.auth_path(home))?;
        let api_key = auth
            .get("OPENAI_API_KEY")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        if api_key.is_empty() {
            return Ok(None);
        }
        let default_model = doc.get("model").and_then(|i| i.as_str()).unwrap_or("").trim().to_string();
        Ok(Some(AdoptedProvider {
            name: host_label(&base_url),
            api_key,
            base_url,
            slot: Slot::Openai,
            default_model: default_model.clone(),
            models: if default_model.is_empty() { vec![] } else { vec![default_model] },
        }))
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

// ---- hermes:~/.hermes/config.yaml custom_providers + model(单激活) -------
//
// 源码验证结论(hermes-agent 0.18.2,本机 ~/.hermes/hermes-agent 可编辑安装):
//
// * provider 解析:hermes 的 model.provider 必须是内置 provider 名
//   (anthropic/openai/...)或能路由到 custom_providers / providers 节里
//   某条命名条目 —— 写一个 ClawBox 内部 UUID 会让 hermes 启动即报
//   "Unknown provider"(auth.py resolve_provider)。因此我们下发一个
//   custom_providers 命名条目,并把 model.provider 设成 custom:<name>
//   显式路由(runtime_provider.py `_get_named_custom_provider`:按 name 归一
//   匹配;`custom:` 前缀绕过内置 provider 阴影检查)。
//
// * custom_providers 条目字段(name/base_url/api_key/api_mode/model/models)
//   取自 hermes 自身归一器与用户既有手建条目(config.py
//   `_normalize_custom_provider_entry` 的 _KNOWN_KEYS;`_get_named_custom_
//   provider` 解析 base_url/api_key/key_env/api_mode/model)。api_mode 由
//   端点槽决定:Anthropic 槽 → anthropic_messages,OpenAI 槽 →
//   chat_completions。内联 api_key 与本机既有手建条目同形(用户原
//   config.yaml 里 minimax-sky 等即内联 key)。
//
// * 写入方式:直接 YAML 读改写 ~/.hermes/config.yaml(serde_yaml)。
//   `hermes config set` 无法新增列表条目(`_set_nested` 不扩展列表),且
//   键名以 _API_KEY/_TOKEN 结尾会被改道 .env —— 故 custom_providers 条目
//   与内联 api_key 都只能直写文件。本机 config.yaml 无注释,serde_yaml 重排
//   格式无损。落盘用 temp+rename 原子写,避免半写损坏 hermes 配置。
//
// * managed 语义:deployed_names 记录本次下发的 custom_providers 条目名。
//   重绑到另一家时,按 managed 差集清掉旧条目再写新的(只动我们写过的条目;
//   用户手建同名条目会被接管为规范形)。取消激活(model.* / 条目)会破坏
//   hermes 自身运行(它总需要一个模型),故 Skip 只产出 skip、不清理 ——
//   与原实现一致。

pub struct HermesProviderAdapter;

/// serde_yaml 字符串值的小工具(hermes 段专用)。
fn ystr(s: &str) -> serde_yaml::Value {
    serde_yaml::Value::String(s.to_string())
}

/// custom_providers 条目里 ClawBox 关心的字段投影(忽略额外字段,保证用户
/// 加的 extra_body 等不触发反复重写;用 BTreeMap 比较,与字段顺序无关)。
#[derive(Clone, Debug, PartialEq)]
struct HermesEntry {
    base_url: String,
    api_key: String,
    api_mode: String,
    model: Option<String>,
    models: BTreeMap<String, String>,
}

/// hermes 当前落盘状态投影(plan/apply diff 用)。
struct HermesState {
    provider: Option<String>,
    base_url: Option<String>,
    default: Option<String>,
    entry: Option<HermesEntry>,
}

/// fallback_providers 列表条目的投影(plan/apply diff 用;忽略额外字段)。
#[derive(Clone, PartialEq)]
struct HermesFbEntry {
    provider: String,
    model: String,
    base_url: String,
    api_mode: String,
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

/// 行级删除:去掉所有 `KEY=` 行,其余行一字不动。全删空后返回空串。
fn remove_env_line(content: &str, key: &str) -> String {
    let lines: Vec<&str> = content
        .lines()
        .filter(|line| {
            !line
                .trim_start()
                .strip_prefix(key)
                .map(|rest| rest.starts_with('='))
                .unwrap_or(false)
        })
        .collect();
    if lines.is_empty() {
        String::new()
    } else {
        let mut out = lines.join("\n");
        out.push('\n');
        out
    }
}

/// hermes 端点偏好:Anthropic 优先、OpenAI 兜底(它按 URL 自检协议,见上)。
const HERMES_SLOTS: [Slot; 2] = [Slot::Anthropic, Slot::Openai];
const HERMES_MISSING: &str = "No endpoint configured";

impl HermesProviderAdapter {
    /// custom_providers 条目名 = 服务商显示名(人类可读,与用户手建条目同形)。
    fn entry_name(spec: &ProviderSpec) -> &str {
        spec.name.trim()
    }

    /// model.provider 值:`custom:<name>` 显式路由到命名 custom provider,
    /// 绕过 hermes 对裸名的内置 provider 阴影检查(见模块注释)。
    fn provider_ref(spec: &ProviderSpec) -> String {
        format!("custom:{}", Self::entry_name(spec))
    }

    /// 端点槽 → hermes api_mode。
    fn api_mode(slot: Slot) -> &'static str {
        match slot {
            Slot::Anthropic => "anthropic_messages",
            Slot::Openai => "chat_completions",
        }
    }

    /// 渲染 custom_providers 条目(内联 api_key,与用户手建条目同形)。
    fn render_entry(spec: &ProviderSpec, url: &str, slot: Slot) -> serde_yaml::Value {
        let mut m = serde_yaml::Mapping::new();
        m.insert(ystr("name"), ystr(Self::entry_name(spec)));
        m.insert(ystr("base_url"), ystr(url));
        m.insert(ystr("api_key"), ystr(spec.api_key.trim()));
        m.insert(ystr("api_mode"), ystr(Self::api_mode(slot)));
        let model = spec.default_model.trim();
        if !model.is_empty() {
            m.insert(ystr("model"), ystr(model));
        }
        if !spec.models.is_empty() {
            let mut models = serde_yaml::Mapping::new();
            for mid in &spec.models {
                let mid = mid.trim();
                if mid.is_empty() {
                    continue;
                }
                let mut inner = serde_yaml::Mapping::new();
                inner.insert(ystr("name"), ystr(mid));
                models.insert(ystr(mid), serde_yaml::Value::Mapping(inner));
            }
            m.insert(ystr("models"), serde_yaml::Value::Mapping(models));
        }
        serde_yaml::Value::Mapping(m)
    }

    fn desired_entry(spec: &ProviderSpec, url: &str, slot: Slot) -> HermesEntry {
        let model = spec.default_model.trim();
        let mut models = BTreeMap::new();
        for m in &spec.models {
            let m = m.trim();
            if !m.is_empty() {
                models.insert(m.to_string(), m.to_string());
            }
        }
        HermesEntry {
            base_url: url.to_string(),
            api_key: spec.api_key.trim().to_string(),
            api_mode: Self::api_mode(slot).to_string(),
            model: if model.is_empty() { None } else { Some(model.to_string()) },
            models,
        }
    }

    fn project_entry(v: &serde_yaml::Value) -> Option<HermesEntry> {
        let base_url = v.get("base_url")?.as_str()?.trim().to_string();
        let api_key = v
            .get("api_key")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        let api_mode = v
            .get("api_mode")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        let model = v.get("model").and_then(|x| x.as_str()).map(|s| s.trim().to_string());
        let mut models = BTreeMap::new();
        if let Some(mm) = v.get("models").and_then(|x| x.as_mapping()) {
            for (k, val) in mm.iter() {
                if let (Some(id), Some(name)) =
                    (k.as_str(), val.get("name").and_then(|n| n.as_str()))
                {
                    models.insert(id.trim().to_string(), name.trim().to_string());
                }
            }
        }
        Some(HermesEntry { base_url, api_key, api_mode, model, models })
    }

    /// 当前落盘投影:model.{provider,base_url,default} + 同名 custom_providers 条目。
    fn current_state(&self, home: &Path, name: &str) -> Result<HermesState, String> {
        let doc = self.read_yaml(home)?;
        let model = doc.get("model");
        let g = |k: &str| {
            model
                .and_then(|m| m.get(k))
                .and_then(|v| v.as_str())
                .map(|s| s.trim().to_string())
        };
        let entry = doc
            .get("custom_providers")
            .and_then(|v| v.as_sequence())
            .and_then(|seq| {
                seq.iter()
                    .find(|e| {
                        e.get("name")
                            .and_then(|n| n.as_str())
                            .map(|n| n.trim() == name)
                            .unwrap_or(false)
                    })
                    .and_then(Self::project_entry)
            });
        Ok(HermesState { provider: g("provider"), base_url: g("base_url"), default: g("default"), entry })
    }

    fn read_yaml(&self, home: &Path) -> Result<serde_yaml::Value, String> {
        let path = self.config_path(home);
        if !path.exists() {
            return Ok(serde_yaml::Value::Mapping(serde_yaml::Mapping::new()));
        }
        let text = std::fs::read_to_string(&path)
            .map_err(|e| format!("failed to read {}: {}", path.display(), e))?;
        if text.trim().is_empty() {
            return Ok(serde_yaml::Value::Mapping(serde_yaml::Mapping::new()));
        }
        serde_yaml::from_str(&text)
            .map_err(|e| format!("failed to parse {}: {}", path.display(), e))
    }

    /// 原子写(temp+rename),避免半写损坏 hermes 配置。
    fn write_yaml(&self, home: &Path, doc: &serde_yaml::Value) -> Result<(), String> {
        let path = self.config_path(home);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)
                .map_err(|e| format!("failed to create {}: {}", dir.display(), e))?;
        }
        let mut out = serde_yaml::to_string(doc)
            .map_err(|e| format!("failed to serialize {}: {}", path.display(), e))?;
        if !out.ends_with('\n') {
            out.push('\n');
        }
        let tmp = path.with_extension("yaml.tmp");
        std::fs::write(&tmp, &out).map_err(|e| format!("failed to write {}: {}", tmp.display(), e))?;
        std::fs::rename(&tmp, &path)
            .map_err(|e| format!("failed to rename {} -> {}: {}", tmp.display(), path.display(), e))
    }

    /// 不含敏感值的变更摘要。
    fn detail(spec: &ProviderSpec, url: &str) -> String {
        let model = spec.default_model.trim();
        format!(
            "provider={} · base_url={} · model={}",
            Self::provider_ref(spec),
            url,
            if model.is_empty() { "(not set)" } else { model }
        )
    }

    // ---- fallback 链 -------------------------------------------------------

    /// 解析 fallback 列表里每家的端点槽(无法解析 = 该家不可下发,跳过)。
    fn resolve_fallbacks<'a>(
        fallbacks: &'a [ProviderSpec],
    ) -> Vec<(&'a ProviderSpec, &'a str, Slot)> {
        fallbacks
            .iter()
            .filter_map(|s| pick_endpoint(s, &HERMES_SLOTS).map(|(u, sl)| (s, u, sl)))
            .collect()
    }

    /// fallback_providers 列表条目:{provider, model, base_url, api_mode}。
    fn render_fb_entry(spec: &ProviderSpec, url: &str, slot: Slot) -> serde_yaml::Value {
        let mut m = serde_yaml::Mapping::new();
        m.insert(ystr("provider"), ystr(&Self::provider_ref(spec)));
        m.insert(ystr("model"), ystr(spec.default_model.trim()));
        m.insert(ystr("base_url"), ystr(url));
        m.insert(ystr("api_mode"), ystr(Self::api_mode(slot)));
        serde_yaml::Value::Mapping(m)
    }

    /// 期望的 fallback_providers 投影列表(diff 用)。
    fn desired_fb_list(resolved: &[(&ProviderSpec, &str, Slot)]) -> Vec<HermesFbEntry> {
        resolved
            .iter()
            .map(|(s, url, slot)| HermesFbEntry {
                provider: Self::provider_ref(s),
                model: s.default_model.trim().to_string(),
                base_url: (*url).to_string(),
                api_mode: Self::api_mode(*slot).to_string(),
            })
            .collect()
    }

    /// fallback_providers 单条 → 投影。
    fn project_fb_entry(v: &serde_yaml::Value) -> Option<HermesFbEntry> {
        Some(HermesFbEntry {
            provider: v.get("provider")?.as_str()?.trim().to_string(),
            model: v.get("model")?.as_str()?.trim().to_string(),
            base_url: v.get("base_url").and_then(|x| x.as_str()).unwrap_or("").trim().to_string(),
            api_mode: v.get("api_mode").and_then(|x| x.as_str()).unwrap_or("").trim().to_string(),
        })
    }

    /// 归一化名称(与 hermes `_normalize_custom_provider_name` 一致:小写 + 空格转 -)。
    fn norm_name(s: &str) -> String {
        s.trim().to_lowercase().replace(' ', "-")
    }

    /// 校验 `custom:<name>`(或裸名)引用能解析到一条 base_url+api_key 齐全的
    /// custom_providers / providers 条目。非 custom: 形式且找不到也不报错
    /// (可能是内置 provider 名,我们无法穷举;ClawBox 只产 custom: 引用)。
    fn assert_provider_resolves(
        doc: &serde_yaml::Value,
        provider: &str,
        path: &Path,
    ) -> Result<(), String> {
        let provider = provider.trim();
        let name = provider.strip_prefix("custom:").unwrap_or(provider);
        let want = Self::norm_name(name);
        if want.is_empty() {
            return Ok(());
        }
        // custom_providers 列表(旧式,本机主用)
        let entry = doc
            .get("custom_providers")
            .and_then(|v| v.as_sequence())
            .into_iter()
            .flatten()
            .find(|e| {
                e.get("name")
                    .and_then(|n| n.as_str())
                    .map(|n| Self::norm_name(n) == want)
                    .unwrap_or(false)
            });
        if let Some(e) = entry {
            let base = e.get("base_url").and_then(|v| v.as_str()).unwrap_or("");
            let key = e.get("api_key").and_then(|v| v.as_str()).unwrap_or("");
            if base.trim().is_empty() {
                return Err(format!("{}: custom provider '{}' has no base_url", path.display(), name));
            }
            if key.trim().is_empty() {
                return Err(format!("{}: custom provider '{}' has no api_key", path.display(), name));
            }
            return Ok(());
        }
        // providers dict(新式)
        let in_dict = doc
            .get("providers")
            .and_then(|v| v.as_mapping())
            .into_iter()
            .flat_map(|m| m.iter())
            .any(|(k, _)| k.as_str().map(|s| Self::norm_name(s) == want).unwrap_or(false));
        if in_dict {
            Ok(())
        } else {
            // 裸名(非 custom:)且查不到 → 可能是内置 provider,放行,不误报。
            if provider.starts_with("custom:") {
                Err(format!(
                    "{}: provider '{}' has no matching custom_providers/providers entry",
                    path.display(),
                    provider
                ))
            } else {
                Ok(())
            }
        }
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
        let mut changes = Vec::new();
        match resolve_single_active(providers, active_id, &HERMES_SLOTS, HERMES_MISSING) {
            Target::Deploy { spec, url } => {
                let slot =
                    pick_endpoint(spec, &HERMES_SLOTS).map(|(_, s)| s).unwrap_or(Slot::Openai);
                let name = Self::entry_name(spec);
                let st = self.current_state(home, name)?;
                let desired = Self::desired_entry(spec, url, slot);
                let pref = Self::provider_ref(spec);
                let model = spec.default_model.trim();
                let unchanged = st.provider.as_deref() == Some(pref.as_str())
                    && st.base_url.as_deref() == Some(url)
                    && (model.is_empty() || st.default.as_deref() == Some(model))
                    && st.entry.as_ref() == Some(&desired);
                let action = if unchanged {
                    "unchanged"
                } else if st.provider.is_none() && st.entry.is_none() {
                    "add"
                } else {
                    "update"
                };
                changes.push(ChangeItem {
                    name: spec.name.clone(),
                    action: action.into(),
                    detail: if unchanged { String::new() } else { Self::detail(spec, url) },
                });
            }
            Target::Skip { name, reason } => {
                // 无 remove:清空 model.* / 删条目会破坏 hermes 自身运行(见模块注释)。
                changes.push(ChangeItem { name, action: "skip".into(), detail: reason });
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
        let (spec, url) = match resolve_single_active(providers, active_id, &HERMES_SLOTS, HERMES_MISSING) {
            Target::Deploy { spec, url } => (spec, url),
            Target::Skip { .. } => return Ok(0),
        };
        let slot = pick_endpoint(spec, &HERMES_SLOTS).map(|(_, s)| s).unwrap_or(Slot::Openai);
        let name = Self::entry_name(spec);
        let st = self.current_state(home, name)?;
        let desired = Self::desired_entry(spec, url, slot);
        let pref = Self::provider_ref(spec);
        let model = spec.default_model.trim();
        if st.provider.as_deref() == Some(pref.as_str())
            && st.base_url.as_deref() == Some(url)
            && (model.is_empty() || st.default.as_deref() == Some(model))
            && st.entry.as_ref() == Some(&desired)
        {
            return Ok(0);
        }
        let mut doc = self.read_yaml(home)?;
        if !doc.is_mapping() {
            doc = serde_yaml::Value::Mapping(serde_yaml::Mapping::new());
        }
        // 先以不可变读取出要保留的部分(owned),再取 root 可变引用改写。
        let kept: Vec<serde_yaml::Value> = doc
            .get("custom_providers")
            .and_then(|v| v.as_sequence())
            .map(|seq| {
                seq.iter()
                    .filter(|e| {
                        let ename =
                            e.get("name").and_then(|n| n.as_str()).map(|s| s.trim()).unwrap_or("");
                        if ename == name {
                            return false; // 同名:我们重写规范形
                        }
                        // managed 差集:我们曾以该名下发、现已不激活 → 丢弃
                        !managed.iter().any(|m| m == ename)
                    })
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        let mut model_node = doc
            .get("model")
            .filter(|v| v.is_mapping())
            .cloned()
            .unwrap_or_else(|| serde_yaml::Value::Mapping(serde_yaml::Mapping::new()));

        let root = doc.as_mapping_mut().unwrap();
        let mut new_list = kept;
        new_list.push(Self::render_entry(spec, url, slot));
        root.insert(ystr("custom_providers"), serde_yaml::Value::Sequence(new_list));

        {
            let mm = model_node.as_mapping_mut().unwrap();
            mm.insert(ystr("provider"), ystr(&pref));
            mm.insert(ystr("base_url"), ystr(url));
            if model.is_empty() {
                let dk = ystr("default");
                mm.remove(&dk);
            } else {
                mm.insert(ystr("default"), ystr(model));
            }
        }
        root.insert(ystr("model"), model_node);

        self.write_yaml(home, &doc)?;
        Ok(1)
    }

    fn deployed_names(&self, providers: &[ProviderSpec], active_id: Option<&str>) -> Vec<String> {
        match resolve_single_active(providers, active_id, &HERMES_SLOTS, HERMES_MISSING) {
            Target::Deploy { spec, .. } => vec![Self::entry_name(spec).to_string()],
            Target::Skip { .. } => vec![],
        }
    }

    // ---- fallback 链(hermes 原生支持)----

    fn supports_fallback(&self) -> bool {
        true
    }

    fn fallback_deployable(&self, spec: &ProviderSpec) -> bool {
        pick_endpoint(spec, &HERMES_SLOTS).is_some()
            && !spec.api_key.trim().is_empty()
            && !spec.default_model.trim().is_empty()
    }

    fn deployed_fallback_names(&self, fallbacks: &[ProviderSpec]) -> Vec<String> {
        Self::resolve_fallbacks(fallbacks)
            .iter()
            .map(|(s, _, _)| Self::entry_name(s).to_string())
            .collect()
    }

    fn plan_fallbacks(
        &self,
        home: &Path,
        fallbacks: &[ProviderSpec],
        managed: &[String],
    ) -> Result<Vec<ChangeItem>, String> {
        let resolved = Self::resolve_fallbacks(fallbacks);
        // 无 fallback 且从未管理 → 不出条目
        if resolved.is_empty() && managed.is_empty() {
            return Ok(vec![]);
        }
        let desired = Self::desired_fb_list(&resolved);
        let doc = self.read_yaml(home)?;
        let cur_fb: Vec<HermesFbEntry> = doc
            .get("fallback_providers")
            .and_then(|v| v.as_sequence())
            .map(|seq| seq.iter().filter_map(Self::project_fb_entry).collect())
            .unwrap_or_default();
        // 我们管理的 fallback custom_providers 条目名是否都齐
        let cur_cp_names: HashSet<String> = doc
            .get("custom_providers")
            .and_then(|v| v.as_sequence())
            .map(|seq| {
                seq.iter()
                    .filter_map(|e| e.get("name").and_then(|n| n.as_str()).map(|s| s.trim().to_string()))
                    .collect()
            })
            .unwrap_or_default();
        let desired_names: HashSet<String> = resolved
            .iter()
            .map(|(s, _, _)| Self::entry_name(s).to_string())
            .collect();
        let entries_ok = desired_names.iter().all(|n| cur_cp_names.contains(n));
        if cur_fb == desired && entries_ok {
            return Ok(vec![]);
        }
        let action = if desired.is_empty() {
            "remove"
        } else if cur_fb.is_empty() && managed.is_empty() {
            "add"
        } else {
            "update"
        };
        Ok(vec![ChangeItem {
            name: "fallback chain".to_string(),
            action: action.into(),
            detail: if desired.is_empty() {
                "clear fallback chain".to_string()
            } else {
                format!(
                    "{} fallback provider(s): {}",
                    desired.len(),
                    desired.iter().map(|f| f.provider.clone()).collect::<Vec<_>>().join(", ")
                )
            },
        }])
    }

    fn apply_fallbacks(
        &self,
        home: &Path,
        fallbacks: &[ProviderSpec],
        managed: &[String],
    ) -> Result<usize, String> {
        // 无 fallback 且无历史管理 → no-op,不碰文件
        if fallbacks.is_empty() && managed.is_empty() {
            return Ok(0);
        }
        let resolved = Self::resolve_fallbacks(fallbacks);
        let cur_names: HashSet<String> = resolved
            .iter()
            .map(|(s, _, _)| Self::entry_name(s).to_string())
            .collect();
        let mut doc = self.read_yaml(home)?;
        if !doc.is_mapping() {
            doc = serde_yaml::Value::Mapping(serde_yaml::Mapping::new());
        }
        // custom_providers:同名(当前 fallback)重写规范形;managed 差集(曾下发、
        // 现不在链)丢弃;其余(含 primary 条目、用户手建条目)一字不动。
        let kept: Vec<serde_yaml::Value> = doc
            .get("custom_providers")
            .and_then(|v| v.as_sequence())
            .map(|seq| {
                seq.iter()
                    .filter(|e| {
                        let ename =
                            e.get("name").and_then(|n| n.as_str()).map(|s| s.trim()).unwrap_or("");
                        if cur_names.contains(ename) {
                            return false; // 我们重写规范形
                        }
                        // managed 里、但不在当前链 → stale,丢弃
                        !managed.iter().any(|m| m == ename)
                    })
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        let mut new_cp = kept;
        for (s, url, slot) in &resolved {
            new_cp.push(Self::render_entry(s, url, *slot));
        }

        let root = doc.as_mapping_mut().unwrap();
        root.insert(ystr("custom_providers"), serde_yaml::Value::Sequence(new_cp));
        if resolved.is_empty() {
            root.remove(&ystr("fallback_providers"));
        } else {
            let fb_seq: Vec<serde_yaml::Value> = resolved
                .iter()
                .map(|(s, url, slot)| Self::render_fb_entry(s, url, *slot))
                .collect();
            root.insert(ystr("fallback_providers"), serde_yaml::Value::Sequence(fb_seq));
        }
        self.write_yaml(home, &doc)?;
        Ok(1)
    }

    fn validate(&self, home: &Path) -> Result<(), String> {
        let path = self.config_path(home);
        let doc = self.read_yaml(home)?;
        let provider = doc
            .get("model")
            .and_then(|m| m.get("provider"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if provider.trim().is_empty() {
            return Err(format!(
                "{}: model.provider is empty (hermes needs a model provider)",
                path.display()
            ));
        }
        Self::assert_provider_resolves(&doc, provider, &path)?;
        // fallback 链里每个 custom: 引用同样要能解析
        if let Some(seq) = doc.get("fallback_providers").and_then(|v| v.as_sequence()) {
            for (i, e) in seq.iter().enumerate() {
                if let Some(p) = e.get("provider").and_then(|v| v.as_str()) {
                    Self::assert_provider_resolves(&doc, p, &path)
                        .map_err(|err| format!("fallback_providers[{}]: {}", i, err))?;
                }
            }
        }
        Ok(())
    }

    fn extract_active(&self, home: &Path) -> Result<Option<AdoptedProvider>, String> {
        let doc = self.read_yaml(home)?;
        let provider = doc
            .get("model")
            .and_then(|m| m.get("provider"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if provider.trim().is_empty() {
            return Ok(None);
        }
        let raw_name = provider.strip_prefix("custom:").unwrap_or(provider);
        let want = Self::norm_name(raw_name);
        if want.is_empty() {
            return Ok(None);
        }
        // 先查 custom_providers 列表(本机主用),再查 providers dict(新式)
        let entry = doc
            .get("custom_providers")
            .and_then(|v| v.as_sequence())
            .into_iter()
            .flatten()
            .find(|e| {
                e.get("name")
                    .and_then(|n| n.as_str())
                    .map(|n| Self::norm_name(n) == want)
                    .unwrap_or(false)
            })
            .or_else(|| {
                doc.get("providers").and_then(|v| v.as_mapping()).and_then(|m| {
                    m.iter()
                        .find(|(k, _)| k.as_str().map(|s| Self::norm_name(s) == want).unwrap_or(false))
                        .map(|(_, v)| v)
                })
            });
        let Some(entry) = entry else { return Ok(None) };
        let base_url = entry.get("base_url").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
        let api_key = entry.get("api_key").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
        if base_url.is_empty() || api_key.is_empty() {
            return Ok(None);
        }
        let api_mode = entry.get("api_mode").and_then(|v| v.as_str()).unwrap_or("");
        let slot = if api_mode.trim() == "anthropic_messages" {
            Slot::Anthropic
        } else {
            Slot::Openai
        };
        let default_model = entry
            .get("model")
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .or_else(|| {
                doc.get("model")
                    .and_then(|m| m.get("default"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.trim().to_string())
            })
            .unwrap_or_default();
        let mut models: Vec<String> = Vec::new();
        if let Some(mm) = entry.get("models").and_then(|v| v.as_mapping()) {
            for (k, _) in mm.iter() {
                if let Some(id) = k.as_str() {
                    if !id.trim().is_empty() {
                        models.push(id.trim().to_string());
                    }
                }
            }
        }
        if !default_model.is_empty() && !models.contains(&default_model) {
            models.push(default_model.clone());
        }
        let name = entry
            .get("name")
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| raw_name.to_string());
        Ok(Some(AdoptedProvider { name, api_key, base_url, slot, default_model, models }))
    }
}

// ---- gemini:~/.gemini/.env 三键(单激活) ----------------------------------
//
// gemini-cli 官方支持 env 覆盖端点(联网核实 2026-07-31,geminicli.com 配置
// 文档 + LiteLLM 教程):GOOGLE_GEMINI_BASE_URL(要求 Gemini 协议端点,
// new-api 系网关的根地址即是;仅 gemini-api-key 认证下生效)+
// GEMINI_API_KEY + GEMINI_MODEL。~/.gemini/.env 自动加载(项目级 .env 优先
// 命中则整文件独占,家目录兜底 —— 我们只管家目录层)。
//
// 端点取 Anthropic 槽:网关根 URL 惯例上配在该槽;OpenAI 槽带 /v1 后缀,
// 对 Gemini 协议(路径 /v1beta/...)是错误前缀。auth 选择不代管:用户在
// CLI 内 /auth 自选;env key 在场时 gemini-api-key 即可用。

pub struct GeminiProviderAdapter;

const GEMINI_SLOTS: [Slot; 1] = [Slot::Anthropic];
const GEMINI_MISSING: &str = "Anthropic-slot (gateway root) endpoint not configured";
/// ~/.gemini/.env 里我们管理的三键;providers_managed 标记 ENV_MANAGED_MARK。
const GEMINI_ENV_KEYS: [&str; 3] = ["GOOGLE_GEMINI_BASE_URL", "GEMINI_API_KEY", "GEMINI_MODEL"];

impl GeminiProviderAdapter {
    /// 期望写入的键值(defaultModel 为空则不含 GEMINI_MODEL)。
    fn desired(spec: &ProviderSpec, url: &str) -> BTreeMap<&'static str, String> {
        let mut m = BTreeMap::new();
        m.insert(GEMINI_ENV_KEYS[0], url.to_string());
        m.insert(GEMINI_ENV_KEYS[1], spec.api_key.trim().to_string());
        let model = spec.default_model.trim();
        if !model.is_empty() {
            m.insert(GEMINI_ENV_KEYS[2], model.to_string());
        }
        m
    }

    fn read_env(&self, home: &Path) -> Result<String, String> {
        let path = self.config_path(home);
        if !path.exists() {
            return Ok(String::new());
        }
        std::fs::read_to_string(&path).map_err(|e| format!("failed to read {}: {}", path.display(), e))
    }

    /// 当前 .env 里三个管理键的投影(其它行忽略)。
    fn current(content: &str) -> BTreeMap<&'static str, String> {
        let mut m = BTreeMap::new();
        for key in GEMINI_ENV_KEYS {
            if let Some(v) = env_line_value(content, key) {
                m.insert(key, v);
            }
        }
        m
    }

    fn write_env(&self, home: &Path, content: &str) -> Result<(), String> {
        let path = self.config_path(home);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("failed to create {}: {}", dir.display(), e))?;
        }
        std::fs::write(&path, content).map_err(|e| format!("failed to write {}: {}", path.display(), e))
    }
}

impl ProviderAdapter for GeminiProviderAdapter {
    fn agent_id(&self) -> &'static str {
        "gemini"
    }

    fn config_path(&self, home: &Path) -> PathBuf {
        home.join(".gemini").join(".env")
    }

    fn plan(
        &self,
        home: &Path,
        providers: &[ProviderSpec],
        active_id: Option<&str>,
        managed: &[String],
    ) -> Result<Vec<ChangeItem>, String> {
        let env = self.read_env(home)?;
        let current = Self::current(&env);
        let mut changes = Vec::new();
        match resolve_single_active(providers, active_id, &GEMINI_SLOTS, GEMINI_MISSING) {
            Target::Deploy { spec, url } => {
                let desired = Self::desired(spec, url);
                let action = if current == desired {
                    "unchanged"
                } else if current.is_empty() {
                    "add"
                } else {
                    "update"
                };
                let model = spec.default_model.trim();
                changes.push(ChangeItem {
                    name: spec.name.clone(),
                    action: action.into(),
                    detail: if action == "unchanged" {
                        String::new()
                    } else {
                        format!(
                            "GOOGLE_GEMINI_BASE_URL={} · model={}",
                            url,
                            if model.is_empty() { "(not set)" } else { model }
                        )
                    },
                });
            }
            Target::Skip { name, reason } => {
                changes.push(ChangeItem { name, action: "skip".into(), detail: reason });
                if managed.iter().any(|m| m == ENV_MANAGED_MARK) && !current.is_empty() {
                    changes.push(ChangeItem {
                        name: "GEMINI_*".into(),
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
        let env = self.read_env(home)?;
        let current = Self::current(&env);
        match resolve_single_active(providers, active_id, &GEMINI_SLOTS, GEMINI_MISSING) {
            Target::Deploy { spec, url } => {
                let desired = Self::desired(spec, url);
                if current == desired {
                    return Ok(0);
                }
                let mut content = env;
                for key in GEMINI_ENV_KEYS {
                    content = match desired.get(key) {
                        Some(v) => merge_env_line(&content, key, v),
                        None => remove_env_line(&content, key),
                    };
                }
                self.write_env(home, &content)?;
                Ok(1)
            }
            Target::Skip { .. } => {
                // 无可下发目标:只有曾管理过才清理我们的三行,其余行不动。
                if !managed.iter().any(|m| m == ENV_MANAGED_MARK) || current.is_empty() {
                    return Ok(0);
                }
                let mut content = env;
                for key in GEMINI_ENV_KEYS {
                    content = remove_env_line(&content, key);
                }
                self.write_env(home, &content)?;
                Ok(1)
            }
        }
    }

    fn deployed_names(&self, providers: &[ProviderSpec], active_id: Option<&str>) -> Vec<String> {
        match resolve_single_active(providers, active_id, &GEMINI_SLOTS, GEMINI_MISSING) {
            Target::Deploy { .. } => vec![ENV_MANAGED_MARK.to_string()],
            Target::Skip { .. } => vec![],
        }
    }
}

// ---- cline:providers.json 经 cline auth CLI(单激活) ----------------------
//
// cline 的 ~/.cline/data/settings/providers.json 由其 ProviderSettingsManager
// 维护(version/tokenSource 等内部字段),schema 未公开 —— 不盲写文件,改走
// 官方非交互 CLI(cline 3.0.15 `auth --help` 本机核实):
//   cline auth -p anthropic -k <key> -b <url> [-m <model>]
// 端点取 Anthropic 槽(cline 的 anthropic provider 支持自定义 base URL;
// Messages 协议 base 不带 /v1)。unchanged 检测对 providers.json 做宽松投影
// (键名容错),读不出就重跑 auth(幂等)。无 remove 语义:cline 总需要一个
// 可用 provider,解绑保留现值。CLI 固定写真实 ~/.cline,同 hermes 铁律:
// 测试只测纯函数与文件投影,不跑 CLI。

pub struct ClineProviderAdapter;

const CLINE_SLOTS: [Slot; 1] = [Slot::Anthropic];
const CLINE_MISSING: &str = "Anthropic endpoint not configured";
/// providers_managed 标记:表示我们经 cline auth 配过。
const CLINE_MANAGED_MARK: &str = "auth";

impl ClineProviderAdapter {
    /// `cline auth` 参数组(纯函数,可测)。
    fn auth_args(spec: &ProviderSpec, url: &str) -> Vec<String> {
        let mut args: Vec<String> = vec![
            "auth".into(),
            "-p".into(),
            "anthropic".into(),
            "-k".into(),
            spec.api_key.trim().into(),
            "-b".into(),
            url.into(),
        ];
        let model = spec.default_model.trim();
        if !model.is_empty() {
            args.push("-m".into());
            args.push(model.into());
        }
        args
    }

    /// providers.json 里 anthropic 条目的 (apiKey, model, baseUrl) 宽松投影;
    /// 文件/条目缺失 = 全 None。
    fn current(&self, home: &Path) -> (Option<String>, Option<String>, Option<String>) {
        let path = self.config_path(home);
        let Ok(text) = std::fs::read_to_string(&path) else {
            return (None, None, None);
        };
        let Ok(doc) = serde_json::from_str::<Value>(&text) else {
            return (None, None, None);
        };
        let settings = doc
            .get("providers")
            .and_then(|p| p.get("anthropic"))
            .and_then(|e| e.get("settings"));
        let get = |keys: &[&str]| -> Option<String> {
            let s = settings?;
            keys.iter()
                .find_map(|k| s.get(*k).and_then(|v| v.as_str()).map(|v| v.to_string()))
        };
        (
            get(&["apiKey"]),
            get(&["model", "modelId"]),
            get(&["baseUrl", "anthropicBaseUrl", "baseURL"]),
        )
    }

    fn is_unchanged(
        spec: &ProviderSpec,
        url: &str,
        cur: &(Option<String>, Option<String>, Option<String>),
    ) -> bool {
        let model = spec.default_model.trim();
        let model_ok = model.is_empty() || cur.1.as_deref() == Some(model);
        cur.0.as_deref() == Some(spec.api_key.trim()) && model_ok && cur.2.as_deref() == Some(url)
    }

    fn run_cli(args: &[String]) -> Result<(), String> {
        let output = crate::proc::command("cline")
            .args(args)
            .output()
            .map_err(|e| format!("failed to run cline CLI: {}", e))?;
        if !output.status.success() {
            return Err(format!(
                "cline auth failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
        Ok(())
    }
}

impl ProviderAdapter for ClineProviderAdapter {
    fn agent_id(&self) -> &'static str {
        "cline"
    }

    fn config_path(&self, home: &Path) -> PathBuf {
        home.join(".cline").join("data").join("settings").join("providers.json")
    }

    fn plan(
        &self,
        home: &Path,
        providers: &[ProviderSpec],
        active_id: Option<&str>,
        _managed: &[String],
    ) -> Result<Vec<ChangeItem>, String> {
        let mut changes = Vec::new();
        match resolve_single_active(providers, active_id, &CLINE_SLOTS, CLINE_MISSING) {
            Target::Deploy { spec, url } => {
                let cur = self.current(home);
                let action = if Self::is_unchanged(spec, url, &cur) {
                    "unchanged"
                } else if cur.0.is_none() {
                    "add"
                } else {
                    "update"
                };
                let model = spec.default_model.trim();
                changes.push(ChangeItem {
                    name: spec.name.clone(),
                    action: action.into(),
                    detail: if action == "unchanged" {
                        String::new()
                    } else {
                        format!(
                            "via `cline auth` · base={} · model={}",
                            url,
                            if model.is_empty() { "(keep)" } else { model }
                        )
                    },
                });
            }
            Target::Skip { name, reason } => {
                changes.push(ChangeItem { name, action: "skip".into(), detail: reason });
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
        let (spec, url) = match resolve_single_active(providers, active_id, &CLINE_SLOTS, CLINE_MISSING) {
            Target::Deploy { spec, url } => (spec, url),
            Target::Skip { .. } => return Ok(0),
        };
        if Self::is_unchanged(spec, url, &self.current(home)) {
            return Ok(0);
        }
        Self::run_cli(&Self::auth_args(spec, url))?;
        Ok(1)
    }

    fn deployed_names(&self, providers: &[ProviderSpec], active_id: Option<&str>) -> Vec<String> {
        match resolve_single_active(providers, active_id, &CLINE_SLOTS, CLINE_MISSING) {
            Target::Deploy { .. } => vec![CLINE_MANAGED_MARK.to_string()],
            Target::Skip { .. } => vec![],
        }
    }
}

// ---- pi:~/.pi/agent/models.json provider 节点 + settings.json 默认两键 ----
//
// pi(badlogic/pi-coding-agent)自定义服务商走 models.json(docs/models.md,
// 联网核实 2026-07-31):providers.<id> = { baseUrl, api, apiKey,
// models: [{id}] }。api 按命中槽定协议:Anthropic 槽 → anthropic-messages,
// OpenAI 槽 → openai-completions(Anthropic 优先)。默认模型写 settings.json
// 的 defaultProvider/defaultModel 两键(本机 pi 0.83 核实存在)。解绑:删
// models.json 里我们的节点;settings 两键保留(pi 总要有默认可用)。

pub struct PiProviderAdapter;

const PI_SLOTS: [Slot; 2] = [Slot::Anthropic, Slot::Openai];
const PI_MISSING: &str = "No endpoint configured";

impl PiProviderAdapter {
    fn agent_dir(home: &Path) -> PathBuf {
        home.join(".pi").join("agent")
    }

    fn settings_path(home: &Path) -> PathBuf {
        Self::agent_dir(home).join("settings.json")
    }

    /// 期望的 models.json provider 节点。
    fn desired_node(spec: &ProviderSpec, url: &str, slot: Slot) -> Value {
        let api = match slot {
            Slot::Anthropic => "anthropic-messages",
            Slot::Openai => "openai-completions",
        };
        let mut models: Vec<String> = spec
            .models
            .iter()
            .map(|m| m.trim().to_string())
            .filter(|m| !m.is_empty())
            .collect();
        let default_model = spec.default_model.trim();
        if models.is_empty() && !default_model.is_empty() {
            models.push(default_model.to_string());
        }
        json!({
            "baseUrl": url,
            "api": api,
            "apiKey": spec.api_key.trim(),
            "models": models.iter().map(|m| json!({"id": m})).collect::<Vec<_>>(),
        })
    }

    /// settings.json 期望的两键(defaultModel 为空则只下 defaultProvider)。
    fn desired_settings(spec: &ProviderSpec) -> Vec<(&'static str, String)> {
        let mut kv = vec![("defaultProvider", spec.id.clone())];
        let model = spec.default_model.trim();
        if !model.is_empty() {
            kv.push(("defaultModel", model.to_string()));
        }
        kv
    }

    fn settings_unchanged(doc: &Value, kv: &[(&'static str, String)]) -> bool {
        kv.iter()
            .all(|(k, v)| doc.get(*k).and_then(|x| x.as_str()) == Some(v.as_str()))
    }
}

impl ProviderAdapter for PiProviderAdapter {
    fn agent_id(&self) -> &'static str {
        "pi"
    }

    fn config_path(&self, home: &Path) -> PathBuf {
        Self::agent_dir(home).join("models.json")
    }

    fn plan(
        &self,
        home: &Path,
        providers: &[ProviderSpec],
        active_id: Option<&str>,
        managed: &[String],
    ) -> Result<Vec<ChangeItem>, String> {
        let models_doc = load_json(&self.config_path(home))?;
        let node_of = |id: &str| models_doc.get("providers").and_then(|p| p.get(id)).cloned();
        let mut changes = Vec::new();
        match resolve_single_active(providers, active_id, &PI_SLOTS, PI_MISSING) {
            Target::Deploy { spec, url } => {
                let (_, slot) = pick_endpoint(spec, &PI_SLOTS).expect("Deploy implies a slot");
                let desired = Self::desired_node(spec, url, slot);
                let settings_doc = load_json(&Self::settings_path(home))?;
                let settings_ok = Self::settings_unchanged(&settings_doc, &Self::desired_settings(spec));
                let action = match node_of(&spec.id) {
                    Some(existing) if existing == desired && settings_ok => "unchanged",
                    Some(_) => "update",
                    None => "add",
                };
                let model = spec.default_model.trim();
                changes.push(ChangeItem {
                    name: spec.name.clone(),
                    action: action.into(),
                    detail: if action == "unchanged" {
                        String::new()
                    } else {
                        format!(
                            "models.json providers.{} · base={} · model={}",
                            spec.id,
                            url,
                            if model.is_empty() { "(not set)" } else { model }
                        )
                    },
                });
            }
            Target::Skip { name, reason } => {
                changes.push(ChangeItem { name, action: "skip".into(), detail: reason });
                for id in managed {
                    if node_of(id).is_some() {
                        changes.push(ChangeItem {
                            name: id.clone(),
                            action: "remove".into(),
                            detail: "no longer managed by ClawBox".into(),
                        });
                    }
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
        let models_path = self.config_path(home);
        let mut models_doc = load_json(&models_path)?;
        match resolve_single_active(providers, active_id, &PI_SLOTS, PI_MISSING) {
            Target::Deploy { spec, url } => {
                let (_, slot) = pick_endpoint(spec, &PI_SLOTS).expect("Deploy implies a slot");
                let desired = Self::desired_node(spec, url, slot);
                let mut applied = 0;

                let root = models_doc.as_object_mut().unwrap();
                let nodes = root
                    .entry("providers".to_string())
                    .or_insert_with(|| Value::Object(Map::new()))
                    .as_object_mut()
                    .ok_or_else(|| "\"providers\" is not a JSON object".to_string())?;
                if nodes.get(&spec.id) != Some(&desired) {
                    nodes.insert(spec.id.clone(), desired);
                    applied += 1;
                }
                // 曾管理、现不再下发的节点(换绑了别的服务商)一并清掉。
                for id in managed {
                    if id != &spec.id && nodes.remove(id).is_some() {
                        applied += 1;
                    }
                }
                if applied > 0 {
                    write_json(&models_path, &models_doc)?;
                }

                let settings_path = Self::settings_path(home);
                let mut settings_doc = load_json(&settings_path)?;
                let kv = Self::desired_settings(spec);
                if !Self::settings_unchanged(&settings_doc, &kv) {
                    let obj = settings_doc.as_object_mut().unwrap();
                    for (k, v) in &kv {
                        obj.insert((*k).to_string(), json!(v));
                    }
                    write_json(&settings_path, &settings_doc)?;
                    applied += 1;
                }
                Ok(applied)
            }
            Target::Skip { .. } => {
                // 解绑清理:只删我们写过的 models.json 节点,settings 两键保留。
                let Some(nodes) = models_doc.get_mut("providers").and_then(|p| p.as_object_mut()) else {
                    return Ok(0);
                };
                let mut applied = 0;
                for id in managed {
                    if nodes.remove(id).is_some() {
                        applied += 1;
                    }
                }
                if applied > 0 {
                    write_json(&models_path, &models_doc)?;
                }
                Ok(applied)
            }
        }
    }

    fn deployed_names(&self, providers: &[ProviderSpec], active_id: Option<&str>) -> Vec<String> {
        match resolve_single_active(providers, active_id, &PI_SLOTS, PI_MISSING) {
            Target::Deploy { spec, .. } => vec![spec.id.clone()],
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
    /// `new_format` = 目标是 ~/.kimi-code(Kimi Code 0.31+ schema,kimi
    /// doctor 核实 2026-07-31):openai_legacy 更名 openai,models 条目必须带
    /// max_context_size(数字)。旧 ~/.kimi 保持旧 schema。
    fn render(spec: &ProviderSpec, new_format: bool) -> Result<Value, String> {
        let Some((url, slot)) = pick_endpoint(spec, &KIMI_SLOTS) else {
            return Err("No endpoint configured".to_string());
        };
        let kind = match slot {
            Slot::Openai => {
                if new_format {
                    "openai"
                } else {
                    "openai_legacy"
                }
            }
            Slot::Anthropic => "anthropic",
        };
        let name = Self::entry_name(&spec.id);
        let model = spec.default_model.trim();
        Ok(json!({
            "provider": {"type": kind, "base_url": url, "api_key": spec.api_key.trim()},
            "model": if model.is_empty() {
                Value::Null
            } else if new_format {
                // max_context_size 必填;取 128k 保守通用值(主流模型下限)。
                json!({"provider": name, "model": model, "max_context_size": 128000})
            } else {
                json!({"provider": name, "model": model})
            },
        }))
    }

    /// 全部 enabled 服务商按条目名(clawbox-<id>)渲染。
    fn mapped(providers: &[ProviderSpec], new_format: bool) -> BTreeMap<String, Result<Value, String>> {
        providers
            .iter()
            .filter(|p| p.enabled)
            .map(|p| (Self::entry_name(&p.id), Self::render(p, new_format)))
            .collect()
    }

    /// 目标是否新数据根(~/.kimi-code);同时决定 config_path 与 schema。
    fn uses_new_root(home: &Path) -> bool {
        home.join(".kimi-code").exists()
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

    /// Kimi CLI 改名 Kimi Code 后数据根从 ~/.kimi 迁至 ~/.kimi-code(config
    /// 结构不变;官方 config-files 文档核实 2026-07-31)。新目录存在即优先,
    /// 否则回落旧目录(老版本 CLI 仍读 ~/.kimi)。
    fn config_path(&self, home: &Path) -> PathBuf {
        let new_root = home.join(".kimi-code");
        if new_root.exists() {
            new_root.join("config.toml")
        } else {
            home.join(".kimi").join("config.toml")
        }
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
        let mapped = Self::mapped(providers, Self::uses_new_root(home));
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
        let mapped = Self::mapped(providers, Self::uses_new_root(home));
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
                if let Some(mcs) = desired["model"]["max_context_size"].as_i64() {
                    m["max_context_size"] = value(mcs);
                }
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
        // 只取条目名集合,新旧 schema 的 Ok/Err 判定一致,格式随便取。
        Self::mapped(providers, false)
            .into_iter()
            .filter(|(_, r)| r.is_ok())
            .map(|(name, _)| name)
            .collect()
    }
}

// ---- dsh(DeepSeek Harness):~/.dsh/settings.yaml + .credentials.yaml ----
//
// dsh 的自定义服务商走 settings.yaml 的 llm-pi-ai.providers.<route>
// (docs/user/guide/providers.md,联网核实 2026-08-24):{apiKeyEnv(凭据引用,
// 即环境变量名), api(协议), baseURL, models: [{id}]}。key 不落在
// settings 里,而是存 ~/.dsh/.credentials.yaml 的 refs 节(ref → 明文);
// dsh 要求该文件 0600,否则直接拒读。api 按命中槽定协议:Anthropic 槽
// → anthropic-messages,OpenAI 槽 → openai-completions(Anthropic 优先)。
// 解绑:删我们的 route 与 refs 键;其余用户配置一律不动。

pub struct DshProviderAdapter;

const DSH_SLOTS: [Slot; 2] = [Slot::Anthropic, Slot::Openai];
const DSH_MISSING: &str = "No endpoint configured";
/// settings.yaml 里我们占用的路由键(dsh 路由键永久,固定名便于 remove)。
const DSH_ROUTE: &str = "clawbox";
/// 凭据引用名(环境变量名语法 [A-Za-z_][A-Za-z0-9_]*,且避开常见真名)。
const DSH_KEY_REF: &str = "CLAWBOX_DSH_API_KEY";

impl DshProviderAdapter {
    fn dsh_dir(home: &Path) -> PathBuf {
        home.join(".dsh")
    }

    fn settings_path(home: &Path) -> PathBuf {
        Self::dsh_dir(home).join("settings.yaml")
    }

    fn credentials_path(home: &Path) -> PathBuf {
        Self::dsh_dir(home).join(".credentials.yaml")
    }

    fn read_yaml(path: &Path) -> Result<serde_yaml::Value, String> {
        if !path.exists() {
            return Ok(serde_yaml::Value::Mapping(serde_yaml::Mapping::new()));
        }
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("failed to read {}: {}", path.display(), e))?;
        serde_yaml::from_str(&content)
            .map_err(|e| format!("failed to parse {}: {}", path.display(), e))
    }

    fn write_yaml(path: &Path, doc: &serde_yaml::Value) -> Result<(), String> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)
                .map_err(|e| format!("failed to create {}: {}", dir.display(), e))?;
        }
        // credentials 文件必须 0600(dsh 拒读 group/other 可读的文件)。
        #[cfg(unix)]
        if path.file_name().and_then(|n| n.to_str()) == Some(".credentials.yaml") {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
        }
        let text = serde_yaml::to_string(doc)
            .map_err(|e| format!("failed to serialize {}: {}", path.display(), e))?;
        std::fs::write(path, text).map_err(|e| format!("failed to write {}: {}", path.display(), e))?;
        #[cfg(unix)]
        if path.file_name().and_then(|n| n.to_str()) == Some(".credentials.yaml") {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
                .map_err(|e| format!("failed to chmod {}: {}", path.display(), e))?;
        }
        Ok(())
    }

    /// settings.yaml 里我们的 route 节点(不存在 = None)。
    fn our_route(doc: &serde_yaml::Value) -> Option<&serde_yaml::Value> {
        doc.get("llm-pi-ai")
            .and_then(|v| v.get("providers"))
            .and_then(|v| v.get(DSH_ROUTE))
    }

    /// credentials refs 节里我们的 key(空串 = 未配置;dsh 空值视为缺)。
    fn our_key(doc: &serde_yaml::Value) -> String {
        doc.get("refs")
            .and_then(|v| v.get(DSH_KEY_REF))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string()
    }

    /// 模型目录:models 优先,空则 default_model 兀底;都空 = None(dsh
    /// 自定义服务商要求至少一个模型,由 plan/apply 以 skip 拒绝)。
    fn model_ids(spec: &ProviderSpec) -> Vec<String> {
        let mut ids: Vec<String> = spec
            .models
            .iter()
            .map(|m| m.trim().to_string())
            .filter(|m| !m.is_empty())
            .collect();
        let dm = spec.default_model.trim();
        if ids.is_empty() && !dm.is_empty() {
            ids.push(dm.to_string());
        }
        ids
    }

    fn api_of(slot: Slot) -> &'static str {
        match slot {
            Slot::Anthropic => "anthropic-messages",
            Slot::Openai => "openai-completions",
        }
    }

    fn detail(spec: &ProviderSpec, url: &str) -> String {
        format!(
            "baseURL={} · api={} · models={}",
            url,
            "auto",
            Self::model_ids(spec).join(",")
        )
    }

    /// 期望的 route 节点(不含 key;key 在 credentials)。
    fn desired_route(spec: &ProviderSpec, url: &str, slot: Slot) -> serde_yaml::Value {
        let mut m = serde_yaml::Mapping::new();
        m.insert(ystr("apiKeyEnv"), ystr(DSH_KEY_REF));
        m.insert(ystr("api"), ystr(Self::api_of(slot)));
        m.insert(ystr("baseURL"), ystr(url));
        m.insert(
            ystr("models"),
            serde_yaml::Value::Sequence(
                Self::model_ids(spec)
                    .into_iter()
                    .map(|id| {
                        let mut mm = serde_yaml::Mapping::new();
                        mm.insert(ystr("id"), ystr(&id));
                        serde_yaml::Value::Mapping(mm)
                    })
                    .collect(),
            ),
        );
        serde_yaml::Value::Mapping(m)
    }

    /// route 节点的等价比较投影(只看我们下发的四个键)。
    fn route_projection(v: &serde_yaml::Value) -> serde_yaml::Value {
        let pick = |k: &str| v.get(k).cloned().unwrap_or(serde_yaml::Value::Null);
        let mut m = serde_yaml::Mapping::new();
        m.insert(ystr("apiKeyEnv"), pick("apiKeyEnv"));
        m.insert(ystr("api"), pick("api"));
        m.insert(ystr("baseURL"), pick("baseURL"));
        m.insert(ystr("models"), pick("models"));
        serde_yaml::Value::Mapping(m)
    }
}

impl ProviderAdapter for DshProviderAdapter {
    fn agent_id(&self) -> &'static str {
        "dsh"
    }

    fn config_path(&self, home: &Path) -> PathBuf {
        Self::settings_path(home)
    }

    /// 下发写两个文件:settings.yaml(route)+ .credentials.yaml(refs),
    /// 快照需覆盖全部。
    fn touch_paths(&self, home: &Path) -> Vec<PathBuf> {
        vec![Self::settings_path(home), Self::credentials_path(home)]
    }

    fn plan(
        &self,
        home: &Path,
        providers: &[ProviderSpec],
        active_id: Option<&str>,
        managed: &[String],
    ) -> Result<Vec<ChangeItem>, String> {
        let mut changes = Vec::new();
        match resolve_single_active(providers, active_id, &DSH_SLOTS, DSH_MISSING) {
            Target::Deploy { spec, url } => {
                if Self::model_ids(spec).is_empty() {
                    changes.push(ChangeItem {
                        name: spec.name.clone(),
                        action: "skip".into(),
                        detail: "No model configured".to_string(),
                    });
                    return Ok(changes);
                }
                let (_, slot) = pick_endpoint(spec, &DSH_SLOTS).unwrap();
                let settings = Self::read_yaml(&Self::settings_path(home))?;
                let creds = Self::read_yaml(&Self::credentials_path(home))?;
                let desired = Self::desired_route(spec, url, slot);
                let route_ok = Self::our_route(&settings)
                    .map(|r| Self::route_projection(r) == Self::route_projection(&desired))
                    .unwrap_or(false);
                let key_ok = Self::our_key(&creds) == spec.api_key.trim()
                    && creds.get("version").and_then(|v| v.as_i64()) == Some(1);
                let action = if route_ok && key_ok {
                    "unchanged"
                } else if Self::our_route(&settings).is_none() && Self::our_key(&creds).is_empty() {
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
                if managed.iter().any(|m| m == DSH_ROUTE) {
                    changes.push(ChangeItem {
                        name: DSH_ROUTE.into(),
                        action: "remove".into(),
                        detail: "no longer managed by ClawBox".into(),
                    });
                } else {
                    changes.push(ChangeItem { name, action: "skip".into(), detail: reason });
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
        match resolve_single_active(providers, active_id, &DSH_SLOTS, DSH_MISSING) {
            Target::Deploy { spec, url } => {
                let models = Self::model_ids(spec);
                if models.is_empty() {
                    return Ok(0); // plan 已 skip,apply 兑底不报错
                }
                let (_, slot) = pick_endpoint(spec, &DSH_SLOTS).unwrap();
                let mut settings = Self::read_yaml(&Self::settings_path(home))?;
                let mut creds = Self::read_yaml(&Self::credentials_path(home))?;
                if !settings.is_mapping() {
                    settings = serde_yaml::Value::Mapping(serde_yaml::Mapping::new());
                }
                if !creds.is_mapping() {
                    creds = serde_yaml::Value::Mapping(serde_yaml::Mapping::new());
                }

                // settings:llm-pi-ai.providers.clawbox = 期望节点(其余键不动)
                {
                    let root = settings.as_mapping_mut().unwrap();
                    let llm = root
                        .entry(ystr("llm-pi-ai"))
                        .or_insert_with(|| serde_yaml::Value::Mapping(serde_yaml::Mapping::new()));
                    if !llm.is_mapping() {
                        return Err("settings.yaml: 'llm-pi-ai' is not a mapping".to_string());
                    }
                    let prov = llm
                        .as_mapping_mut()
                        .unwrap()
                        .entry(ystr("providers"))
                        .or_insert_with(|| serde_yaml::Value::Mapping(serde_yaml::Mapping::new()));
                    if !prov.is_mapping() {
                        return Err("settings.yaml: 'llm-pi-ai.providers' is not a mapping".to_string());
                    }
                    prov.as_mapping_mut()
                        .unwrap()
                        .insert(ystr(DSH_ROUTE), Self::desired_route(spec, url, slot));
                }
                // credentials:refs.CLAWBOX_DSH_API_KEY = key(其余 refs 不动)。
                // dsh 要求顶层 version: 1(credentials-local 的 DOCUMENT_VERSION),
                // 缺失会被判为 pre-release 平铺格式拒读。
                {
                    let root = creds.as_mapping_mut().unwrap();
                    root.insert(ystr("version"), serde_yaml::Value::Number(1.into()));
                    let refs = root
                        .entry(ystr("refs"))
                        .or_insert_with(|| serde_yaml::Value::Mapping(serde_yaml::Mapping::new()));
                    if !refs.is_mapping() {
                        return Err(".credentials.yaml: 'refs' is not a mapping".to_string());
                    }
                    refs.as_mapping_mut()
                        .unwrap()
                        .insert(ystr(DSH_KEY_REF), ystr(spec.api_key.trim()));
                }
                Self::write_yaml(&Self::credentials_path(home), &creds)?; // 先 key 后路由
                Self::write_yaml(&Self::settings_path(home), &settings)?;
                Ok(1)
            }
            Target::Skip { .. } => {
                if !managed.iter().any(|m| m == DSH_ROUTE) {
                    return Ok(0);
                }
                // 解绑:删我们的 route 与 refs 键(文件不存在则免)
                let mut changed = false;
                let sp = Self::settings_path(home);
                if sp.exists() {
                    let mut settings = Self::read_yaml(&sp)?;
                    if let Some(root) = settings.as_mapping_mut() {
                        if let Some(llm) = root.get_mut(&ystr("llm-pi-ai")) {
                            if let Some(prov) = llm.get_mut(&ystr("providers")) {
                                if let Some(m) = prov.as_mapping_mut() {
                                    let rk = ystr(DSH_ROUTE);
                                    changed |= m.remove(&rk).is_some();
                                }
                            }
                        }
                    }
                    if changed {
                        Self::write_yaml(&sp, &settings)?;
                    }
                }
                let cp = Self::credentials_path(home);
                if cp.exists() {
                    let mut creds = Self::read_yaml(&cp)?;
                    if let Some(root) = creds.as_mapping_mut() {
                        if let Some(refs) = root.get_mut(&ystr("refs")) {
                            if let Some(m) = refs.as_mapping_mut() {
                                let rk = ystr(DSH_KEY_REF);
                                changed |= m.remove(&rk).is_some();
                            }
                        }
                    }
                    if changed {
                        Self::write_yaml(&cp, &creds)?;
                    }
                }
                Ok(if changed { 1 } else { 0 })
            }
        }
    }

    fn deployed_names(&self, _providers: &[ProviderSpec], _active_id: Option<&str>) -> Vec<String> {
        // Deploy 与 Skip(remove) 都国绕固定路由键;deployed_names 只在
        // apply 成功后记账,Deploy 时记路由键即可。
        match active_spec(_providers, _active_id) {
            Some(spec) if pick_endpoint(spec, &DSH_SLOTS).is_some() && !Self::model_ids(spec).is_empty() => {
                vec![DSH_ROUTE.to_string()]
            }
            _ => vec![],
        }
    }

    /// 写后校验:settings 可解析、route 四键齐全、credentials 里我们的
    /// key 非空(dsh 把空值当未配置)。
    fn validate(&self, home: &Path) -> Result<(), String> {
        let sp = Self::settings_path(home);
        let settings = Self::read_yaml(&sp)?;
        let route = Self::our_route(&settings).ok_or_else(|| {
            format!("{}: llm-pi-ai.providers.{} missing", sp.display(), DSH_ROUTE)
        })?;
        let base = route.get("baseURL").and_then(|v| v.as_str()).unwrap_or("");
        if base.trim().is_empty() {
            return Err(format!("{}: route '{}' has empty baseURL", sp.display(), DSH_ROUTE));
        }
        let creds = Self::read_yaml(&Self::credentials_path(home))?;
        if Self::our_key(&creds).is_empty() {
            return Err(format!(
                "{}: refs.{} is empty",
                Self::credentials_path(home).display(),
                DSH_KEY_REF
            ));
        }
        Ok(())
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
                // 其余新增 CLI 走自家登录(GitHub OAuth / qwen OAuth),占位。
                Box::new(GeminiProviderAdapter),
                Box::new(ClineProviderAdapter),
                Box::new(PiProviderAdapter),
                Box::new(DshProviderAdapter),
                Box::new(UnsupportedProviderAdapter { id: "qwen-code" }),
                Box::new(UnsupportedProviderAdapter { id: "trae-agent" }),
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
            // 绑「官方默认」哨兵:按空激活出 plan —— managed 有残留则展示
            // remove(防手改配置后悬空托管),否则空条目。
            if bindings.get(a.agent_id()).map(String::as_str)
                == Some(crate::commands::config::DEFAULT_PROVIDER_ID)
            {
                let empty = vec![];
                let m = managed.get(a.agent_id()).unwrap_or(&empty);
                let changes = a.plan(home, &[], None, m).unwrap_or_default();
                return AgentPlan { agent_id, supported: true, config_path, changes, error: None };
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

/// 对单个 agent 应用 fallback 链。与 apply_one 分离:fallback 不走主配置
/// 备份(同一个 config.yaml 已在主 apply 备份过),只汇报写入条数与错误。
/// 把主配置文件回滚到 apply 前的快照。snapshot_id=None 表示 apply 前
/// 没拍上快照(异常情况),只能尽力而为记日志。missing entry 语义下
/// 「文件原本不存在」也能正确还原(删掉 apply 产物)。
fn rollback_config(home: &Path, agent_id: &str, snapshot_id: &Option<String>, target: &Path) {
    let Some(id) = snapshot_id else {
        eprintln!("[clawbox] rollback: no snapshot for {}, cannot restore {}", agent_id, target.display());
        return;
    };
    let rel = target
        .strip_prefix(home)
        .ok()
        .map(|p| p.to_string_lossy().to_string());
    let Some(rel) = rel else {
        eprintln!("[clawbox] rollback: {} escapes home", target.display());
        return;
    };
    if let Err(e) = snapshots::restore_paths(home, agent_id, id, &[rel]) {
        eprintln!(
            "[clawbox] rollback: failed to restore {} from snapshot {}: {}",
            target.display(),
            id,
            e
        );
    }
}

/// 校验刚写入的配置。apply 改了文件(applied>0)才校验;不过则回滚到快照,
/// 返回 Err(带原因)。apply 未改文件(applied==0)直接 Ok——原文件此前应是合法的。
fn validate_or_rollback(
    home: &Path,
    adapter: &dyn ProviderAdapter,
    applied: usize,
    snapshot_id: &Option<String>,
) -> Result<(), (Option<String>, String)> {
    if applied == 0 {
        return Ok(());
    }
    if let Err(ve) = adapter.validate(home) {
        rollback_config(home, adapter.agent_id(), snapshot_id, &adapter.config_path(home));
        return Err((snapshot_id.clone(), format!("validation failed after write, rolled back: {}", ve)));
    }
    Ok(())
}

/// 对单个 agent 应用 fallback 链。与 apply_one 同样走 快照→写→校验→(不过)回滚。
pub fn apply_fallbacks_one(
    home: &Path,
    adapter: &dyn ProviderAdapter,
    fallbacks: &[ProviderSpec],
    managed: &[String],
) -> ApplyResult {
    let agent_id = adapter.agent_id().to_string();
    if !adapter.supported() {
        return ApplyResult {
            agent_id,
            ok: false,
            snapshot_id: None,
            applied: 0,
            error: Some("agent not supported for provider sync".to_string()),
        };
    }
    let snapshot_id = match snapshots::capture(home, adapter.agent_id(), "fallback", "fallback sync", &adapter.touch_paths(home)) {
        Ok(s) => Some(s.id),
        Err(e) => {
            return ApplyResult {
                agent_id,
                ok: false,
                snapshot_id: None,
                applied: 0,
                error: Some(e),
            }
        }
    };
    let applied = match adapter.apply_fallbacks(home, fallbacks, managed) {
        Ok(n) => n,
        Err(e) => {
            return ApplyResult {
                agent_id,
                ok: false,
                snapshot_id,
                applied: 0,
                error: Some(e),
            }
        }
    };
    match validate_or_rollback(home, adapter, applied, &snapshot_id) {
        Ok(()) => ApplyResult {
            agent_id,
            ok: true,
            snapshot_id,
            applied,
            error: None,
        },
        Err((bk, msg)) => ApplyResult {
            agent_id,
            ok: false,
            snapshot_id: bk,
            applied: 0,
            error: Some(msg),
        },
    }
}

/// 对单个 agent 应用:快照、写入、校验、(不过)回滚。调用方在
/// 成功后更新 providers_managed。
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
            snapshot_id: None,
            applied: 0,
            error: Some("agent not supported for provider sync".to_string()),
        };
    }
    let snapshot_id = match snapshots::capture(home, adapter.agent_id(), "provider", "provider sync", &adapter.touch_paths(home)) {
        Ok(s) => Some(s.id),
        Err(e) => {
            return ApplyResult {
                agent_id,
                ok: false,
                snapshot_id: None,
                applied: 0,
                error: Some(e),
            }
        }
    };
    let applied = match adapter.apply(home, providers, active_id, managed) {
        Ok(n) => n,
        Err(e) => {
            return ApplyResult {
                agent_id,
                ok: false,
                snapshot_id,
                applied: 0,
                error: Some(e),
            }
        }
    };
    match validate_or_rollback(home, adapter, applied, &snapshot_id) {
        Ok(()) => ApplyResult {
            agent_id,
            ok: true,
            snapshot_id,
            applied,
            error: None,
        },
        Err((bk, msg)) => ApplyResult {
            agent_id,
            ok: false,
            snapshot_id: bk,
            applied: 0,
            error: Some(msg),
        },
    }
}

#[cfg(test)]
mod tests {
    use crate::usage::pricing::ProviderPricing;

    #[test]
    fn plan_all_default_binding_shows_remove_only_with_leftover_managed() {
        let home = TempHome::new();
        let bindings =
            std::collections::HashMap::from([("dsh".to_string(), "__default__".to_string())]);
        // 无残留:空条目
        let plans = plan_all(home.path(), &[], &bindings, &Default::default());
        let dsh = plans.iter().find(|p| p.agent_id == "dsh").unwrap();
        // 只允许 skip(未下发),不得出现 add/update/remove
        assert!(dsh.changes.iter().all(|c| c.action == "skip"), "{:?}", dsh.changes);
        // 有残留(手改配置悬空):出 remove 条目
        let managed =
            std::collections::HashMap::from([("dsh".to_string(), vec!["clawbox".to_string()])]);
        let plans = plan_all(home.path(), &[], &bindings, &managed);
        let dsh = plans.iter().find(|p| p.agent_id == "dsh").unwrap();
        assert_eq!(dsh.changes.len(), 1);
        assert_eq!(dsh.changes[0].action, "remove");
    }

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
            model_aliases: BTreeMap::new(),
            pricing: ProviderPricing::default(),
            enabled: true,
            flavor: None,
        }
    }

    // ---- gemini(~/.gemini/.env 三键) ----------------------------------

    #[test]
    fn gemini_plan_add_then_apply_writes_env_and_unchanged() {
        let home = TempHome::new();
        let providers = vec![provider("gw", "Gateway", "https://gw.example.com/", "")];
        let a = GeminiProviderAdapter;

        let changes = a.plan(home.path(), &providers, Some("gw"), &[]).unwrap();
        assert_eq!(changes[0].action, "add");

        assert_eq!(a.apply(home.path(), &providers, Some("gw"), &[]).unwrap(), 1);
        let env = std::fs::read_to_string(home.path().join(".gemini").join(".env")).unwrap();
        assert!(env.contains("GOOGLE_GEMINI_BASE_URL=https://gw.example.com/"), "{}", env);
        assert!(env.contains("GEMINI_API_KEY=sk-secret-123"), "{}", env);
        assert!(env.contains("GEMINI_MODEL=model-a"), "{}", env);

        let changes = a.plan(home.path(), &providers, Some("gw"), &["env".into()]).unwrap();
        assert_eq!(changes[0].action, "unchanged");
        assert_eq!(a.deployed_names(&providers, Some("gw")), vec!["env".to_string()]);
    }

    #[test]
    fn gemini_skips_provider_without_anthropic_slot() {
        let home = TempHome::new();
        let providers = vec![provider("oa", "OpenAI-only", "", "https://x.example.com/v1")];
        let a = GeminiProviderAdapter;
        let changes = a.plan(home.path(), &providers, Some("oa"), &[]).unwrap();
        assert_eq!(changes[0].action, "skip");
        assert_eq!(a.apply(home.path(), &providers, Some("oa"), &[]).unwrap(), 0);
        assert!(a.deployed_names(&providers, Some("oa")).is_empty());
    }

    #[test]
    fn gemini_unbind_cleanup_removes_only_managed_lines() {
        let home = TempHome::new();
        let providers = vec![provider("gw", "Gateway", "https://gw.example.com/", "")];
        let a = GeminiProviderAdapter;
        // 用户自有行必须原样保留
        std::fs::create_dir_all(home.path().join(".gemini")).unwrap();
        std::fs::write(home.path().join(".gemini").join(".env"), "OTHER=keep\n").unwrap();
        a.apply(home.path(), &providers, Some("gw"), &[]).unwrap();

        // 解绑(无 active):曾管理 → 清三行;OTHER 行不动
        assert_eq!(a.apply(home.path(), &providers, None, &["env".into()]).unwrap(), 1);
        let env = std::fs::read_to_string(home.path().join(".gemini").join(".env")).unwrap();
        assert_eq!(env, "OTHER=keep\n");
    }

    // ---- cline(providers.json 经 cline auth,纯函数/文件投影) ----------

    #[test]
    fn cline_auth_args_include_base_url_and_optional_model() {
        let spec = provider("gw", "Gateway", "https://gw.example.com/", "");
        let args = ClineProviderAdapter::auth_args(&spec, "https://gw.example.com/");
        assert_eq!(
            args,
            vec!["auth", "-p", "anthropic", "-k", "sk-secret-123", "-b", "https://gw.example.com/", "-m", "model-a"]
        );
        let mut no_model = spec.clone();
        no_model.default_model = String::new();
        let args = ClineProviderAdapter::auth_args(&no_model, "https://gw.example.com/");
        assert!(!args.contains(&"-m".to_string()));
    }

    #[test]
    fn cline_plan_reads_current_projection_and_skips_openai_only() {
        let home = TempHome::new();
        let a = ClineProviderAdapter;
        // 已是期望状态 → unchanged(不会去跑 CLI)
        let dir = home.path().join(".cline").join("data").join("settings");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("providers.json"),
            r#"{"version":1,"providers":{"anthropic":{"settings":{"provider":"anthropic","apiKey":"sk-secret-123","model":"model-a","baseUrl":"https://gw.example.com/"}}}}"#,
        )
        .unwrap();
        let providers = vec![provider("gw", "Gateway", "https://gw.example.com/", "")];
        let changes = a.plan(home.path(), &providers, Some("gw"), &[]).unwrap();
        assert_eq!(changes[0].action, "unchanged");
        assert_eq!(a.apply(home.path(), &providers, Some("gw"), &[]).unwrap(), 0);

        // OpenAI-only 服务商 → skip
        let oa = vec![provider("oa", "OpenAI-only", "", "https://x.example.com/v1")];
        let changes = a.plan(home.path(), &oa, Some("oa"), &[]).unwrap();
        assert_eq!(changes[0].action, "skip");
    }

    // ---- pi(models.json 节点 + settings.json 两键) ---------------------

    #[test]
    fn pi_apply_writes_node_and_settings_then_unchanged() {
        let home = TempHome::new();
        let providers = vec![provider("gw", "Gateway", "https://gw.example.com/", "")];
        let a = PiProviderAdapter;

        let changes = a.plan(home.path(), &providers, Some("gw"), &[]).unwrap();
        assert_eq!(changes[0].action, "add");
        assert!(a.apply(home.path(), &providers, Some("gw"), &[]).unwrap() >= 1);

        let models: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(home.path().join(".pi").join("agent").join("models.json")).unwrap(),
        )
        .unwrap();
        let node = &models["providers"]["gw"];
        assert_eq!(node["api"], "anthropic-messages");
        assert_eq!(node["baseUrl"], "https://gw.example.com/");
        assert_eq!(node["models"].as_array().unwrap().len(), 2);
        let settings: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(home.path().join(".pi").join("agent").join("settings.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(settings["defaultProvider"], "gw");
        assert_eq!(settings["defaultModel"], "model-a");

        let changes = a.plan(home.path(), &providers, Some("gw"), &["gw".into()]).unwrap();
        assert_eq!(changes[0].action, "unchanged");
        assert_eq!(a.deployed_names(&providers, Some("gw")), vec!["gw".to_string()]);
    }

    #[test]
    fn pi_openai_only_provider_uses_openai_completions() {
        let home = TempHome::new();
        let providers = vec![provider("oa", "OpenAI-only", "", "https://x.example.com/v1")];
        let a = PiProviderAdapter;
        a.apply(home.path(), &providers, Some("oa"), &[]).unwrap();
        let models: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(home.path().join(".pi").join("agent").join("models.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(models["providers"]["oa"]["api"], "openai-completions");
    }

    #[test]
    fn pi_unbind_removes_managed_node_keeps_others_and_settings() {
        let home = TempHome::new();
        let providers = vec![provider("gw", "Gateway", "https://gw.example.com/", "")];
        let a = PiProviderAdapter;
        // 预置用户自有节点
        let dir = home.path().join(".pi").join("agent");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("models.json"),
            r#"{"providers":{"mine":{"baseUrl":"http://localhost:11434/v1","api":"openai-completions","apiKey":"x","models":[]}}}"#,
        )
        .unwrap();
        a.apply(home.path(), &providers, Some("gw"), &[]).unwrap();

        // 解绑:删我们的 gw 节点;mine 与 settings 两键保留
        assert_eq!(a.apply(home.path(), &providers, None, &["gw".into()]).unwrap(), 1);
        let models: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join("models.json")).unwrap()).unwrap();
        assert!(models["providers"].get("gw").is_none());
        assert!(models["providers"].get("mine").is_some());
        let settings: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join("settings.json")).unwrap()).unwrap();
        assert_eq!(settings["defaultProvider"], "gw"); // 保留历史值
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
        let abs_catalog = home.path().join(codex_catalog_rel());
        assert!(
            text.contains(&format!("model_catalog_json = \"{}\"", abs_catalog.display())),
            "must reference the catalog by absolute path: {}",
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
    // hermes 下发走直接 YAML 读改写(无 CLI 依赖),plan/apply 全程 home
    // 参数化 + TempHome,可安全真跑 apply 并读回断言。env_line_value /
    // merge_env_line 是 gemini 仍用的模块级工具,这里一并覆盖。

    fn read_hermes_yaml(home: &Path) -> serde_yaml::Value {
        let p = home.join(".hermes").join("config.yaml");
        serde_yaml::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap()
    }

    #[test]
    fn hermes_merge_env_line_replaces_in_place_and_appends() {
        let key = "CUSTOM_PROVIDER_P_ANTH_KEY";
        // 已存在 → 整行替换,其余行一字不动
        let content = "A=1\nCUSTOM_PROVIDER_P_ANTH_KEY=old\nB=2 # keep\n";
        let merged = merge_env_line(content, key, "new-key");
        assert_eq!(merged, "A=1\nCUSTOM_PROVIDER_P_ANTH_KEY=new-key\nB=2 # keep\n");
        // 不存在 → 追加(含无尾换行的输入)
        let merged = merge_env_line("A=1", key, "v");
        assert_eq!(merged, "A=1\nCUSTOM_PROVIDER_P_ANTH_KEY=v\n");
        // 空文件 → 单行
        let merged = merge_env_line("", key, "v");
        assert_eq!(merged, "CUSTOM_PROVIDER_P_ANTH_KEY=v\n");
        // 前缀相同的其它键(…_KEY_BACKUP)不受影响
        let content = "CUSTOM_PROVIDER_P_ANTH_KEY_BACKUP=x\n";
        let merged = merge_env_line(content, key, "v");
        assert_eq!(merged, "CUSTOM_PROVIDER_P_ANTH_KEY_BACKUP=x\nCUSTOM_PROVIDER_P_ANTH_KEY=v\n");
    }

    #[test]
    fn hermes_env_line_value_parses_plain_and_quoted() {
        assert_eq!(env_line_value("K=abc\nX=1\n", "K").as_deref(), Some("abc"));
        assert_eq!(env_line_value("K=\"abc\"\n", "K").as_deref(), Some("abc"));
        assert!(env_line_value("K2=abc\n", "K").is_none());
        // 前缀相同的长键不误配
        assert!(env_line_value("K_LONG=abc\n", "K").is_none());
    }

    #[test]
    fn hermes_api_mode_provider_ref_and_entry_name() {
        assert_eq!(HermesProviderAdapter::api_mode(Slot::Anthropic), "anthropic_messages");
        assert_eq!(HermesProviderAdapter::api_mode(Slot::Openai), "chat_completions");
        let spec = anthropic_provider();
        assert_eq!(HermesProviderAdapter::entry_name(&spec), "Anthro Relay");
        assert_eq!(HermesProviderAdapter::provider_ref(&spec), "custom:Anthro Relay");
    }

    #[test]
    fn hermes_render_and_project_entry_roundtrip() {
        let spec = anthropic_provider();
        let url = "https://relay.example.com/anthropic";
        let entry = HermesProviderAdapter::render_entry(&spec, url, Slot::Anthropic);
        // 必备字段齐
        assert_eq!(entry.get("name").and_then(|v| v.as_str()), Some("Anthro Relay"));
        assert_eq!(entry.get("base_url").and_then(|v| v.as_str()), Some(url));
        assert_eq!(entry.get("api_mode").and_then(|v| v.as_str()), Some("anthropic_messages"));
        assert_eq!(entry.get("model").and_then(|v| v.as_str()), Some("model-a"));
        assert!(entry.get("models").unwrap().get("model-a").is_some());
        assert!(entry.get("models").unwrap().get("model-b").is_some());
        // project(读回)与 desired(目标)等价 → 落盘后不会反复重写
        let proj = HermesProviderAdapter::project_entry(&entry).unwrap();
        let desired = HermesProviderAdapter::desired_entry(&spec, url, Slot::Anthropic);
        assert_eq!(proj, desired);
    }

    #[test]
    fn hermes_plan_add_then_apply_then_unchanged_and_no_key_leak() {
        let home = TempHome::new();
        let providers = vec![anthropic_provider()];
        let a = HermesProviderAdapter;

        // 文件不存在 → add;detail 绝不含 apiKey
        let changes = a.plan(home.path(), &providers, Some("p-anth"), &[]).unwrap();
        assert_eq!(changes[0].action, "add");
        assert!(!changes[0].detail.contains("sk-secret"), "detail must not leak apiKey");

        // apply → 写 custom_providers 命名条目 + model 节
        assert_eq!(a.apply(home.path(), &providers, Some("p-anth"), &[]).unwrap(), 1);
        let doc = read_hermes_yaml(home.path());
        assert_eq!(
            doc.get("model").and_then(|m| m.get("provider")).and_then(|v| v.as_str()),
            Some("custom:Anthro Relay")
        );
        assert_eq!(
            doc.get("model").and_then(|m| m.get("base_url")).and_then(|v| v.as_str()),
            Some("https://relay.example.com/anthropic")
        );
        assert_eq!(
            doc.get("model").and_then(|m| m.get("default")).and_then(|v| v.as_str()),
            Some("model-a")
        );
        let seq = doc.get("custom_providers").and_then(|v| v.as_sequence()).unwrap();
        assert_eq!(seq.len(), 1);
        assert_eq!(seq[0].get("name").and_then(|v| v.as_str()), Some("Anthro Relay"));
        assert_eq!(seq[0].get("api_mode").and_then(|v| v.as_str()), Some("anthropic_messages"));

        // 再 plan → unchanged;再 apply → 短路 0(不重写)
        let changes = a.plan(home.path(), &providers, Some("p-anth"), &[]).unwrap();
        assert_eq!(changes[0].action, "unchanged");
        assert_eq!(a.apply(home.path(), &providers, Some("p-anth"), &[]).unwrap(), 0);

        // 端点变更 → update
        let mut moved = anthropic_provider();
        moved.anthropic_base_url = "https://relay2.example.com/anthropic".into();
        let changes = a.plan(home.path(), &[moved], Some("p-anth"), &[]).unwrap();
        assert_eq!(changes[0].action, "update");
    }

    #[test]
    fn hermes_apply_preserves_user_entries_top_keys_and_drops_managed() {
        let home = TempHome::new();
        let a = HermesProviderAdapter;
        // 既有:用户手建条目 + 顶层 agent 节 + 我们曾以 "old-name" 下发(managed)
        write_file(
            home.path(),
            &PathBuf::from(".hermes").join("config.yaml"),
            "agent:\n  max_turns: 90\ncustom_providers:\n  - name: user-own\n    base_url: https://u.example.com/v1\n    api_key: user-key\n    api_mode: chat_completions\n  - name: old-name\n    base_url: https://old.example.com/anthropic\n    api_key: old-key\n    api_mode: anthropic_messages\nmodel:\n  provider: custom:old-name\n  default: old-model\n",
        );
        let providers = vec![anthropic_provider()];
        let managed = vec!["old-name".to_string()];

        assert_eq!(a.apply(home.path(), &providers, Some("p-anth"), &managed).unwrap(), 1);
        let doc = read_hermes_yaml(home.path());

        // 顶层 agent 节原样保留
        assert_eq!(
            doc.get("agent").and_then(|a| a.get("max_turns")).and_then(|v| v.as_i64()),
            Some(90)
        );
        // 用户手建条目保留;old-name(managed 差集)被丢弃;新条目已写
        let seq = doc.get("custom_providers").and_then(|v| v.as_sequence()).unwrap();
        let names: Vec<&str> = seq
            .iter()
            .filter_map(|e| e.get("name").and_then(|v| v.as_str()))
            .collect();
        assert!(names.contains(&"user-own"), "user entry must be preserved");
        assert!(!names.contains(&"old-name"), "stale managed entry must be dropped");
        assert!(names.contains(&"Anthro Relay"), "new entry must be written");
        // model.provider 指向新条目
        assert_eq!(
            doc.get("model").and_then(|m| m.get("provider")).and_then(|v| v.as_str()),
            Some("custom:Anthro Relay")
        );
    }

    #[test]
    fn hermes_slot_fallback_skip_and_deployed_names() {
        let home = TempHome::new();
        let a = HermesProviderAdapter;
        // 只有 OpenAI 端点 → 可下发;api_mode 由 OpenAI 槽决定(chat_completions)
        let providers = vec![openai_provider()];
        let changes = a.plan(home.path(), &providers, Some("p-oa"), &[]).unwrap();
        assert_eq!(changes[0].action, "add");
        assert!(changes[0].detail.contains("https://api.oa.example.com/v1"), "{}", changes[0].detail);
        assert_eq!(a.deployed_names(&providers, Some("p-oa")), vec!["OA Relay"]);
        a.apply(home.path(), &providers, Some("p-oa"), &[]).unwrap();
        let doc = read_hermes_yaml(home.path());
        let seq = doc.get("custom_providers").and_then(|v| v.as_sequence()).unwrap();
        assert_eq!(seq[0].get("api_mode").and_then(|v| v.as_str()), Some("chat_completions"));

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
        // 未选择激活 → skip,apply 不做任何事(不写文件)
        let changes = a.plan(home.path(), &providers, None, &[]).unwrap();
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].action, "skip");
        assert!(a.deployed_names(&providers, None).is_empty());
        let fresh = TempHome::new();
        assert_eq!(a.apply(fresh.path(), &providers, None, &[]).unwrap(), 0);
        assert!(!fresh.path().join(".hermes").exists());
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

    // ---- hermes fallback 链 ----

    #[test]
    fn hermes_fallback_deployable_requires_endpoint_key_model() {
        let a = HermesProviderAdapter;
        assert!(a.supports_fallback());
        assert!(a.fallback_deployable(&anthropic_provider()));
        // 无端点 → 不可
        let no_ep = provider("p-x", "X", "", "");
        assert!(!a.fallback_deployable(&no_ep));
        // 无 key → 不可
        let mut no_key = anthropic_provider();
        no_key.api_key = String::new();
        assert!(!a.fallback_deployable(&no_key));
        // 无 default model → 不可(hermes 缺 model 字段会禁用该 fallback)
        let mut no_model = anthropic_provider();
        no_model.default_model = String::new();
        assert!(!a.fallback_deployable(&no_model));
    }

    #[test]
    fn hermes_fallback_apply_writes_chain_and_entries_and_keeps_primary() {
        let home = TempHome::new();
        let a = HermesProviderAdapter;
        // primary 先落盘
        a.apply(home.path(), &[anthropic_provider()], Some("p-anth"), &[]).unwrap();
        // 加一个 fallback(openai-only 提供商 → chat_completions)
        let fb = vec![openai_provider()];
        assert_eq!(a.apply_fallbacks(home.path(), &fb, &[]).unwrap(), 1);

        let doc = read_hermes_yaml(home.path());
        // custom_providers 同时有 primary 与 fallback 条目
        let names: Vec<&str> = doc
            .get("custom_providers").and_then(|v| v.as_sequence()).unwrap()
            .iter().filter_map(|e| e.get("name").and_then(|v| v.as_str())).collect();
        assert!(names.contains(&"Anthro Relay"), "primary entry present");
        assert!(names.contains(&"OA Relay"), "fallback entry present");
        // fallback_providers 链一条,引用 custom:OA Relay
        let fbs = doc.get("fallback_providers").and_then(|v| v.as_sequence()).unwrap();
        assert_eq!(fbs.len(), 1);
        assert_eq!(fbs[0].get("provider").and_then(|v| v.as_str()), Some("custom:OA Relay"));
        assert_eq!(fbs[0].get("model").and_then(|v| v.as_str()), Some("model-a"));
        assert_eq!(fbs[0].get("api_mode").and_then(|v| v.as_str()), Some("chat_completions"));
        // primary 的 model.provider 未被 fallback apply 碰
        assert_eq!(
            doc.get("model").and_then(|m| m.get("provider")).and_then(|v| v.as_str()),
            Some("custom:Anthro Relay")
        );
        assert_eq!(a.deployed_fallback_names(&fb), vec!["OA Relay"]);
    }

    #[test]
    fn hermes_fallback_rebind_and_clear_cleanup_and_preserve_primary() {
        let home = TempHome::new();
        let a = HermesProviderAdapter;
        a.apply(home.path(), &[anthropic_provider()], Some("p-anth"), &[]).unwrap();
        let oa = openai_provider();
        a.apply_fallbacks(home.path(), &[oa.clone()], &[]).unwrap();
        let managed = a.deployed_fallback_names(&[oa.clone()]);
        assert_eq!(managed, vec!["OA Relay"]);

        // 改链 = [Dual Relay](替换 OA);managed=[OA Relay] → OA 条目应被清掉
        let dual = dual_provider();
        a.apply_fallbacks(home.path(), &[dual.clone()], &managed).unwrap();
        let doc = read_hermes_yaml(home.path());
        let names: Vec<&str> = doc
            .get("custom_providers").and_then(|v| v.as_sequence()).unwrap()
            .iter().filter_map(|e| e.get("name").and_then(|v| v.as_str())).collect();
        assert!(!names.contains(&"OA Relay"), "stale fallback entry must be removed");
        assert!(names.contains(&"Dual Relay"));
        assert!(names.contains(&"Anthro Relay"), "primary entry preserved across rebind");
        let fbs = doc.get("fallback_providers").and_then(|v| v.as_sequence()).unwrap();
        assert_eq!(fbs[0].get("provider").and_then(|v| v.as_str()), Some("custom:Dual Relay"));

        // 清空链:managed=[Dual Relay] → fallback_providers 键删除,Dual Relay 条目清掉
        a.apply_fallbacks(home.path(), &[], &["Dual Relay".to_string()]).unwrap();
        let doc = read_hermes_yaml(home.path());
        assert!(doc.get("fallback_providers").is_none(), "fallback_providers key removed on clear");
        let names2: Vec<&str> = doc
            .get("custom_providers").and_then(|v| v.as_sequence()).unwrap()
            .iter().filter_map(|e| e.get("name").and_then(|v| v.as_str())).collect();
        assert!(!names2.contains(&"Dual Relay"), "cleared fallback entry removed");
        assert!(names2.contains(&"Anthro Relay"), "primary intact after clear");
    }

    #[test]
    fn hermes_fallback_chain_order_preserved_and_reorderable() {
        let home = TempHome::new();
        let a = HermesProviderAdapter;
        a.apply(home.path(), &[anthropic_provider()], Some("p-anth"), &[]).unwrap();
        // 两家 fallback,按 [OA, Dual] 顺序下发
        a.apply_fallbacks(home.path(), &[openai_provider(), dual_provider()], &[]).unwrap();
        let doc = read_hermes_yaml(home.path());
        let fbs = doc.get("fallback_providers").and_then(|v| v.as_sequence()).unwrap();
        assert_eq!(fbs.len(), 2);
        assert_eq!(fbs[0].get("provider").and_then(|v| v.as_str()), Some("custom:OA Relay"));
        assert_eq!(fbs[1].get("provider").and_then(|v| v.as_str()), Some("custom:Dual Relay"));

        // 拖拽换序 → [Dual, OA]:backend 仍按入参顺序写
        a.apply_fallbacks(
            home.path(),
            &[dual_provider(), openai_provider()],
            &["OA Relay".into(), "Dual Relay".into()],
        )
        .unwrap();
        let doc = read_hermes_yaml(home.path());
        let fbs = doc.get("fallback_providers").and_then(|v| v.as_sequence()).unwrap();
        assert_eq!(fbs[0].get("provider").and_then(|v| v.as_str()), Some("custom:Dual Relay"));
        assert_eq!(fbs[1].get("provider").and_then(|v| v.as_str()), Some("custom:OA Relay"));
        // custom_providers 三条都在(primary + 两个 fallback)
        let names: Vec<&str> = doc
            .get("custom_providers").and_then(|v| v.as_sequence()).unwrap()
            .iter().filter_map(|e| e.get("name").and_then(|v| v.as_str())).collect();
        assert!(names.contains(&"Anthro Relay") && names.contains(&"OA Relay") && names.contains(&"Dual Relay"));
    }

    #[test]
    fn hermes_fallback_plan_drift_and_unchanged() {
        let home = TempHome::new();
        let a = HermesProviderAdapter;
        let oa = openai_provider();
        let dual = dual_provider();
        // 空 + 无 managed → 不出条目
        assert!(a.plan_fallbacks(home.path(), &[], &[]).unwrap().is_empty());
        // add
        assert_eq!(a.plan_fallbacks(home.path(), &[oa.clone()], &[]).unwrap()[0].action, "add");        // apply → plan 无漂移(unchanged 返回空)
        a.apply_fallbacks(home.path(), &[oa.clone()], &[]).unwrap();
        let managed = a.deployed_fallback_names(&[oa.clone()]);
        assert!(a.plan_fallbacks(home.path(), &[oa.clone()], &managed).unwrap().is_empty(), "unchanged");
        // 换链 → update
        assert_eq!(a.plan_fallbacks(home.path(), &[dual.clone()], &managed).unwrap()[0].action, "update");
        // 清空 → remove
        assert_eq!(a.plan_fallbacks(home.path(), &[], &managed).unwrap()[0].action, "remove");
    }

    #[test]
    fn hermes_validate_rejects_dangling_and_accepts_resolving() {
        let home = TempHome::new();
        let a = HermesProviderAdapter;
        let cfg = PathBuf::from(".hermes").join("config.yaml");
        // model.provider 指向不存在的 custom 条目 → 拒(正是当年 UUID bug 的形态)
        write_file(home.path(), &cfg, "model:\n  provider: custom:Ghost\n  default: m\n");
        let err = a.validate(home.path()).unwrap_err();
        assert!(err.contains("no matching custom_providers"), "{}", err);
        // 加上匹配条目(base_url+api_key 齐)→ 过
        write_file(
            home.path(),
            &cfg,
            "model:\n  provider: custom:MiniMax\n  default: MiniMax-M3\ncustom_providers:\n  - name: MiniMax\n    base_url: https://x/anthropic\n    api_key: k\n",
        );
        a.validate(home.path()).unwrap();
        // 条目缺 api_key → 拒
        write_file(
            home.path(),
            &cfg,
            "model:\n  provider: custom:MiniMax\ncustom_providers:\n  - name: MiniMax\n    base_url: https://x/anthropic\n",
        );
        let err = a.validate(home.path()).unwrap_err();
        assert!(err.contains("api_key"), "{}", err);
        // model.provider 为空 → 拒
        write_file(home.path(), &cfg, "model:\n  default: m\n");
        let err = a.validate(home.path()).unwrap_err();
        assert!(err.contains("empty"), "{}", err);
        // fallback 链里引用缺失条目 → 拒,且提示 fallback_providers[0]
        write_file(
            home.path(),
            &cfg,
            "model:\n  provider: custom:MiniMax\ncustom_providers:\n  - name: MiniMax\n    base_url: https://x\n    api_key: k\nfallback_providers:\n  - provider: custom:Missing\n    model: m\n",
        );
        let err = a.validate(home.path()).unwrap_err();
        assert!(err.contains("fallback_providers[0]"), "{}", err);
    }

    #[test]
    fn hermes_extract_active_reads_resolving_entry() {
        let home = TempHome::new();
        let a = HermesProviderAdapter;
        let cfg = PathBuf::from(".hermes").join("config.yaml");
        write_file(
            home.path(),
            &cfg,
            "model:\n  provider: custom:MiniMax\n  default: MiniMax-M3\ncustom_providers:\n  - name: MiniMax\n    base_url: https://api.minimaxi.com/anthropic\n    api_key: sk-x\n    api_mode: anthropic_messages\n    model: MiniMax-M3\n    models:\n      MiniMax-M3: {name: MiniMax M3}\n      MiniMax-M2: {name: MiniMax M2}\n",
        );
        let adopted = a.extract_active(home.path()).unwrap().unwrap();
        assert_eq!(adopted.name, "MiniMax");
        assert_eq!(adopted.base_url, "https://api.minimaxi.com/anthropic");
        assert_eq!(adopted.api_key, "sk-x");
        assert_eq!(adopted.slot, Slot::Anthropic);
        assert_eq!(adopted.default_model, "MiniMax-M3");
        assert!(adopted.models.contains(&"MiniMax-M3".to_string()));
        assert!(adopted.models.contains(&"MiniMax-M2".to_string()));
        // model.provider 为空 → None
        write_file(home.path(), &cfg, "model:\n  default: m\n");
        assert!(a.extract_active(home.path()).unwrap().is_none());
        // 指向不存在的 custom → None
        write_file(home.path(), &cfg, "model:\n  provider: custom:Ghost\ncustom_providers: []\n");
        assert!(a.extract_active(home.path()).unwrap().is_none());
    }

    // ---- 写后校验 + 回滚机制(mock adapter)----

    /// apply 会写文件,但 validate 恒拒 —— 用来验证 apply_one 的回滚路径。
    struct AlwaysInvalidAdapter;
    impl ProviderAdapter for AlwaysInvalidAdapter {
        fn agent_id(&self) -> &'static str {
            "mock-invalid"
        }
        fn config_path(&self, home: &Path) -> PathBuf {
            home.join(".mock").join("cfg.json")
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
            home: &Path,
            _providers: &[ProviderSpec],
            _active_id: Option<&str>,
            _managed: &[String],
        ) -> Result<usize, String> {
            let p = home.join(".mock").join("cfg.json");
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(&p, "{\"applied\":true}").unwrap();
            Ok(1)
        }
        fn deployed_names(&self, _providers: &[ProviderSpec], _active_id: Option<&str>) -> Vec<String> {
            vec![]
        }
        fn validate(&self, _home: &Path) -> Result<(), String> {
            Err("mock: always invalid".into())
        }
    }

    #[test]
    fn apply_one_rolls_back_to_prior_file_on_validation_failure() {
        let home = TempHome::new();
        let cfg = PathBuf::from(".mock").join("cfg.json");
        // 先有原始文件 → rollback 应回复它,而非留 apply 产物
        write_file(home.path(), &cfg, "{\"original\":true}");
        let r = apply_one(home.path(), &AlwaysInvalidAdapter, &[], None, &[]);
        assert!(!r.ok, "must fail");
        assert!(
            r.error.as_deref().unwrap_or("").contains("rolled back"),
            "{:?}",
            r.error
        );
        let content = std::fs::read_to_string(home.path().join(&cfg)).unwrap();
        assert!(content.contains("original"), "rollback must restore original; got: {}", content);
    }

    #[test]
    fn apply_one_removes_file_when_no_prior_existed_and_validation_fails() {
        let home = TempHome::new();
        let r = apply_one(home.path(), &AlwaysInvalidAdapter, &[], None, &[]);
        assert!(!r.ok);
        assert!(
            !home.path().join(".mock").join("cfg.json").exists(),
            "rollback must remove the file that didn't exist before apply"
        );
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
    fn kimi_prefers_new_kimi_code_root_when_present() {
        let home = TempHome::new();
        let a = KimiProviderAdapter;
        // 无 ~/.kimi-code → 旧路径
        assert_eq!(a.config_path(home.path()), home.path().join(kimi_rel()));
        // 有 ~/.kimi-code → 新路径(即便旧目录也在)
        std::fs::create_dir_all(home.path().join(".kimi")).unwrap();
        std::fs::create_dir_all(home.path().join(".kimi-code")).unwrap();
        assert_eq!(
            a.config_path(home.path()),
            home.path().join(".kimi-code").join("config.toml")
        );
    }

    #[test]
    fn kimi_new_root_uses_openai_type_and_max_context_size() {
        let home = TempHome::new();
        std::fs::create_dir_all(home.path().join(".kimi-code")).unwrap();
        let providers = vec![provider("gw", "Gateway", "", "https://x.example.com/v1")];
        KimiProviderAdapter.apply(home.path(), &providers, Some("gw"), &[]).unwrap();
        let text =
            std::fs::read_to_string(home.path().join(".kimi-code").join("config.toml")).unwrap();
        // Kimi Code 0.31 schema:openai_legacy 已废弃,models 条目必须带 max_context_size
        assert!(text.contains("type = \"openai\""), "{}", text);
        assert!(!text.contains("openai_legacy"), "{}", text);
        assert!(text.contains("max_context_size = 128000"), "{}", text);
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
        assert_eq!(adapters().len(), 15);
        let supported: Vec<&str> = adapters()
            .iter()
            .filter(|a| a.supported())
            .map(|a| a.agent_id())
            .collect();
        assert_eq!(
            supported,
            vec!["claude-code", "codex", "openclaw", "opencode", "codebuddy", "kimi", "hermes", "gemini", "cline", "pi", "dsh"]
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
        assert_eq!(plans.len(), 15);
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
    fn apply_one_takes_snapshot() {
        let home = TempHome::new();
        write_file(
            home.path(),
            &PathBuf::from(".claude").join("settings.json"),
            r#"{"env": {}}"#,
        );
        let providers = vec![anthropic_provider()];
        let r = apply_one(home.path(), &claude_code(), &providers, Some("p-anth"), &[]);
        assert!(r.ok, "{:?}", r.error);
        let snaps = snapshots::list(home.path(), Some("claude-code"));
        assert_eq!(snaps.len(), 1);
        assert_eq!(snaps[0].scope, "provider");
        assert!(snaps[0].restorable);
    }

    #[test]
    fn codex_provider_snapshot_covers_all_three_files() {
        let home = TempHome::new();
        let providers = vec![openai_provider()];
        let r = apply_one(home.path(), &CodexProviderAdapter, &providers, Some("p-oa"), &[]);
        assert!(r.ok, "{:?}", r.error);
        let id = r.snapshot_id.expect("snapshot");
        let dir = home.path().join(".clawbox").join("snapshots").join("codex").join(&id);
        let m: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(dir.join("manifest.json")).unwrap(),
        )
        .unwrap();
        let rels: Vec<&str> = m["entries"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e["rel_path"].as_str().unwrap())
            .collect();
        assert!(rels.contains(&".codex/config.toml"), "{:?}", rels);
        assert!(rels.contains(&".codex/auth.json"), "{:?}", rels);
        assert!(rels.contains(&".codex/clawbox-model-catalog.json"), "{:?}", rels);
    }
    // ---- dsh(~/.dsh/settings.yaml + .credentials.yaml) -----------------

    fn read_yaml_file(path: &std::path::Path) -> serde_yaml::Value {
        serde_yaml::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
    }

    #[test]
    fn dsh_apply_writes_route_and_credentials_then_plans_unchanged() {
        let home = TempHome::new();
        // 双端点:Anthropic 槽优先 → api = anthropic-messages
        let providers = vec![dual_provider()];
        let a = DshProviderAdapter;

        // 空 models → skip,不落盘
        let mut no_models = dual_provider();
        no_models.models.clear();
        no_models.default_model = String::new();
        let changes = a.plan(home.path(), &[no_models.clone()], Some("p-dual"), &[]).unwrap();
        assert_eq!((changes[0].action.as_str(), changes[0].detail.as_str()), ("skip", "No model configured"));

        let changes = a.plan(home.path(), &providers, Some("p-dual"), &[]).unwrap();
        assert_eq!(changes[0].action, "add");
        assert_eq!(a.apply(home.path(), &providers, Some("p-dual"), &[]).unwrap(), 1);

        let sp = home.path().join(".dsh").join("settings.yaml");
        let cp = home.path().join(".dsh").join(".credentials.yaml");
        let s = read_yaml_file(&sp);
        let route = &s["llm-pi-ai"]["providers"]["clawbox"];
        assert_eq!(route["apiKeyEnv"], "CLAWBOX_DSH_API_KEY");
        assert_eq!(route["api"], "anthropic-messages");
        assert_eq!(route["baseURL"], "https://api.dual.example.com/anthropic");
        assert_eq!(route["models"][0]["id"], "model-a");
        let c = read_yaml_file(&cp);
        assert_eq!(c["version"], 1);
        assert_eq!(c["refs"]["CLAWBOX_DSH_API_KEY"], "sk-secret-123");
        // dsh 要求凭据文件 0600
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&cp).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
        }

        a.validate(home.path()).unwrap();
        // 幂等:再 plan → unchanged
        let changes = a.plan(home.path(), &providers, Some("p-dual"), &["clawbox".into()]).unwrap();
        assert_eq!(changes[0].action, "unchanged");

        // 手改 baseURL → update(漂移)
        let mut tampered = read_yaml_file(&sp);
        tampered["llm-pi-ai"]["providers"]["clawbox"]["baseURL"] =
            serde_yaml::Value::String("https://tampered.example.com".into());
        std::fs::write(&sp, serde_yaml::to_string(&tampered).unwrap()).unwrap();
        let changes = a.plan(home.path(), &providers, Some("p-dual"), &["clawbox".into()]).unwrap();
        assert_eq!(changes[0].action, "update");
    }

    #[test]
    fn dsh_openai_only_uses_openai_completions() {
        let home = TempHome::new();
        let providers = vec![openai_provider()];
        let a = DshProviderAdapter;
        a.apply(home.path(), &providers, Some("p-oa"), &[]).unwrap();
        let s = read_yaml_file(&home.path().join(".dsh").join("settings.yaml"));
        assert_eq!(s["llm-pi-ai"]["providers"]["clawbox"]["api"], "openai-completions");
        assert_eq!(s["llm-pi-ai"]["providers"]["clawbox"]["baseURL"], "https://api.oa.example.com/v1");
    }

    #[test]
    fn dsh_unbind_removes_route_and_ref_keeps_user_entries() {
        let home = TempHome::new();
        let providers = vec![dual_provider()];
        let a = DshProviderAdapter;
        // 预置用户自有 route 与 ref
        let dir = home.path().join(".dsh");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("settings.yaml"),
            "llm-pi-ai:\n  providers:\n    my-gateway:\n      api: openai-completions\n",
        )
        .unwrap();
        std::fs::write(
            dir.join(".credentials.yaml"),
            "version: 1\nrefs:\n  MY_KEY: sk-mine\n",
        )
        .unwrap();
        a.apply(home.path(), &providers, Some("p-dual"), &[]).unwrap();

        // 解绑:删我们的 route 与 ref;用户条目保留
        {
            let mut doc: serde_yaml::Value = serde_yaml::from_str("llm-pi-ai:\n  providers:\n    my-gateway:\n      api: x\n").unwrap();
            let root = doc.as_mapping_mut().unwrap();
            let llm = root.entry(ystr("llm-pi-ai")).or_insert_with(|| serde_yaml::Value::Mapping(serde_yaml::Mapping::new()));
        }
        assert_eq!(
            a.apply(home.path(), &providers, None, &["clawbox".into()]).unwrap(),
            1
        );
        let s = read_yaml_file(&dir.join("settings.yaml"));
        assert!(s["llm-pi-ai"]["providers"].get("clawbox").is_none());
        assert!(s["llm-pi-ai"]["providers"].get("my-gateway").is_some());
        let c = read_yaml_file(&dir.join(".credentials.yaml"));
        assert!(c["refs"].get("CLAWBOX_DSH_API_KEY").is_none());
        assert_eq!(c["refs"]["MY_KEY"], "sk-mine");
    }
}
