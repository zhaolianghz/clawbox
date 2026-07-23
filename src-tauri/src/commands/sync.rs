//! Tauri commands for MCP unified deployment. Thin wrappers: all logic
//! lives in `crate::sync` / `crate::commands::config` so it stays testable
//! against a tempdir home.

use crate::commands::config::{load_config, real_home, save_config, Config, McpServerSpec};
use crate::sync::{self, providers, AgentPlan, ApplyResult};
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
        assert_eq!(all.len(), 9); // 注册表全覆盖
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
}
