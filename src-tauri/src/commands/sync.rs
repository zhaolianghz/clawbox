//! Tauri commands for MCP unified deployment. Thin wrappers: all logic
//! lives in `crate::sync` / `crate::commands::config` so it stays testable
//! against a tempdir home.

use crate::commands::config::{load_config, real_home, save_config, Config, McpServerSpec, ProviderSpec};
use crate::sync::{self, providers, AgentPlan, ApplyResult};
use crate::sync::providers::{AdoptedProvider, Slot};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap};
use std::path::Path;

#[tauri::command]
pub async fn config_mcp_list() -> Result<BTreeMap<String, McpServerSpec>, String> {
    Ok(load_config(&real_home())?.mcp_servers)
}

/// Home-parameterized core of `config_mcp_upsert` so tests can point it at a
/// tempdir. A corrupt config file must fail here *before* any write.
pub fn mcp_upsert_at(home: &Path, name: String, spec: McpServerSpec) -> Result<(), String> {
    if name.trim().is_empty() {
        return Err("server name must not be empty".to_string());
    }
    if !matches!(spec.kind.as_str(), "stdio" | "http") {
        return Err(format!("unsupported server kind: {}", spec.kind));
    }
    let mut config = load_config(home)?;
    config.mcp_servers.insert(name, spec);
    save_config(home, &config)
}

#[tauri::command]
pub async fn config_mcp_upsert(name: String, spec: McpServerSpec) -> Result<(), String> {
    mcp_upsert_at(&real_home(), name, spec)
}

#[tauri::command]
pub async fn config_mcp_remove(name: String) -> Result<(), String> {
    let home = real_home();
    let mut config = load_config(&home)?;
    if config.mcp_servers.remove(&name).is_none() {
        return Err(format!("unknown MCP server: {}", name));
    }
    save_config(&home, &config)
}

#[tauri::command]
pub async fn sync_mcp_plan() -> Result<Vec<AgentPlan>, String> {
    let home = real_home();
    let config = load_config(&home)?;
    Ok(sync::plan_all(&home, &config.mcp_servers, &config.mcp_managed))
}

/// Apply to the selected agents one by one. Per-agent failures land in the
/// corresponding ApplyResult; a success updates that agent's `mcp_managed`
/// entry to exactly the set deployed this run.
#[tauri::command]
pub async fn sync_mcp_apply(agent_ids: Vec<String>) -> Result<Vec<ApplyResult>, String> {
    let home = real_home();
    let mut config = load_config(&home)?;
    let mut results = Vec::with_capacity(agent_ids.len());

    for id in agent_ids {
        let Some(adapter) = sync::find_adapter(&id) else {
            results.push(ApplyResult {
                agent_id: id,
                ok: false,
                backup_path: None,
                applied: 0,
                error: Some("unknown agent".to_string()),
            });
            continue;
        };
        let managed = config.mcp_managed.get(&id).cloned().unwrap_or_default();
        let result = sync::apply_one(&home, adapter, &config.mcp_servers, &managed);
        if result.ok {
            config
                .mcp_managed
                .insert(id, adapter.deployed_names(&config.mcp_servers));
            save_config(&home, &config)?;
        }
        results.push(result);
    }
    Ok(results)
}

// ---- 服务商 per-agent 绑定:选中即生效 --------------------------------------

/// `agent_provider_bind` 的 home 参数化核心。
///
/// Some(id) = 绑定/切换:校验后对该 agent 只下发这一家(单元素列表——多
/// 服务商适配器由此只写绑定项,旧条目走 managed 差集自然清除),apply 成功
/// 才落盘绑定。None = 解绑:按 providers_managed 清掉我们写过的键,恢复
/// agent 原状(hermes 无 remove 语义,只停止管理、保留现值)。
pub fn agent_provider_bind_at(
    home: &Path,
    agent_id: &str,
    provider_id: Option<String>,
) -> Result<ApplyResult, String> {
    let mut config = load_config(home)?;
    let Some(adapter) = providers::find_adapter(agent_id) else {
        return Err(format!("unknown agent: {}", agent_id));
    };
    let managed = config.providers_managed.get(agent_id).cloned().unwrap_or_default();
    match provider_id {
        Some(pid) => {
            let Some(spec) = config.providers.iter().find(|p| p.id == pid) else {
                return Err(format!("unknown provider id: {}", pid));
            };
            if !spec.enabled {
                return Err(format!("provider {} is disabled", spec.name));
            }
            let bound = vec![spec.clone()];
            // deployed_names 为空 = 这家在该 agent 下发不了(端点槽不符/
            // 缺 API key/agent 不支持)。错误信息不含 apiKey。
            let deployed = adapter.deployed_names(&bound, Some(&pid));
            if deployed.is_empty() {
                return Err(format!(
                    "provider {} cannot be deployed to {} (endpoint slot mismatch, missing API key, or unsupported agent)",
                    spec.name, agent_id
                ));
            }
            let result = providers::apply_one(home, adapter, &bound, Some(&pid), &managed);
            if result.ok {
                config.agent_providers.insert(agent_id.to_string(), pid);
                config.providers_managed.insert(agent_id.to_string(), deployed);
                save_config(home, &config)?;
            }
            Ok(result)
        }
        None => {
            let result = providers::apply_one(home, adapter, &[], None, &managed);
            if result.ok {
                config.agent_providers.remove(agent_id);
                config.providers_managed.remove(agent_id);
                save_config(home, &config)?;
            }
            Ok(result)
        }
    }
}

#[tauri::command]
pub async fn agent_provider_bind(
    agent_id: String,
    provider_id: Option<String>,
) -> Result<ApplyResult, String> {
    let _guard = crate::commands::config::CONFIG_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    agent_provider_bind_at(&real_home(), &agent_id, provider_id)
}

/// 绑定表只读快照(agents 页选择器当前值 / providers 页「使用中」徽章)。
#[tauri::command]
pub async fn agent_providers_get() -> Result<HashMap<String, String>, String> {
    Ok(load_config(&real_home())?.agent_providers)
}

/// fallback 链设置核心(同步、home 参数化)。空链 = 清空该 agent 的 fallback。
/// 不支持 fallback 的 agent 返 Err;逐家校验可下发性(端点槽 + key + model)。
pub fn agent_fallbacks_set_at(
    home: &Path,
    agent_id: &str,
    fallback_ids: Vec<String>,
) -> Result<ApplyResult, String> {
    let mut config = load_config(home)?;
    let Some(adapter) = providers::find_adapter(agent_id) else {
        return Err(format!("unknown agent: {}", agent_id));
    };
    if !adapter.supports_fallback() {
        return Err(format!("{} does not support a fallback chain", agent_id));
    }
    // 去重保序(同名 fallback 只取首次),primary 不允许同时当 fallback。
    let primary = config.agent_providers.get(agent_id);
    let mut seen = std::collections::HashSet::new();
    let mut chain: Vec<String> = Vec::new();
    for fid in &fallback_ids {
        if primary == Some(fid) {
            continue; // primary 已是激活,fallback 链里不重复
        }
        if !seen.insert(fid.clone()) {
            continue;
        }
        chain.push(fid.clone());
    }
    let mut fb_specs: Vec<ProviderSpec> = Vec::new();
    for fid in &chain {
        let Some(spec) = config.providers.iter().find(|p| &p.id == fid) else {
            return Err(format!("unknown provider id: {}", fid));
        };
        if !spec.enabled {
            return Err(format!("provider {} is disabled", spec.name));
        }
        if !adapter.fallback_deployable(spec) {
            return Err(format!(
                "provider {} cannot be a fallback for {} (configure its endpoint, API key, and default model first)",
                spec.name, agent_id
            ));
        }
        fb_specs.push(spec.clone());
    }
    let fb_managed = config
        .providers_fallback_managed
        .get(agent_id)
        .cloned()
        .unwrap_or_default();
    let result = providers::apply_fallbacks_one(home, adapter, &fb_specs, &fb_managed);
    if result.ok {
        if chain.is_empty() {
            config.agent_fallbacks.remove(agent_id);
            config.providers_fallback_managed.remove(agent_id);
        } else {
            let deployed = adapter.deployed_fallback_names(&fb_specs);
            config.agent_fallbacks.insert(agent_id.to_string(), chain);
            config
                .providers_fallback_managed
                .insert(agent_id.to_string(), deployed);
        }
        save_config(home, &config)?;
    }
    Ok(result)
}

#[tauri::command]
pub async fn agent_fallbacks_set(
    agent_id: String,
    fallback_ids: Vec<String>,
) -> Result<ApplyResult, String> {
    let _guard = crate::commands::config::CONFIG_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    agent_fallbacks_set_at(&real_home(), &agent_id, fallback_ids)
}

/// fallback 链只读快照(agent_id -> 有序 provider id 链)。
#[tauri::command]
pub async fn agent_fallbacks_get() -> Result<HashMap<String, Vec<String>>, String> {
    Ok(load_config(&real_home())?.agent_fallbacks)
}

/// 手动强制重推该 agent 的当前 provider 绑定。reconcile 默认不再自动覆盖
/// 疑似用户手改(update 类漂移),改由这个显式动作愈合——用户在同步详情里
/// 看到「已过期」时点它。
pub fn agent_provider_resync_at(home: &Path, agent_id: &str) -> Result<ApplyResult, String> {
    let config = load_config(home)?;
    let pid = config
        .agent_providers
        .get(agent_id)
        .ok_or_else(|| format!("no provider bound to {}", agent_id))?;
    agent_provider_bind_at(home, agent_id, Some(pid.clone()))
}

#[tauri::command]
pub async fn agent_provider_resync(agent_id: String) -> Result<ApplyResult, String> {
    let _guard = crate::commands::config::CONFIG_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    agent_provider_resync_at(&real_home(), &agent_id)
}

/// agent → ClawBox「领养」:读 agent 当前在用的服务商,在 ClawBox 里 upsert 一条
/// 同名 ProviderSpec 并绑定。漂移时三态 resolve 的「采用 agent 现值」即调它。
/// created=true=新建了服务商,false=更新了已存在的同名条目。
#[derive(Serialize, Clone, Debug)]
pub struct AdoptResult {
    pub provider_id: String,
    pub provider_name: String,
    pub created: bool,
}

fn apply_adopted_to_spec(p: &mut ProviderSpec, a: &AdoptedProvider) {
    p.api_key = a.api_key.clone();
    p.anthropic_base_url = if a.slot == Slot::Anthropic {
        a.base_url.clone()
    } else {
        String::new()
    };
    p.openai_base_url = if a.slot == Slot::Openai {
        a.base_url.clone()
    } else {
        String::new()
    };
    if !a.default_model.is_empty() {
        p.default_model = a.default_model.clone();
    }
    if !a.models.is_empty() {
        p.models = a.models.clone();
    }
    p.enabled = true;
}

pub fn agent_provider_adopt_at(home: &Path, agent_id: &str) -> Result<AdoptResult, String> {
    let adapter = providers::find_adapter(agent_id)
        .ok_or_else(|| format!("unknown agent: {}", agent_id))?;
    let adopted = adapter
        .extract_active(home)?
        .ok_or_else(|| format!("no recognizable active provider in {}'s config", agent_id))?;
    let mut config = load_config(home)?;
    // 同名则更新,否则新建
    let (id, created) = match config.providers.iter_mut().find(|p| p.name == adopted.name) {
        Some(p) => {
            apply_adopted_to_spec(p, &adopted);
            (p.id.clone(), false)
        }
        None => {
            let id = uuid::Uuid::new_v4().to_string();
            let mut spec = ProviderSpec {
                id: id.clone(),
                name: adopted.name.clone(),
                api_key: String::new(),
                base_url: String::new(),
                anthropic_base_url: String::new(),
                openai_base_url: String::new(),
                default_model: String::new(),
                models: vec![],
                enabled: true,
                flavor: None,
            };
            apply_adopted_to_spec(&mut spec, &adopted);
            config.providers.push(spec);
            (id, true)
        }
    };
    save_config(home, &config)?;
    // 绑定 → re-push(此刻 agent 与 ClawBox 两边一致,幂等,过 validate)
    let r = agent_provider_bind_at(home, agent_id, Some(id.clone()))?;
    if !r.ok {
        return Err(r.error.unwrap_or_else(|| "bind failed after adopt".to_string()));
    }
    Ok(AdoptResult {
        provider_id: id,
        provider_name: adopted.name,
        created,
    })
}

#[tauri::command]
pub async fn agent_provider_adopt(agent_id: String) -> Result<AdoptResult, String> {
    let _guard = crate::commands::config::CONFIG_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    agent_provider_adopt_at(&real_home(), &agent_id)
}

/// 漂移横幅要显示的「agent 现在在用的服务商」信息(只取名字+模型,不含 key)。
#[derive(Serialize, Clone, Debug)]
pub struct ActiveProviderInfo {
    pub name: String,
    pub model: String,
}

/// 批量取若干 agent 当前激活服务商的名字+模型(供三态横幅显示「现在用 X」)。
/// null = 读不出(未实现 extract_active 的 agent,或 agent 没配激活服务商)。
#[tauri::command]
pub async fn agent_active_providers_get(
    agent_ids: Vec<String>,
) -> Result<HashMap<String, Option<ActiveProviderInfo>>, String> {
    let home = real_home();
    let mut out = HashMap::new();
    for id in agent_ids {
        let info = providers::find_adapter(&id)
            .and_then(|a| a.extract_active(&home).ok().flatten())
            .map(|ad| ActiveProviderInfo {
                name: ad.name,
                model: ad.default_model,
            });
        out.insert(id, info);
    }
    Ok(out)
}

/// 启动对账:检测漂移,但**默认不自动覆盖**——只在「安全补写」时才自愈:
/// 我们管理的键缺失(add)且无 update/remove(update 可能是用户手改过的
/// 我们的键,无声覆盖会破坏信任)。疑似手改的 update 漂移只由 UI 标红,
/// 用户点 agent_provider_resync 显式愈合。无漂移零写入。静默执行:失败不
/// 打断启动,下次启动或用户操作时自然重试并可见报错。
pub fn reconcile_bindings_at(home: &Path) -> Vec<ApplyResult> {
    let Ok(config) = load_config(home) else {
        return vec![];
    };
    let mut results = Vec::new();
    for (agent_id, pid) in &config.agent_providers {
        let Some(adapter) = providers::find_adapter(agent_id) else {
            continue;
        };
        let Some(spec) = config.providers.iter().find(|p| &p.id == pid) else {
            continue;
        };
        if !spec.enabled {
            continue; // 绑定指向已禁用服务商:Agents 页已有「请重选」提示,不代用户做主
        }
        let bound = vec![spec.clone()];
        let managed = config.providers_managed.get(agent_id).cloned().unwrap_or_default();
        // 只自动愈合「安全补写」:有 add(我们的键缺失)且无 update/remove
        // (update = 键在但值不同,可能是用户手改 → 不自动覆盖)。
        let safe_heal = match adapter.plan(home, &bound, Some(pid), &managed) {
            Ok(changes) => {
                changes.iter().any(|c| c.action == "add")
                    && !changes.iter().any(|c| c.action == "update" || c.action == "remove")
            }
            // 目标文件读不了/解析不了:不动它,留给用户操作路径显式报错。
            Err(_) => false,
        };
        if !safe_heal {
            continue;
        }
        match agent_provider_bind_at(home, agent_id, Some(pid.clone())) {
            Ok(r) => results.push(r),
            Err(e) => results.push(ApplyResult {
                agent_id: agent_id.clone(),
                ok: false,
                backup_path: None,
                applied: 0,
                error: Some(e),
            }),
        }
    }
    // fallback 链漂移:对每个有 fallback 绑定的 agent,若 plan_fallbacks 报漂移
    // 就重推。不支持 fallback 的 agent plan 返回空 → 跳过。
    for (agent_id, chain) in &config.agent_fallbacks {
        let Some(adapter) = providers::find_adapter(agent_id) else {
            continue;
        };
        if !adapter.supports_fallback() {
            continue;
        }
        let fb_specs: Vec<ProviderSpec> = chain
            .iter()
            .filter_map(|fid| config.providers.iter().find(|p| &p.id == fid))
            .filter(|p| p.enabled)
            .cloned()
            .collect();
        let fb_managed = config
            .providers_fallback_managed
            .get(agent_id)
            .cloned()
            .unwrap_or_default();
        let drifted = match adapter.plan_fallbacks(home, &fb_specs, &fb_managed) {
            Ok(changes) => {
                changes.iter().any(|c| c.action == "add")
                    && !changes.iter().any(|c| c.action == "update" || c.action == "remove")
            }
            Err(_) => false,
        };
        if !drifted {
            continue;
        }
        match agent_fallbacks_set_at(home, agent_id, chain.clone()) {
            Ok(r) => results.push(r),
            Err(e) => results.push(ApplyResult {
                agent_id: agent_id.clone(),
                ok: false,
                backup_path: None,
                applied: 0,
                error: Some(e),
            }),
        }
    }
    results
}

/// 启动时后台跑一次对账(lib.rs setup 调用)。持 CONFIG_LOCK,与用户同时
/// 触发的 config 命令互斥。
pub fn reconcile_bindings_on_startup() {
    std::thread::spawn(|| {
        let _guard = crate::commands::config::CONFIG_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        for r in reconcile_bindings_at(&real_home()) {
            if !r.ok {
                eprintln!(
                    "[clawbox] startup reconcile failed for {}: {}",
                    r.agent_id,
                    r.error.as_deref().unwrap_or("unknown error")
                );
            }
        }
    });
}

// ---- 技能(skills)统一同步:库管理 / 收编 / plan / apply --------------------

/// 库列表 + 来源 join 的 home 参数化核心。
pub fn skills_library_list_at(home: &Path) -> Result<Vec<sync::skills::SkillEntry>, String> {
    let sources = load_config(home)?.skills_sources;
    let mut list = sync::skills::list_library(home)?;
    for entry in &mut list {
        entry.source = sources.get(&entry.name).cloned();
    }
    Ok(list)
}

#[tauri::command]
pub async fn skills_library_list() -> Result<Vec<sync::skills::SkillEntry>, String> {
    skills_library_list_at(&real_home())
}

#[tauri::command]
pub async fn skills_import(src_dir: String) -> Result<sync::skills::SkillEntry, String> {
    sync::skills::import(&real_home(), Path::new(&src_dir))
}

/// 删库目录并联动删除来源记录的 home 参数化核心。
pub fn skills_library_remove_at(home: &Path, name: &str) -> Result<(), String> {
    sync::skills::remove_from_library(home, name)?;
    let mut config = load_config(home)?;
    if config.skills_sources.remove(name).is_some() {
        save_config(home, &config)?;
    }
    Ok(())
}

#[tauri::command]
pub async fn skills_library_remove(name: String) -> Result<(), String> {
    skills_library_remove_at(&real_home(), &name)
}

#[tauri::command]
pub async fn skills_scan() -> Result<Vec<sync::skills::AdoptCandidate>, String> {
    sync::skills::scan(&real_home())
}

/// 收编成功的条目并入 skills_managed:收编产物就是我们的链,库中删除后的
/// remove 清理依赖这里记账。
#[tauri::command]
pub async fn skills_adopt(
    items: Vec<sync::skills::AdoptRequest>,
) -> Result<Vec<sync::skills::AdoptOutcome>, String> {
    let home = real_home();
    let outcomes = sync::skills::adopt(&home, &items);
    let mut config = load_config(&home)?;
    let mut changed = false;
    for o in outcomes.iter().filter(|o| o.ok) {
        let entry = config.skills_managed.entry(o.agent_id.clone()).or_default();
        if !entry.contains(&o.name) {
            entry.push(o.name.clone());
            changed = true;
        }
    }
    if changed {
        save_config(&home, &config)?;
    }
    Ok(outcomes)
}

#[tauri::command]
pub async fn sync_skills_plan() -> Result<Vec<AgentPlan>, String> {
    let home = real_home();
    let config = load_config(&home)?;
    Ok(sync::skills::plan_all(&home, &config.skills_managed))
}

#[tauri::command]
pub async fn sync_skills_apply(agent_ids: Vec<String>) -> Result<Vec<ApplyResult>, String> {
    let home = real_home();
    let mut config = load_config(&home)?;
    let mut results = Vec::with_capacity(agent_ids.len());
    for id in agent_ids {
        let managed = config.skills_managed.get(&id).cloned().unwrap_or_default();
        let result = sync::skills::apply_one(&home, &id, &managed);
        if result.ok {
            let deployed = sync::skills::deployed_names(&home, &result.agent_id);
            config.skills_managed.insert(id, deployed);
            save_config(&home, &config)?;
        }
        results.push(result);
    }
    Ok(results)
}

// ---- 技能 Git 仓库安装:discover / install / check-updates / update ----------

#[tauri::command]
pub async fn skills_repo_discover(repo: String) -> Result<sync::skills::RepoDiscovery, String> {
    sync::skills::repo_discover(&real_home(), &repo)
}

/// 安装成功的技能写入 skills_sources 的 home 参数化核心。
pub fn skills_repo_install_at(
    home: &Path,
    repo: &str,
    subdirs: &[String],
) -> Result<Vec<sync::skills::InstallOutcome>, String> {
    let (outcomes, sources) = sync::skills::repo_install(home, repo, subdirs)?;
    if !sources.is_empty() {
        let mut config = load_config(home)?;
        config.skills_sources.extend(sources);
        save_config(home, &config)?;
    }
    Ok(outcomes)
}

#[tauri::command]
pub async fn skills_repo_install(
    repo: String,
    subdirs: Vec<String>,
) -> Result<Vec<sync::skills::InstallOutcome>, String> {
    skills_repo_install_at(&real_home(), &repo, &subdirs)
}

#[tauri::command]
pub async fn skills_check_updates() -> Result<Vec<sync::skills::SkillUpdateInfo>, String> {
    let config = load_config(&real_home())?;
    sync::skills::check_updates(&config.skills_sources)
}

/// 覆盖更新的 home 参数化核心:更新成功的技能刷新 source 记录。
pub fn skills_update_at(
    home: &Path,
    names: &[String],
) -> Result<Vec<sync::skills::InstallOutcome>, String> {
    let mut config = load_config(home)?;
    let (outcomes, updated) = sync::skills::update(home, names, &config.skills_sources);
    if !updated.is_empty() {
        config.skills_sources.extend(updated);
        save_config(home, &config)?;
    }
    Ok(outcomes)
}

#[tauri::command]
pub async fn skills_update(names: Vec<String>) -> Result<Vec<sync::skills::InstallOutcome>, String> {
    skills_update_at(&real_home(), &names)
}

// ---- 统一指令记忆同步:库读写 / 目标概况 / plan / apply ----------------------

#[tauri::command]
pub async fn memory_read() -> Result<String, String> {
    sync::memory::read_library(&real_home())
}

#[tauri::command]
pub async fn memory_write(content: String) -> Result<(), String> {
    sync::memory::write_library(&real_home(), &content).map(|_| ())
}

#[tauri::command]
pub async fn memory_targets() -> Result<Vec<sync::memory::MemoryTarget>, String> {
    sync::memory::targets(&real_home())
}

#[tauri::command]
pub async fn memory_target_content(agent_id: String) -> Result<String, String> {
    sync::memory::target_content(&real_home(), &agent_id)
}

#[tauri::command]
pub async fn sync_memory_plan() -> Result<Vec<AgentPlan>, String> {
    let home = real_home();
    let config = load_config(&home)?;
    Ok(sync::memory::plan_all(&home, &config.memory_managed))
}

#[tauri::command]
pub async fn sync_memory_apply(agent_ids: Vec<String>) -> Result<Vec<ApplyResult>, String> {
    let home = real_home();
    let mut config = load_config(&home)?;
    let mut results = Vec::with_capacity(agent_ids.len());
    for id in agent_ids {
        let managed = config.memory_managed.get(&id).cloned().unwrap_or_default();
        let result = sync::memory::apply_one(&home, &id, &managed);
        if result.ok {
            let deployed = sync::memory::deployed_names(&home, &result.agent_id);
            config.memory_managed.insert(id, deployed);
            save_config(&home, &config)?;
        }
        results.push(result);
    }
    Ok(results)
}

// ---- agent 同步清单总览(Agent 管理页) --------------------------------------

#[derive(Serialize, Clone, Debug)]
pub struct SyncedItem {
    pub name: String,
    /// "synced"(已下发且一致) | "unsynced"(从未下发) | "outdated"
    /// (下发过但配置已变,待重新同步) | "removing"(不再管理,下次同步将移除)
    pub state: String,
}

#[derive(Serialize, Clone, Debug)]
pub struct AgentSyncOverview {
    pub agent_id: String,
    pub provider_supported: bool,
    pub mcp_supported: bool,
    pub skills_supported: bool,
    pub memory_supported: bool,
    /// CLI 型适配器无文件的为空串。
    pub provider_config_path: String,
    pub mcp_config_path: String,
    /// 该 agent 的技能目录。
    pub skills_config_path: String,
    /// 该 agent 的指令记忆文件。
    pub memory_config_path: String,
    pub providers: Vec<SyncedItem>,
    pub mcp: Vec<SyncedItem>,
    pub skills: Vec<SyncedItem>,
    pub memory: Vec<SyncedItem>,
    /// plan 失败(如 CLI 未装)时的错误;此时对应列表留空。
    pub provider_error: Option<String>,
    pub mcp_error: Option<String>,
    pub skills_error: Option<String>,
    pub memory_error: Option<String>,
}

/// ChangeItem → 清单条目:unchanged→synced,add→unsynced(从未下发),
/// update→outdated(下发过但已变),remove→removing。skip 不进列表 ——
/// 那是"不会下发",不是"下发了什么"。列表因此 = ClawBox 管理视角
/// (managed ∪ plan 非 skip 项),不含 agent 原生配置里用户自己的条目。
fn synced_items(changes: &[sync::ChangeItem]) -> Vec<SyncedItem> {
    changes
        .iter()
        .filter_map(|c| {
            let state = match c.action.as_str() {
                "unchanged" => "synced",
                "add" => "unsynced",
                "update" => "outdated",
                "remove" => "removing",
                _ => return None, // skip
            };
            Some(SyncedItem {
                name: c.name.clone(),
                state: state.into(),
            })
        })
        .collect()
}

/// 服务商 / MCP / 技能三份 plan 逐 agent 归并(纯函数,可测)。骨架取服务
/// 商 plan 的注册表顺序;三个注册表 id 集合一致(注册表测试保证)。
/// plan_all 在 Err 时 changes 已为空,列表自然留空、错误进 *_error。
fn merge_overview(
    provider_plans: Vec<AgentPlan>,
    mcp_plans: Vec<AgentPlan>,
    skills_plans: Vec<AgentPlan>,
    memory_plans: Vec<AgentPlan>,
) -> Vec<AgentSyncOverview> {
    let by_id = |plans: Vec<AgentPlan>| -> HashMap<String, AgentPlan> {
        plans.into_iter().map(|p| (p.agent_id.clone(), p)).collect()
    };
    let mut mcp_by_id = by_id(mcp_plans);
    let mut skills_by_id = by_id(skills_plans);
    let mut memory_by_id = by_id(memory_plans);
    // (supported, config_path, 条目, error);注册表缺席按 unsupported 兜底。
    let split = |plan: Option<AgentPlan>| match plan {
        Some(p) => (p.supported, p.config_path, synced_items(&p.changes), p.error),
        None => (false, String::new(), vec![], None),
    };
    provider_plans
        .into_iter()
        .map(|pp| {
            let (mcp_supported, mcp_config_path, mcp, mcp_error) =
                split(mcp_by_id.remove(&pp.agent_id));
            let (skills_supported, skills_config_path, skills, skills_error) =
                split(skills_by_id.remove(&pp.agent_id));
            let (memory_supported, memory_config_path, memory, memory_error) =
                split(memory_by_id.remove(&pp.agent_id));
            AgentSyncOverview {
                agent_id: pp.agent_id,
                provider_supported: pp.supported,
                mcp_supported,
                skills_supported,
                memory_supported,
                provider_config_path: pp.config_path,
                mcp_config_path,
                skills_config_path,
                memory_config_path,
                providers: synced_items(&pp.changes),
                mcp,
                skills,
                memory,
                provider_error: pp.error,
                mcp_error,
                skills_error,
                memory_error,
            }
        })
        .collect()
}

/// `agent_sync_overview` 的 home 参数化核心。
pub fn agent_sync_overview_at(home: &Path, config: &Config) -> Vec<AgentSyncOverview> {
    let provider_plans = providers::plan_all(
        home,
        &config.providers,
        &config.agent_providers,
        &config.providers_managed,
    );
    let mcp_plans = sync::plan_all(home, &config.mcp_servers, &config.mcp_managed);
    let skills_plans = sync::skills::plan_all(home, &config.skills_managed);
    let memory_plans = sync::memory::plan_all(home, &config.memory_managed);
    merge_overview(provider_plans, mcp_plans, skills_plans, memory_plans)
}

#[tauri::command]
pub async fn agent_sync_overview() -> Result<Vec<AgentSyncOverview>, String> {
    let home = real_home();
    let config = load_config(&home)?;
    Ok(agent_sync_overview_at(&home, &config))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::config::{clawbox_config_path, ProviderSpec};
    use crate::sync::test_util::TempHome;
    use std::fs;

    fn stdio_spec() -> McpServerSpec {
        McpServerSpec {
            kind: "stdio".to_string(),
            command: Some("npx".to_string()),
            args: vec!["my-mcp".to_string()],
            env: BTreeMap::new(),
            url: None,
            headers: BTreeMap::new(),
            enabled: true,
        }
    }

    #[test]
    fn upsert_on_corrupt_config_fails_and_leaves_file_untouched() {
        let home = TempHome::new();
        let path = clawbox_config_path(home.path());
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let corrupt = r#"{"models": TRUNCATED"#;
        fs::write(&path, corrupt).unwrap();

        let err = mcp_upsert_at(home.path(), "srv".to_string(), stdio_spec()).unwrap_err();
        assert!(err.contains("corrupt"), "unexpected error: {}", err);

        // The whole point: the broken file must not be overwritten.
        assert_eq!(fs::read_to_string(&path).unwrap(), corrupt);
    }

    #[test]
    fn upsert_on_missing_config_creates_it() {
        let home = TempHome::new();
        mcp_upsert_at(home.path(), "srv".to_string(), stdio_spec()).unwrap();
        let loaded = load_config(home.path()).unwrap();
        assert!(loaded.mcp_servers.contains_key("srv"));
    }

    // ---- 服务商测试共用 fixture ----
    // 铁律不变:全程 TempHome;hermes 只走 plan / 手写落盘文件,绝不触发其
    // apply 的 CLI 段。

    /// 双端点 fixture:anthropic_url / openai_url 任一为空 = 该槽未配置。
    fn pspec(id: &str, name: &str, anthropic_url: &str, openai_url: &str) -> ProviderSpec {
        ProviderSpec {
            id: id.to_string(),
            name: name.to_string(),
            api_key: "sk-secret-123".to_string(),
            base_url: String::new(),
            anthropic_base_url: anthropic_url.to_string(),
            openai_base_url: openai_url.to_string(),
            default_model: "model-a".to_string(),
            models: vec!["model-a".to_string()],
            enabled: true,
            flavor: None,
        }
    }

    fn apply_provider_adapter(home: &Path, agent: &str, specs: &[ProviderSpec], active: Option<&str>) {
        providers::find_adapter(agent)
            .unwrap()
            .apply(home, specs, active, &[])
            .unwrap();
    }

    // ---- 技能安装:list join source / remove 联动清理 ----

    #[test]
    fn skills_source_joins_into_list_and_remove_cleans_up() {
        let home = TempHome::new();
        // 本地仓造一个技能并安装(记录 source)
        let repo = home.path().join("srcrepo");
        fs::create_dir_all(repo.join("sk")).unwrap();
        fs::write(repo.join("sk").join("SKILL.md"), "---\ndescription: d\n---\n").unwrap();
        for args in [
            vec!["init"],
            vec!["add", "-A"],
            vec!["commit", "-m", "init"],
        ] {
            let out = std::process::Command::new("git")
                .args(&args)
                .current_dir(&repo)
                .env("GIT_TERMINAL_PROMPT", "0")
                .env("GIT_AUTHOR_NAME", "t")
                .env("GIT_AUTHOR_EMAIL", "t@example.com")
                .env("GIT_COMMITTER_NAME", "t")
                .env("GIT_COMMITTER_EMAIL", "t@example.com")
                .output()
                .unwrap();
            assert!(out.status.success(), "git {:?}", args);
        }
        let url = format!("file://{}", repo.display());
        let outcomes = skills_repo_install_at(home.path(), &url, &["sk".to_string()]).unwrap();
        assert!(outcomes[0].ok, "{}", outcomes[0].detail);
        // config 落了 source;list join 出来
        let config = load_config(home.path()).unwrap();
        assert_eq!(config.skills_sources["sk"].repo, url);
        let list = skills_library_list_at(home.path()).unwrap();
        let sk = list.iter().find(|s| s.name == "sk").unwrap();
        let src = sk.source.as_ref().expect("source joined");
        assert_eq!(src.repo, url);
        assert_eq!(src.subdir, "sk");
        assert_eq!(src.commit.len(), 40);
        // 手动导入的技能没有 source
        fs::create_dir_all(sync::skills::library_dir(home.path()).join("manual")).unwrap();
        fs::write(
            sync::skills::library_dir(home.path()).join("manual").join("SKILL.md"),
            "---\n---\n",
        )
        .unwrap();
        let list = skills_library_list_at(home.path()).unwrap();
        assert!(list.iter().find(|s| s.name == "manual").unwrap().source.is_none());

        // remove 联动删 source 记录
        skills_library_remove_at(home.path(), "sk").unwrap();
        assert!(!sync::skills::library_dir(home.path()).join("sk").exists());
        assert!(load_config(home.path()).unwrap().skills_sources.get("sk").is_none());
    }

    // ---- agent_sync_overview 归并 ----

    fn plan(id: &str, supported: bool, changes: Vec<(&str, &str)>, error: Option<&str>) -> AgentPlan {
        AgentPlan {
            agent_id: id.to_string(),
            supported,
            config_path: if supported { format!("/home/x/{}.json", id) } else { String::new() },
            changes: changes
                .into_iter()
                .map(|(name, action)| sync::ChangeItem {
                    name: name.to_string(),
                    action: action.to_string(),
                    detail: String::new(),
                })
                .collect(),
            error: error.map(|e| e.to_string()),
        }
    }

    fn overview_of<'a>(all: &'a [AgentSyncOverview], id: &str) -> &'a AgentSyncOverview {
        all.iter()
            .find(|o| o.agent_id == id)
            .unwrap_or_else(|| panic!("no overview for {}", id))
    }

    #[test]
    fn overview_merge_maps_actions_filters_skip_and_carries_errors() {
        let provider_plans = vec![
            plan(
                "claude-code",
                true,
                vec![("OA Relay", "unchanged"), ("skipped", "skip")],
                None,
            ),
            plan("opencode", true, vec![("A", "add"), ("B", "update"), ("C", "remove")], None),
            plan("hermes", true, vec![], Some("hermes CLI exploded")),
            plan("cursor-agent", false, vec![], None),
        ];
        let mcp_plans = vec![
            plan("claude-code", true, vec![("srv", "update")], None),
            plan("opencode", true, vec![("srv", "skip")], None),
            plan("hermes", true, vec![("srv", "unchanged")], None),
            plan("cursor-agent", false, vec![], None),
        ];
        let skills_plans = vec![
            plan("claude-code", true, vec![("sk", "add"), ("old", "remove")], None),
            plan("opencode", true, vec![("sk", "unchanged")], None),
            plan("hermes", true, vec![], Some("library unreadable")),
            plan("cursor-agent", false, vec![], None),
        ];
        let memory_plans = vec![
            plan("claude-code", true, vec![("memory", "unchanged")], None),
            plan("opencode", true, vec![("memory", "skip")], None),
            plan("hermes", true, vec![("memory", "add")], None),
            plan("cursor-agent", false, vec![], None),
        ];
        let all = merge_overview(provider_plans, mcp_plans, skills_plans, memory_plans);
        assert_eq!(all.len(), 4);

        // 状态映射:unchanged→synced;skip 不进列表
        let cc = overview_of(&all, "claude-code");
        assert_eq!(cc.providers.len(), 1);
        assert_eq!((cc.providers[0].name.as_str(), cc.providers[0].state.as_str()), ("OA Relay", "synced"));
        assert_eq!(cc.mcp[0].state, "outdated");
        assert!(cc.provider_config_path.ends_with("claude-code.json"));
        // add→unsynced,update→outdated,remove→removing
        let oc = overview_of(&all, "opencode");
        let states: Vec<(&str, &str)> =
            oc.providers.iter().map(|i| (i.name.as_str(), i.state.as_str())).collect();
        assert_eq!(states, vec![("A", "unsynced"), ("B", "outdated"), ("C", "removing")]);
        assert!(oc.mcp.is_empty()); // 全 skip → 空列表
        // skills 列表与状态映射
        let cc_states: Vec<(&str, &str)> =
            cc.skills.iter().map(|i| (i.name.as_str(), i.state.as_str())).collect();
        assert_eq!(cc_states, vec![("sk", "unsynced"), ("old", "removing")]);
        assert!(cc.skills_supported);
        let oc_skills: Vec<&str> = oc.skills.iter().map(|i| i.state.as_str()).collect();
        assert_eq!(oc_skills, vec!["synced"]);
        // 单侧 plan 错误:provider_error 填充、providers 空,mcp 侧照常;
        // skills 侧错误进 skills_error
        let hermes = overview_of(&all, "hermes");
        assert_eq!(hermes.provider_error.as_deref(), Some("hermes CLI exploded"));
        assert!(hermes.providers.is_empty());
        assert_eq!(hermes.mcp[0].state, "synced");
        assert!(hermes.mcp_error.is_none());
        assert_eq!(hermes.skills_error.as_deref(), Some("library unreadable"));
        assert!(hermes.skills.is_empty());
        // memory 维度:unchanged→synced;skip 不进;error 侧列表已空
        assert_eq!(cc.memory[0].state, "synced");
        assert!(cc.memory_supported);
        assert!(oc.memory.is_empty()); // skip 不进列表
        assert_eq!(hermes.memory[0].state, "unsynced");
        // unsupported:三侧 supported=false、列表空、无错误
        let cursor = overview_of(&all, "cursor-agent");
        assert!(!cursor.provider_supported && !cursor.mcp_supported && !cursor.skills_supported);
        assert!(!cursor.memory_supported && cursor.memory.is_empty());
        assert!(cursor.providers.is_empty() && cursor.mcp.is_empty() && cursor.skills.is_empty());
        assert!(cursor.provider_error.is_none() && cursor.mcp_error.is_none() && cursor.skills_error.is_none());
    }

    #[test]
    fn overview_end_to_end_covers_registry_with_real_plans() {
        let home = TempHome::new();
        let mut config = Config::default();
        config.providers = vec![pspec("p-oa", "OA Relay", "", "https://api.oa.example.com/v1")];
        // per-agent 绑定:只有 codex 绑了服务商,其余 agent 不受管理
        config.agent_providers.insert("codex".to_string(), "p-oa".to_string());
        config.mcp_servers.insert("srv".to_string(), stdio_spec());

        // 下发 codex 的服务商配置和 claude-code 的 MCP 配置
        apply_provider_adapter(home.path(), "codex", &config.providers, Some("p-oa"));
        sync::find_adapter("claude-code")
            .unwrap()
            .apply_mcp(home.path(), &config.mcp_servers, &[])
            .unwrap();
        config.providers_managed.insert("codex".to_string(), vec!["clawbox".to_string()]);
        config.mcp_managed.insert("claude-code".to_string(), vec!["srv".to_string()]);
        // 库放一个技能,下发到 opencode
        let skill_dir = sync::skills::library_dir(home.path()).join("myskill");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(skill_dir.join("SKILL.md"), "---\nname: myskill\n---\n").unwrap();
        sync::skills::apply_agent(home.path(), "opencode", &[]).unwrap();
        config.skills_managed.insert("opencode".to_string(), vec!["myskill".to_string()]);
        // 记忆库写入并下发到 hermes
        sync::memory::write_library(home.path(), "team memo\n").unwrap();
        sync::memory::apply_agent(home.path(), "hermes", &[]).unwrap();
        config.memory_managed.insert("hermes".to_string(), vec!["block".to_string()]);

        let all = agent_sync_overview_at(home.path(), &config);
        assert_eq!(all.len(), 15); // 注册表全覆盖
        let codex = overview_of(&all, "codex");
        assert_eq!((codex.providers[0].name.as_str(), codex.providers[0].state.as_str()), ("OA Relay", "synced"));
        assert!(codex.provider_config_path.ends_with("config.toml"));
        let cc = overview_of(&all, "claude-code");
        assert_eq!((cc.mcp[0].name.as_str(), cc.mcp[0].state.as_str()), ("srv", "synced"));
        // claude-code 未绑定服务商 → 不管理即不看,列表为空(不出现)
        assert!(cc.providers.is_empty());
        // skills:opencode 已建链 → synced;claude-code 未下发 → unsynced;
        // codex 不支持技能
        let oc = overview_of(&all, "opencode");
        assert!(oc.skills_supported);
        assert!(oc.skills_config_path.ends_with("skills"), "{}", oc.skills_config_path);
        assert_eq!((oc.skills[0].name.as_str(), oc.skills[0].state.as_str()), ("myskill", "synced"));
        assert_eq!((cc.skills[0].name.as_str(), cc.skills[0].state.as_str()), ("myskill", "unsynced"));
        assert!(!codex.skills_supported && codex.skills.is_empty());
        // memory:hermes 已注入 → synced;codex 支持但未下发 → unsynced;
        // codebuddy 不支持
        let hermes = overview_of(&all, "hermes");
        assert!(hermes.memory_supported);
        assert!(hermes.memory_config_path.ends_with("MEMORY.md"), "{}", hermes.memory_config_path);
        assert_eq!((hermes.memory[0].name.as_str(), hermes.memory[0].state.as_str()), ("memory", "synced"));
        assert_eq!(codex.memory[0].state, "unsynced");
        assert!(!overview_of(&all, "codebuddy").memory_supported);
        // 混合支持:cursor-agent 服务商侧 unsupported、MCP 侧 supported
        let cursor = overview_of(&all, "cursor-agent");
        assert!(!cursor.provider_supported && cursor.mcp_supported);
        assert!(cursor.providers.is_empty());
        // 全 unsupported 占位:qodercli 列表空、无错误
        let qoder = overview_of(&all, "qodercli");
        assert!(!qoder.provider_supported && !qoder.mcp_supported && !qoder.skills_supported);
        assert!(qoder.providers.is_empty() && qoder.mcp.is_empty() && qoder.skills.is_empty());
        assert!(qoder.provider_error.is_none() && qoder.mcp_error.is_none());

        // spec 改动 → outdated(曾下发过);MCP desired 清空但 managed 记着 →
        // removing;库删技能但 managed 记着 → skills removing
        config.providers[0].openai_base_url = "https://api2.oa.example.com/v1".to_string();
        config.mcp_servers.clear();
        sync::skills::remove_from_library(home.path(), "myskill").unwrap();
        let all = agent_sync_overview_at(home.path(), &config);
        let codex = overview_of(&all, "codex");
        assert_eq!(codex.providers[0].state, "outdated");
        let cc = overview_of(&all, "claude-code");
        assert_eq!((cc.mcp[0].name.as_str(), cc.mcp[0].state.as_str()), ("srv", "removing"));
        let oc = overview_of(&all, "opencode");
        assert_eq!((oc.skills[0].name.as_str(), oc.skills[0].state.as_str()), ("myskill", "removing"));

        // 服务商侧 plan 失败 → provider_error,同 agent 的 mcp 侧不受影响。
        // claude-code 两侧文件独立:provider 读 .claude/settings.json,
        // MCP 读 .claude.json —— 只写坏前者。
        fs::create_dir_all(home.path().join(".claude")).unwrap();
        fs::write(home.path().join(".claude").join("settings.json"), "{ broken").unwrap();
        // 绑上服务商让 claude-code 走 plan 的文件读取路径(未绑定 = 不看文件)
        config.agent_providers.insert("claude-code".to_string(), "p-oa".to_string());
        let all = agent_sync_overview_at(home.path(), &config);
        let cc = overview_of(&all, "claude-code");
        assert!(
            cc.provider_error.as_deref().unwrap_or_default().contains("parse"),
            "{:?}",
            cc.provider_error
        );
        assert!(cc.providers.is_empty());
        assert!(cc.mcp_error.is_none());
        assert_eq!((cc.mcp[0].name.as_str(), cc.mcp[0].state.as_str()), ("srv", "removing"));
    }

    // ---- agent_provider_bind:绑定即生效 / 解绑只删管理键 ----

    fn bind_home_with(providers: Vec<ProviderSpec>) -> TempHome {
        let home = TempHome::new();
        let mut c = Config::default();
        c.providers = providers;
        crate::commands::config::save_config(home.path(), &c).unwrap();
        home
    }

    fn claude_env(home: &Path) -> serde_json::Map<String, serde_json::Value> {
        let p = home.join(".claude").join("settings.json");
        let doc: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(p).unwrap()).unwrap();
        doc.get("env").and_then(|e| e.as_object()).cloned().unwrap_or_default()
    }

    #[test]
    fn bind_writes_config_and_persists_binding() {
        let home = bind_home_with(vec![pspec(
            "p-anth", "Anthro Relay", "https://relay.example.com/anthropic", "",
        )]);
        let r = agent_provider_bind_at(home.path(), "claude-code", Some("p-anth".to_string())).unwrap();
        assert!(r.ok, "{:?}", r.error);

        let env = claude_env(home.path());
        assert_eq!(
            env.get("ANTHROPIC_BASE_URL").and_then(|v| v.as_str()),
            Some("https://relay.example.com/anthropic")
        );
        let cfg = load_config(home.path()).unwrap();
        assert_eq!(cfg.agent_providers.get("claude-code").map(String::as_str), Some("p-anth"));
        assert_eq!(cfg.providers_managed.get("claude-code"), Some(&vec!["env".to_string()]));
    }

    // ---- reconcile_bindings_at:启动对账,只安全自愈、不覆盖手改 ----

    #[test]
    fn reconcile_noop_without_drift_and_does_not_clobber_hand_edits() {
        let home = bind_home_with(vec![pspec(
            "p1", "One", "https://one.example.com/anthropic", "",
        )]);
        agent_provider_bind_at(home.path(), "claude-code", Some("p1".to_string())).unwrap();

        // 无漂移 → 零结果 = 零写入(不产生备份)
        assert!(reconcile_bindings_at(home.path()).is_empty());

        // 手改我们管理的键(update 漂移)→ 对账默认不覆盖(信任优先)
        let p = home.path().join(".claude").join("settings.json");
        let text = fs::read_to_string(&p)
            .unwrap()
            .replace("one.example.com", "evil.example.com");
        fs::write(&p, text).unwrap();
        assert!(
            reconcile_bindings_at(home.path()).is_empty(),
            "reconcile must NOT auto-clobber hand edits to managed keys"
        );
        let env = claude_env(home.path());
        assert_eq!(
            env.get("ANTHROPIC_BASE_URL").and_then(|v| v.as_str()),
            Some("https://evil.example.com/anthropic"),
            "hand edit preserved until explicit resync"
        );

        // 用户显式 resync 才愈合
        let r = agent_provider_resync_at(home.path(), "claude-code").unwrap();
        assert!(r.ok, "{:?}", r.error);
        let env = claude_env(home.path());
        assert_eq!(
            env.get("ANTHROPIC_BASE_URL").and_then(|v| v.as_str()),
            Some("https://one.example.com/anthropic")
        );
    }

    #[test]
    fn reconcile_auto_heals_only_when_managed_keys_missing() {
        let home = bind_home_with(vec![pspec(
            "p1", "One", "https://one.example.com/anthropic", "",
        )]);
        agent_provider_bind_at(home.path(), "claude-code", Some("p1".to_string())).unwrap();

        // 把我们写的键整个删掉(文件被重置/其它工具清空)→ 纯 add,安全自愈
        let p = home.path().join(".claude").join("settings.json");
        fs::write(&p, r#"{"env":{}}"#).unwrap();
        let results = reconcile_bindings_at(home.path());
        assert_eq!(results.len(), 1);
        assert!(results[0].ok, "{:?}", results[0].error);
        let env = claude_env(home.path());
        assert_eq!(
            env.get("ANTHROPIC_BASE_URL").and_then(|v| v.as_str()),
            Some("https://one.example.com/anthropic")
        );
    }

    #[test]
    fn reconcile_skips_disabled_provider_even_when_drifted() {
        let home = bind_home_with(vec![pspec(
            "p1", "One", "https://one.example.com/anthropic", "",
        )]);
        agent_provider_bind_at(home.path(), "claude-code", Some("p1".to_string())).unwrap();
        let mut cfg = load_config(home.path()).unwrap();
        cfg.providers[0].enabled = false;
        crate::commands::config::save_config(home.path(), &cfg).unwrap();
        let p = home.path().join(".claude").join("settings.json");
        fs::write(&p, r#"{"env":{}}"#).unwrap();
        // 禁用的服务商不代用户做主:Agents 页有「请重选」提示
        assert!(reconcile_bindings_at(home.path()).is_empty());
    }

    #[test]
    fn bind_switch_replaces_previous_provider() {
        let home = bind_home_with(vec![
            pspec("p1", "One", "https://one.example.com/anthropic", ""),
            pspec("p2", "Two", "https://two.example.com/anthropic", ""),
        ]);
        agent_provider_bind_at(home.path(), "claude-code", Some("p1".to_string())).unwrap();
        agent_provider_bind_at(home.path(), "claude-code", Some("p2".to_string())).unwrap();
        let env = claude_env(home.path());
        assert_eq!(
            env.get("ANTHROPIC_BASE_URL").and_then(|v| v.as_str()),
            Some("https://two.example.com/anthropic")
        );
        let cfg = load_config(home.path()).unwrap();
        assert_eq!(cfg.agent_providers.get("claude-code").map(String::as_str), Some("p2"));
    }

    #[test]
    fn unbind_removes_only_managed_keys_and_binding() {
        let home = bind_home_with(vec![pspec(
            "p-anth", "Anthro Relay", "https://relay.example.com/anthropic", "",
        )]);
        // 用户自有键:解绑后必须原样保留
        fs::create_dir_all(home.path().join(".claude")).unwrap();
        fs::write(
            home.path().join(".claude").join("settings.json"),
            r#"{"env":{"MY_OWN":"keep"},"theme":"dark"}"#,
        ).unwrap();

        agent_provider_bind_at(home.path(), "claude-code", Some("p-anth".to_string())).unwrap();
        let r = agent_provider_bind_at(home.path(), "claude-code", None).unwrap();
        assert!(r.ok, "{:?}", r.error);

        let env = claude_env(home.path());
        assert!(env.get("ANTHROPIC_BASE_URL").is_none());
        assert_eq!(env.get("MY_OWN").and_then(|v| v.as_str()), Some("keep"));
        let cfg = load_config(home.path()).unwrap();
        assert!(!cfg.agent_providers.contains_key("claude-code"));
        assert!(!cfg.providers_managed.contains_key("claude-code"));
    }

    #[test]
    fn bind_rejects_incompatible_disabled_or_unknown() {
        // codex 只认 OpenAI 槽 → 绑只有 Anthropic 端点的服务商必须报错且不落盘
        let mut disabled = pspec("p-off", "Off", "https://off.example.com/anthropic", "");
        disabled.enabled = false;
        let home = bind_home_with(vec![
            pspec("p-anth", "Anthro Relay", "https://relay.example.com/anthropic", ""),
            disabled,
        ]);

        assert!(agent_provider_bind_at(home.path(), "codex", Some("p-anth".to_string())).is_err());
        assert!(agent_provider_bind_at(home.path(), "claude-code", Some("p-off".to_string())).is_err());
        assert!(agent_provider_bind_at(home.path(), "claude-code", Some("nope".to_string())).is_err());
        assert!(agent_provider_bind_at(home.path(), "not-an-agent", Some("p-anth".to_string())).is_err());

        let cfg = load_config(home.path()).unwrap();
        assert!(cfg.agent_providers.is_empty());
        assert!(!home.path().join(".codex").exists());
    }

    // ---- fallback 链端到端(agent_fallbacks_set_at)----

    /// 造一个带两家服务商、primary 已绑定到 hermes 的 home。返回 (home, p1, p2)。
    fn fallback_home() -> (TempHome, ProviderSpec, ProviderSpec) {
        let home = TempHome::new();
        let p1 = pspec("p1", "MiniMax", "https://api.minimaxi.com/anthropic", "");
        let p2 = pspec("p2", "DeepSeek", "", "https://api.deepseek.com/v1");
        let mut config = Config::default();
        config.providers = vec![p1.clone(), p2.clone()];
        save_config(home.path(), &config).unwrap();
        // primary 绑定到 hermes
        agent_provider_bind_at(home.path(), "hermes", Some("p1".to_string())).unwrap();
        (home, p1, p2)
    }

    #[test]
    fn fallback_set_writes_chain_and_persists_and_clears() {
        let (home, _p1, _p2) = fallback_home();
        // 设 fallback = [p2]
        let r = agent_fallbacks_set_at(home.path(), "hermes", vec!["p2".to_string()]).unwrap();
        assert!(r.ok, "{:?}", r.error);

        // config.json 落盘了 agent_fallbacks + providers_fallback_managed
        let cfg = load_config(home.path()).unwrap();
        assert_eq!(cfg.agent_fallbacks.get("hermes").map(|v| v.clone()), Some(vec!["p2".to_string()]));
        assert_eq!(
            cfg.providers_fallback_managed.get("hermes").map(|v| v.clone()),
            Some(vec!["DeepSeek".to_string()])
        );

        // hermes config.yaml:fallback_providers 链 + 两家 custom_providers 条目都在
        let yaml = std::fs::read_to_string(home.path().join(".hermes").join("config.yaml")).unwrap();
        assert!(yaml.contains("fallback_providers:"), "{}", yaml);
        assert!(yaml.contains("provider: custom:DeepSeek"), "{}", yaml);
        assert!(yaml.contains("name: MiniMax"), "{}", yaml);
        assert!(yaml.contains("name: DeepSeek"), "{}", yaml);
        // primary 未被碰
        assert!(yaml.contains("provider: custom:MiniMax"), "{}", yaml);

        // 清空 fallback 链 → 键删除、条目清掉、config.json 收回
        agent_fallbacks_set_at(home.path(), "hermes", vec![]).unwrap();
        let yaml2 = std::fs::read_to_string(home.path().join(".hermes").join("config.yaml")).unwrap();
        assert!(!yaml2.contains("fallback_providers"), "{}", yaml2);
        assert!(!yaml2.contains("name: DeepSeek"), "cleared fallback entry removed");
        assert!(yaml2.contains("name: MiniMax"), "primary intact after clear");
        let cfg2 = load_config(home.path()).unwrap();
        assert!(!cfg2.agent_fallbacks.contains_key("hermes"));
        assert!(!cfg2.providers_fallback_managed.contains_key("hermes"));
    }

    #[test]
    fn fallback_rejects_unsupported_agent_and_undeployable_provider() {
        let (home, _p1, _p2) = fallback_home();
        // claude-code 不支持 fallback
        let err = agent_fallbacks_set_at(home.path(), "claude-code", vec!["p2".to_string()]).unwrap_err();
        assert!(err.contains("does not support a fallback"), "{}", err);
        // 无端点的服务商不可作 fallback
        let mut config = load_config(home.path()).unwrap();
        config.providers.push(pspec("p3", "NoEP", "", ""));
        save_config(home.path(), &config).unwrap();
        let err = agent_fallbacks_set_at(home.path(), "hermes", vec!["p3".to_string()]).unwrap_err();
        assert!(err.contains("cannot be a fallback"), "{}", err);
    }

    // ---- adopt:agent → ClawBox 领养 ----

    #[test]
    fn adopt_creates_provider_from_agent_config_then_updates_on_readopt() {
        let home = TempHome::new();
        // hermes 配置里有个手建的 MiniMax 条目,ClawBox 完全不知道
        let hcfg = home.path().join(".hermes").join("config.yaml");
        fs::create_dir_all(hcfg.parent().unwrap()).unwrap();
        fs::write(
            &hcfg,
            "model:\n  provider: custom:MiniMax\n  default: MiniMax-M3\ncustom_providers:\n  - name: MiniMax\n    base_url: https://api.minimaxi.com/anthropic\n    api_key: sk-adopted\n    api_mode: anthropic_messages\n    model: MiniMax-M3\n",
        )
        .unwrap();
        save_config(home.path(), &Config::default()).unwrap();

        let r = agent_provider_adopt_at(home.path(), "hermes").unwrap();
        assert_eq!(r.provider_name, "MiniMax");
        assert!(r.created, "first adopt should create a new provider");

        let cfg = load_config(home.path()).unwrap();
        let p = cfg.providers.iter().find(|p| p.name == "MiniMax").unwrap();
        assert_eq!(p.api_key, "sk-adopted");
        assert_eq!(p.anthropic_base_url, "https://api.minimaxi.com/anthropic");
        assert_eq!(p.openai_base_url, "");
        assert_eq!(p.default_model, "MiniMax-M3");
        assert_eq!(cfg.agent_providers.get("hermes").map(String::as_str), Some(p.id.as_str()));

        // 再 adopt:key 变了 → 同名更新(created=false),不新建
        fs::write(&hcfg, fs::read_to_string(&hcfg).unwrap().replace("sk-adopted", "sk-updated")).unwrap();
        let r2 = agent_provider_adopt_at(home.path(), "hermes").unwrap();
        assert!(!r2.created, "re-adopt should update existing, not create");
        assert_eq!(r2.provider_id, r.provider_id, "same provider id on re-adopt");
        let cfg = load_config(home.path()).unwrap();
        assert_eq!(cfg.providers.iter().filter(|p| p.name == "MiniMax").count(), 1, "no duplicate");
        assert_eq!(
            cfg.providers.iter().find(|p| p.name == "MiniMax").unwrap().api_key,
            "sk-updated"
        );
    }
}
