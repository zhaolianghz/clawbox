# 服务商 per-agent 绑定（选中即生效）实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 每个 agent 独立选择服务商、选中即写入生效；移除全局星标默认与同步面板。

**Architecture:** `Config` 新增 `agent_providers: HashMap<agent_id, provider_id>` 绑定表；新命令 `agent_provider_bind` 复用现有 `ProviderAdapter::apply_one`（对每个 agent 只传绑定服务商的单元素列表，九个适配器零改动）；编辑服务商时自动重推到绑定它的 agent；`plan_all` 改为按绑定表逐 agent 计划，继续支撑 `agent_sync_overview` 漂移检测；旧的 `active_provider_id`、`sync_providers_plan/apply/status` 全部下线。

**Tech Stack:** Rust (Tauri 2 后端) + Svelte 5 runes (前端) + svelte-i18n。

**规格:** `docs/superpowers/specs/2026-07-23-provider-per-agent-binding-design.md`

## Global Constraints

- **安全铁律 1**：所有 agent 配置写入前必须走 `backup_target` 备份（`apply_one` 已内置，不得绕过）。
- **安全铁律 2**：`ChangeItem.detail`、错误信息、任何返回结构**绝不含 apiKey 明文**。
- **测试铁律 1**：所有测试用 `crate::sync::test_util::TempHome` 隔离 home，绝不触碰真实用户配置。
- **测试铁律 2**：测试**绝不调用 hermes 适配器的 `apply`**（它会执行真实 `hermes config set` CLI，写真实 `~/.hermes`）。测试一律用 claude-code / opencode / codex 适配器。
- 所有路径以显式 `home: &Path` 解析；每个 tauri 命令有 home 参数化的 `_at` 核心函数供测试。
- 注释风格：跟随现有代码的中文注释密度与口吻。
- 端点槽位偏好（与适配器一致，前端置灰要用）：claude-code=[anthropic]、codex=[openai]、codebuddy=[openai]、hermes=[anthropic,openai]、opencode=[openai,anthropic]、openclaw=[anthropic,openai]、kimi=[openai,anthropic]；cursor-agent / qodercli 不支持。
- `EnvSettingsProviderAdapter`（claude-code/codebuddy）的 `providers_managed` 标记是 `["env"]`（`ENV_MANAGED_MARK`），不是键名。
- 运行测试：`cd src-tauri && cargo test`；前端检查：`npm run check`（若无此脚本则 `npx svelte-check`）。

---

### Task 1: Config 数据模型 + 星标迁移 + 删服务商自动解绑

**Files:**
- Modify: `src-tauri/src/commands/config.rs`（`Config` 结构 ~L89-125、`load_config` ~L156、`config_providers_set` ~L237）

**Interfaces:**
- Produces: `Config.agent_providers: HashMap<String, String>`；`pub fn providers_set_at(home: &Path, providers: Vec<ProviderSpec>) -> Result<(), String>`（Task 3 会把返回值改成 `Vec<ApplyResult>`）。

- [ ] **Step 1: 写失败测试（迁移 + 解绑）**

在 `src-tauri/src/commands/config.rs` 的 `mod tests` 末尾追加（`spec` helper 已存在于该 mod）：

```rust
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
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cd src-tauri && cargo test migrates_star providers_set_drops migration_ignores 2>&1 | tail -20`
Expected: 编译失败——`agent_providers` 字段与 `providers_set_at` 不存在。

- [ ] **Step 3: 实现**

3a. `Config` 结构（L102-112 区域）：`active_provider_id` 上加 `skip_serializing_if`，其后新增 `agent_providers`：

```rust
    /// 废弃:旧「全局激活(默认)服务商」。仅为迁移保留可反序列化;
    /// load_config 迁移到 agent_providers 后清空,不再落盘。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_provider_id: Option<String>,
    /// agent_id -> 绑定的服务商 id。无条目 = ClawBox 不管理该 agent 的
    /// 服务商配置。绑定/切换/解绑经 agent_provider_bind,选中即写入生效。
    #[serde(default)]
    pub agent_providers: HashMap<String, String>,
```

3b. `load_config`（在 `normalize_provider_endpoints` 循环之后、`Ok(config)` 之前）：

```rust
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
```

3c. 重写 `config_providers_set`（替换 L235-248 的整个函数，含 doc 注释）：

```rust
/// Whole-table overwrite 的 home 参数化核心。被删除的服务商自动解除相关
/// agent 的绑定(只解绑,不写 agent 配置文件——不打断正在工作的 agent)。
pub fn providers_set_at(home: &Path, providers: Vec<ProviderSpec>) -> Result<(), String> {
    let mut config = load_config(home)?;
    config.providers = providers;
    let ids: std::collections::HashSet<String> =
        config.providers.iter().map(|p| p.id.clone()).collect();
    config.agent_providers.retain(|_, pid| ids.contains(pid));
    save_config(home, &config)
}

/// Whole-table overwrite: the frontend always sends the full provider list.
#[tauri::command]
pub async fn config_providers_set(providers: Vec<ProviderSpec>) -> Result<(), String> {
    providers_set_at(&real_home(), providers)
}
```

注意：`config_active_provider_get/set`、`active_provider_set_at` 本任务**不动**（Task 6 统一删除），它们仍引用 `active_provider_id` 字段，可编译。

- [ ] **Step 4: 跑测试确认通过**

Run: `cd src-tauri && cargo test 2>&1 | tail -5`
Expected: 全部 PASS（含既有测试；若既有测试 `active_provider_id` 相关断言因迁移逻辑失败——如 L395/L449 断言 load 后 active 保留——把这些断言改为符合新语义：load 后 `active_provider_id` 恒为 `None`、绑定按迁移规则生成。L337-338 的空配置断言不受影响）。

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands/config.rs
git commit -m "feat: Config 新增 agent_providers 绑定表 + 星标一次性迁移 + 删服务商自动解绑"
```

---

### Task 2: agent_provider_bind / agent_providers_get 命令

**Files:**
- Modify: `src-tauri/src/commands/sync.rs`（在「服务商同步状态」段之前新增命令段 + tests）
- Modify: `src-tauri/src/lib.rs`（注册两条新命令）

**Interfaces:**
- Consumes: Task 1 的 `Config.agent_providers`；现有 `providers::find_adapter` / `providers::apply_one` / `ProviderAdapter::deployed_names`。
- Produces: `pub fn agent_provider_bind_at(home: &Path, agent_id: &str, provider_id: Option<String>) -> Result<ApplyResult, String>`；tauri 命令 `agent_provider_bind(agent_id, provider_id)`、`agent_providers_get() -> HashMap<String, String>`。Task 3 复用 `agent_provider_bind_at` 做重推。

- [ ] **Step 1: 写失败测试**

在 `src-tauri/src/commands/sync.rs` 的 `mod tests` 中（`pspec` helper 已存在）追加。铁律：只用 claude-code / opencode / codex 适配器，绝不触发 hermes apply：

```rust
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
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cd src-tauri && cargo test bind_ unbind_ 2>&1 | tail -10`
Expected: 编译失败——`agent_provider_bind_at` 不存在。

- [ ] **Step 3: 实现**

在 `commands/sync.rs` 的「---- 服务商(模型)配置下发」段（`sync_providers_apply` 之后）追加：

```rust
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
    agent_provider_bind_at(&real_home(), &agent_id, provider_id)
}

/// 绑定表只读快照(agents 页选择器当前值 / providers 页「使用中」徽章)。
#[tauri::command]
pub async fn agent_providers_get() -> Result<HashMap<String, String>, String> {
    Ok(load_config(&real_home())?.agent_providers)
}
```

`lib.rs` 在 `commands::sync::agent_sync_overview,` 之后插入：

```rust
            commands::sync::agent_provider_bind,
            commands::sync::agent_providers_get,
```

- [ ] **Step 4: 跑测试确认通过**

Run: `cd src-tauri && cargo test 2>&1 | tail -5`
Expected: 全部 PASS。

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands/sync.rs src-tauri/src/lib.rs
git commit -m "feat: agent_provider_bind 绑定即生效命令(绑/切/解绑,复用适配器与备份)"
```

---

### Task 3: 编辑服务商自动重推到绑定 agent

**Files:**
- Modify: `src-tauri/src/commands/config.rs`（`providers_set_at` / `config_providers_set` 返回值升级）
- Test: 同文件 `mod tests`

**Interfaces:**
- Consumes: Task 2 的 `agent_provider_bind_at`。
- Produces: `providers_set_at(home, providers) -> Result<Vec<ApplyResult>, String>`；`config_providers_set` 命令返回 `Vec<ApplyResult>`（前端 Task 4 消费）。`ProviderSpec` 已有 `PartialEq` derive，可直接比较。

- [ ] **Step 1: 写失败测试**

在 `commands/config.rs` 的 `mod tests` 追加：

```rust
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
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cd src-tauri && cargo test providers_set_ 2>&1 | tail -10`
Expected: 编译失败——`providers_set_at` 返回类型不匹配（Task 1 里是 `Result<(), String>`）。

- [ ] **Step 3: 实现**

替换 Task 1 写的 `providers_set_at` / `config_providers_set`（文件头部需补 `use crate::sync::ApplyResult;` 及 `use std::collections::HashSet;`，按现有 use 风格放置）：

```rust
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
        .filter(|p| old.iter().find(|o| o.id == p.id) != Some(p))
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
```

注意 `old.iter().find(|o| o.id == p.id) != Some(p)` 需要 `&ProviderSpec: PartialEq`——直接写 `.map_or(true, |o| o != *p)` 形式更稳：

```rust
        .filter(|p| old.iter().find(|o| o.id == p.id).map_or(true, |o| o != *p))
```

同时更新 Task 1 的 `providers_set_drops_bindings_of_deleted_providers` 测试里 `providers_set_at(...).unwrap()` —— 返回类型变了但 `.unwrap()` 依旧成立，无需改动。

- [ ] **Step 4: 跑测试确认通过**

Run: `cd src-tauri && cargo test 2>&1 | tail -5`
Expected: 全部 PASS。

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands/config.rs
git commit -m "feat: 编辑服务商自动重推到绑定 agent(失败不回滚保存,逐条返回)"
```

---

### Task 4: 前端 API 层 + Agents 页服务商选择器

**Files:**
- Modify: `src/lib/api/providerSync.ts`（新增 bind/get；旧函数 Task 5 再删）
- Modify: `src/lib/api/config.ts`（`save_providers` 返回类型升级——先看文件再改）
- Modify: `src/routes/agents/+page.svelte`
- Modify: `src/lib/i18n/zh.json`、`src/lib/i18n/en.json`（新增 `agents.provider.*` 键）

**Interfaces:**
- Consumes: Task 2/3 的命令。
- Produces: `agent_provider_bind(agentId, providerId)` / `agent_providers_get()`（Task 5 的 providers 页也用 `agent_providers_get`）；`save_providers(): Promise<ApplyResult[]>`。

- [ ] **Step 1: providerSync.ts 追加 API**

在文件末尾追加：

```ts
/** 绑定/切换/解绑该 agent 的服务商(选中即写入生效;null = 解绑恢复原状) */
export function agent_provider_bind(
  agentId: string,
  providerId: string | null
): Promise<ApplyResult> {
  return invoke<ApplyResult>('agent_provider_bind', { agentId, providerId });
}

/** 绑定表快照:agent_id → provider_id */
export function agent_providers_get(): Promise<Record<string, string>> {
  return invoke<Record<string, string>>('agent_providers_get');
}
```

- [ ] **Step 2: config.ts 升级 save_providers 返回类型**

打开 `src/lib/api/config.ts`，找到 `save_providers`（调 `config_providers_set`），把返回类型从 `Promise<void>` 改为 `Promise<ApplyResult[]>`（`ApplyResult` 从 `./mcpSync` re-export，参照 providerSync.ts L2 的写法 `import type { ApplyResult } from './mcpSync'`）。`src/lib/stores/config.ts` 的 `persist` 相应返回 `Promise<ApplyResult[]>`，`addProvider/updateProvider/deleteProvider` 把 `persist` 的返回值透传（`return persist(prev);`，签名改为 `Promise<ApplyResult[]>`）。

- [ ] **Step 3: Agents 页加选择器**

`src/routes/agents/+page.svelte`：

3a. script 顶部追加 import 与状态：

```ts
import { providers, loadProviders } from '../../lib/stores/config';
import { agent_provider_bind, agent_providers_get } from '../../lib/api/providerSync';
import type { ModelProvider } from '../../lib/api/config';

// ---------- 服务商绑定(选中即生效) ----------
let bindings = $state<Record<string, string>>({});
let bindApplying = $state<Record<string, boolean>>({});
let bindErrors = $state<Record<string, string>>({});
let bindFlash = $state<Record<string, boolean>>({}); // 成功短暂高亮

// 各 agent 的端点槽位偏好(与 src-tauri/src/sync/providers.rs 各适配器一致;
// 不在表里的 agent 不支持服务商下发,不渲染选择器)
const AGENT_SLOTS: Record<string, ('anthropic' | 'openai')[]> = {
  'claude-code': ['anthropic'],
  codex: ['openai'],
  codebuddy: ['openai'],
  hermes: ['anthropic', 'openai'],
  opencode: ['openai', 'anthropic'],
  openclaw: ['anthropic', 'openai'],
  kimi: ['openai', 'anthropic'],
};

/** 该服务商能否下发给该 agent:任一偏好槽已配置端点,且有 API key */
function compatible(agentId: string, p: ModelProvider): boolean {
  const slots = AGENT_SLOTS[agentId];
  if (!slots || !p.apiKey) return false;
  return slots.some((s) => (s === 'anthropic' ? !!p.anthropicBaseUrl : !!p.openaiBaseUrl));
}

const enabledProviders = $derived($providers.filter((p) => p.enabled));

async function bindProvider(agentId: string, providerId: string) {
  const prev = bindings[agentId] ?? '';
  bindApplying = { ...bindApplying, [agentId]: true };
  bindErrors = { ...bindErrors, [agentId]: '' };
  try {
    await agent_provider_bind(agentId, providerId === '' ? null : providerId);
    if (providerId === '') {
      const { [agentId]: _, ...rest } = bindings;
      bindings = rest;
    } else {
      bindings = { ...bindings, [agentId]: providerId };
    }
    bindFlash = { ...bindFlash, [agentId]: true };
    setTimeout(() => (bindFlash = { ...bindFlash, [agentId]: false }), 2000);
    if (overview !== null) void loadOverview(true); // 漂移状态跟着刷新
  } catch (e) {
    bindErrors = { ...bindErrors, [agentId]: String(e) };
    bindings = prev === '' ? (({ [agentId]: _drop, ...rest }) => rest)(bindings) : { ...bindings, [agentId]: prev };
  } finally {
    bindApplying = { ...bindApplying, [agentId]: false };
  }
}
```

（注意上面 catch 里的回滚写法若 svelte-check 报怨，简化为：`bindings = { ...bindings }; if (prev) bindings[agentId] = prev; else delete bindings[agentId];` 前先复制。以 check 通过为准，语义 = 恢复 prev。）

3b. `onMount` 里追加加载：

```ts
onMount(async () => {
  await refresh();
  checkUpdates(false);
  void loadProviders();
  try {
    bindings = await agent_providers_get();
  } catch (e) {
    console.warn('agent_providers_get failed', e);
  }
});
```

3c. 卡片模板：在 `card-actions` div **之前**插入选择器行（仅 `AGENT_SLOTS` 里的 agent 渲染）：

```svelte
{#if AGENT_SLOTS[a.id]}
  <div class="provider-bind" class:flash={bindFlash[a.id]}>
    <span class="bind-label">{$_('agents.provider.label')}</span>
    <select
      class="bind-select"
      disabled={bindApplying[a.id]}
      value={bindings[a.id] ?? ''}
      onchange={(e) => bindProvider(a.id, e.currentTarget.value)}
    >
      <option value="">{$_('agents.provider.unmanaged')}</option>
      {#each enabledProviders as p (p.id)}
        <option value={p.id} disabled={!compatible(a.id, p)}>
          {p.name}{compatible(a.id, p) ? '' : ` (${$_('agents.provider.incompatible')})`}
        </option>
      {/each}
      {#if bindings[a.id] && !enabledProviders.some((p) => p.id === bindings[a.id])}
        <!-- 绑定的服务商已被禁用:保留一个占位项让选择器如实回显,提示重选 -->
        <option value={bindings[a.id]} disabled>{$_('agents.provider.stale')}</option>
      {/if}
    </select>
    {#if bindApplying[a.id]}<span class="spinner small"></span>{/if}
  </div>
  {#if bindErrors[a.id]}
    <pre class="install-error">{bindErrors[a.id]}</pre>
  {/if}
{/if}
```

3d. style 块追加：

```css
.provider-bind { display: flex; align-items: center; gap: 0.5rem; }
.bind-label { font-size: 0.75rem; opacity: 0.6; white-space: nowrap; }
.bind-select {
  flex: 1; min-width: 0; padding: 0.25rem 0.5rem; border-radius: 6px;
  border: 1px solid rgba(255,255,255,0.15); background: transparent;
  color: inherit; font-size: 0.75rem;
}
.bind-select:disabled { opacity: 0.5; }
.provider-bind.flash .bind-select { border-color: #4ade80; transition: border-color 0.3s; }
```

- [ ] **Step 4: i18n 键**

`zh.json` 的 `agents` 节内追加：

```json
"provider": {
  "label": "服务商",
  "unmanaged": "不由 ClawBox 管理",
  "incompatible": "端点不兼容",
  "stale": "(已禁用的服务商,请重选)"
}
```

`en.json` 对应：

```json
"provider": {
  "label": "Provider",
  "unmanaged": "Not managed by ClawBox",
  "incompatible": "endpoint mismatch",
  "stale": "(disabled provider — pick another)"
}
```

- [ ] **Step 5: 验证**

Run: `npm run check 2>&1 | tail -15`（无 check 脚本则 `npx svelte-check --threshold error`）
Expected: 无新增错误。再 `npm run tauri dev` 手动冒烟（可选，交给最终验证也行）。

- [ ] **Step 6: Commit**

```bash
git add src/lib/api/providerSync.ts src/lib/api/config.ts src/lib/stores/config.ts src/routes/agents/+page.svelte src/lib/i18n/zh.json src/lib/i18n/en.json
git commit -m "feat: Agents 页服务商选择器——选中即生效,不兼容项置灰"
```

---

### Task 5: Providers 页移除星标/同步面板,改「使用中」徽章 + 重推提示

**Files:**
- Modify: `src/routes/providers/+page.svelte`
- Modify: `src/lib/api/providerSync.ts`（删除旧 plan/apply/status 导出）
- Modify: `src/lib/i18n/zh.json`、`src/lib/i18n/en.json`

**Interfaces:**
- Consumes: Task 4 的 `agent_providers_get`、升级后的 store 动作返回 `ApplyResult[]`。

- [ ] **Step 1: 删除旧机制（script 部分）**

`src/routes/providers/+page.svelte`：

- 删 import：`sync_providers_plan, sync_providers_apply, sync_providers_status, config_active_provider_get, config_active_provider_set` 与类型 `AgentPlan, ProviderSyncStatus`（保留 `ApplyResult`、`ChangeItem` 若仍被引用，不被引用则一并删）。
- 删「激活(默认)服务商」整段（L26-56：`activeProviderId`、`refreshActive`、`toggleActive`、`activeMissing`）。
- 删「卡片同步状态标签」整段（L58-72：`cardStatus`、`cardStatusLoaded`、`refreshSyncStatus`）及所有 `refreshSyncStatus()` 调用点（L156、L411、onMount）。
- 删「同步到 Agent」整段状态与函数（L420-576：`SyncStage/syncStage/quickSync/plans/syncError/expanded/checked/batchApplying/rowApplying/rowError/rowSynced/rowBackup`、`realChanges/skipItems/rowStatus/selectable/checkedCount/selectablePlans/allPlansPicked/toggleAllPlans/startSync/recordResult/applyOne/applyChecked/toggleExpand/closeSync/unchangedCount`、`UNSUPPORTED_REASON_IDS`）。**保留** `FALLBACK_LABELS/agentLabels/agentLabel`（「使用中」徽章 title 要用）。
- `svelte:window` Escape 处理删掉 `syncStage` 分支；`startImport` 的守卫 `|| syncStage !== 'closed'` 删掉；头部按钮区删掉「同步到 Agent」按钮及 import 按钮上的 `syncStage` 条件。
- `saveProvider`（L404-412）改为消费重推结果：

```ts
      let repushed: ApplyResult[] = [];
      if (editingId === null) {
        repushed = await addProvider({ id: crypto.randomUUID(), ...data });
      } else {
        repushed = await updateProvider(editingId, data);
      }
      closeEditor();
      reportRepush(repushed);
```

新增状态与函数（放在 pageError 附近）：

```ts
  // 编辑保存后的自动重推结果提示(成功 N 家 / 失败列出)
  let repushNote = $state('');
  let repushFailed = $state<ApplyResult[]>([]);

  function reportRepush(results: ApplyResult[]) {
    repushFailed = results.filter((r) => !r.ok);
    const okCount = results.length - repushFailed.length;
    repushNote = okCount > 0 ? $_('providers.repushOk', { values: { count: okCount } }) : '';
    if (repushNote) setTimeout(() => (repushNote = ''), 5000);
  }
```

`removeProvider` / `toggleEnabled` / `applyImport` 里对 `updateProvider/deleteProvider/addProvider` 的调用点：`deleteProvider` 返回值忽略（删除不重推）；`toggleEnabled` 与 `applyImport` 的结果传给 `reportRepush`（禁用已绑定服务商会重推失败 → 正好在失败列表里提示）。

- [ ] **Step 2: 删除旧机制（模板 + 样式）**

- 删 quick-sync-bar 块（L757-765）与 sync-panel 块（L768-908）。
- 卡片：删 `class:default-active`、`default-badge` 块（L1024-1026）、星标按钮块（L1081-1089）。
- 卡片 meta 的同步状态徽章（L1050-1072）整块替换为「使用中」徽章：

```svelte
          {#if configured && (usageByProvider[configured.id]?.length ?? 0) > 0}
            <span
              class="sync-badge synced"
              title={usageByProvider[configured.id].map(agentLabel).join(', ')}
            >{$_('providers.usedBy', { values: { count: usageByProvider[configured.id].length } })}</span>
          {/if}
```

script 里对应新增（onMount 拉取；绑定在 agents 页变更后本页重新挂载时自然刷新）：

```ts
  import { agent_providers_get } from '$lib/api/providerSync';

  // agent_id → provider_id 绑定表 → 反查每家服务商被哪些 agent 使用
  let agentBindings = $state<Record<string, string>>({});
  const usageByProvider = $derived.by(() => {
    const m: Record<string, string[]> = {};
    for (const [agentId, pid] of Object.entries(agentBindings)) (m[pid] ??= []).push(agentId);
    return m;
  });
```

onMount 追加 `agentBindings = await agent_providers_get().catch(() => ({}));`（保留原有 agents_list 拉标签逻辑）。

- pageError 块下方渲染重推提示：

```svelte
  {#if repushNote}
    <div class="quick-sync-bar"><span class="qs-hint">{repushNote}</span></div>
  {/if}
  {#if repushFailed.length > 0}
    <div class="quick-sync-bar">
      <span class="qs-hint">{$_('providers.repushFail', { values: { agents: repushFailed.map((r) => agentLabel(r.agent_id)).join(', ') } })}</span>
      <button class="qs-close" onclick={() => (repushFailed = [])} aria-label={$_('providers.cancel')}>✕</button>
    </div>
  {/if}
```

- 样式清理：**先 grep 再删**——`.sync-panel` 被 cc-switch 导入面板复用（L578 注释），必须保留；`.quick-sync-bar/.qs-hint/.qs-close` 重推提示复用，保留；删除只剩 sync 面板引用的类（`.plan-list/.plan-item/.plan-row/.plan-head/.plan-info/.plan-title-line/.select-all-plans/.selectable-count/.row-check/.change-list/.change/.change-action/.change-name/.change-detail/.chevron/.sync-one/.unchanged-note/.skip-reason/.star/.default-badge` 等）。对每个候选类名执行 `grep -c "类名" src/routes/providers/+page.svelte`，仅当模板中已无引用才删。`.sync-badge` 保留（使用中徽章还在用）。

- [ ] **Step 3: providerSync.ts 删旧导出**

删除 `sync_providers_plan`、`sync_providers_apply`、`sync_providers_status`、`ProviderSyncStatus`。保留 `AgentPlan/ApplyResult/ChangeItem` re-export（mcp 页与 overview 仍用）、`SyncedItem/AgentSyncOverview/agent_sync_overview`、Task 4 新增的两个函数。`config_active_provider_get/set` 一并删除。

- [ ] **Step 4: i18n 清理与新增**

`zh.json` / `en.json` 同步操作：

- 删除键：`providers.setDefault`、`providers.unsetDefault`、`providers.defaultBadge`、`providers.syncToAgents`、`providers.syncedTo`、`providers.pendingTo`、`providers.sync.*` 整节、`quickSync.*` 整节。
- 新增键（zh）：`providers.usedBy = "{count} 个 agent 使用中"`、`providers.repushOk = "已重新下发到 {count} 个 agent"`、`providers.repushFail = "自动下发失败: {agents}(在 Agent 管理页重选一次即可重试)"`。
- 新增键（en）：`providers.usedBy = "Used by {count} agents"`、`providers.repushOk = "Re-deployed to {count} agents"`、`providers.repushFail = "Auto-deploy failed: {agents} (re-select on the Agents page to retry)"`。
- 删完后全局 grep 确认无残留引用：`grep -rn "providers.sync\.\|quickSync\.\|setDefault\|defaultBadge\|syncToAgents" src/`（应只剩 0 条；`providers.syncedTo/pendingTo` 同理）。

- [ ] **Step 5: 验证**

Run: `npm run check 2>&1 | tail -15`
Expected: 无错误（尤其确认删掉的符号无残留引用）。

- [ ] **Step 6: Commit**

```bash
git add src/routes/providers/+page.svelte src/lib/api/providerSync.ts src/lib/i18n/zh.json src/lib/i18n/en.json
git commit -m "feat: Providers 页下线星标与同步面板,改「N 个 agent 使用中」+ 保存自动重推提示"
```

---

### Task 6: 后端下线旧命令 + overview 改 per-agent 语义

**Files:**
- Modify: `src-tauri/src/sync/providers.rs`（`plan_all` 签名、`resolve_single_active` 文案、相关测试）
- Modify: `src-tauri/src/commands/sync.rs`（删 `sync_providers_plan/apply`、`sync_providers_status`、`providers_status_at`、`ProviderSyncStatus`、`SINGLE_ACTIVE_AGENTS`、`DEFAULT_MODEL_ITEMS` 及 status 相关测试；`agent_sync_overview_at` 改传绑定表）
- Modify: `src-tauri/src/commands/config.rs`（删 `config_active_provider_get/set`、`active_provider_set_at` 及其测试）
- Modify: `src-tauri/src/lib.rs`（注销 5 条旧命令）

**Interfaces:**
- Produces: `providers::plan_all(home, providers, bindings: &HashMap<String, String>, managed) -> Vec<AgentPlan>`——未绑定的 agent `changes = []`（不管理即不看）；绑定的 agent 按单元素列表 plan。

- [ ] **Step 1: 写失败测试（新 plan_all 语义）**

在 `src-tauri/src/sync/providers.rs` 的 `mod tests` 追加：

```rust
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
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cd src-tauri && cargo test plan_all_uses plan_all_dangling 2>&1 | tail -10`
Expected: 编译失败——`plan_all` 第三参类型不符。

- [ ] **Step 3: 实现**

3a. `providers.rs` 重写 `plan_all`（L1660-1701 整体替换）：

```rust
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
```

3b. `resolve_single_active` 的 skip 文案（L93）更新：

```rust
            reason: "No provider bound (pick one on the Agents page)".to_string(),
```

先 `grep -rn "star one on the Providers page" src-tauri/` 找测试断言一并改。

3c. `commands/sync.rs`：
- 删 `sync_providers_plan`、`sync_providers_apply`（L87-136）。
- 删 `ProviderSyncStatus` 结构、`SINGLE_ACTIVE_AGENTS`、`DEFAULT_MODEL_ITEMS`、`providers_status_at`、`sync_providers_status`（L138-265）。
- 删 `mod tests` 里 status 归并相关测试（`status_multi_provider_synced_then_pending`、`status_single_active_synced_pending_and_inactive_untouched` 及只被它们使用的 helper `status_of` / `apply_provider_adapter`；`pspec` 保留——Task 2 的 bind 测试在用）。
- `agent_sync_overview_at`（L570-581）改：

```rust
    let provider_plans = providers::plan_all(
        home,
        &config.providers,
        &config.agent_providers,
        &config.providers_managed,
    );
```

3d. `commands/config.rs`：删 `config_active_provider_get`、`active_provider_set_at`、`config_active_provider_set`（L250-270）及测试里对它们的引用（L374-395、L449-455 一带的用例——涉及 `active_provider_set_at` 的测试整个删除；仅断言字段默认值的保留并按需调整）。

3e. `lib.rs`：删除注册行 `config_active_provider_get`、`config_active_provider_set`、`sync_providers_plan`、`sync_providers_apply`、`sync_providers_status`。

- [ ] **Step 4: 全量测试**

Run: `cd src-tauri && cargo test 2>&1 | tail -8`
Expected: 全部 PASS，无 dead_code warning（有则把只剩测试使用的 helper 挪进 `#[cfg(test)]` 或删除）。`providers.rs` 里原有引用旧 `plan_all(…, active_id, …)` 签名的测试按新签名改写：把 `Some("p-x")` 换成 `HashMap::from([(agent.to_string(), "p-x".to_string())])` 形式，语义等价的绑定表。

- [ ] **Step 5: 前端残留检查**

Run: `grep -rn "sync_providers_\|config_active_provider" src/ && echo FOUND || echo CLEAN`
Expected: `CLEAN`（Task 5 已删干净；发现残留则删除）。

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/sync/providers.rs src-tauri/src/commands/sync.rs src-tauri/src/commands/config.rs src-tauri/src/lib.rs
git commit -m "refactor: plan_all 改 per-agent 绑定语义;下线星标与 sync_providers_* 旧命令"
```

---

### Task 7: 文档更新 + 全量验证

**Files:**
- Modify: `README.md`、`README.zh.md`（Providers 功能描述）
- Modify: `CHANGELOG.md`
- Modify: `docs/TRANSPARENCY.md`、`docs/TRANSPARENCY.zh.md`（如有星标/同步面板表述）

- [ ] **Step 1: 更新文档**

- `README.md` Features 表 Providers 行：`Sync the active provider to all agents in one click.` → `Pick a provider per agent — selection applies instantly, edits auto-redeploy.`（README.zh.md 对应中文：`每个 agent 独立选择服务商，选中即生效，编辑自动重新下发。`）
- README 顶部 tagline `sync to every agent with one click` 若涉及服务商语义，酌情微调（MCP 等仍是一键同步，可保留）。
- `grep -n "默认服务商\|active provider\|星标\|star" docs/TRANSPARENCY*.md README*.md` 逐条核对更新。
- `CHANGELOG.md` 追加未发布节条目：`- 服务商改为 per-agent 绑定:在 Agent 管理页为每个 agent 独立选择服务商,选中即生效;编辑服务商自动重新下发;移除全局默认(星标)与同步面板。旧星标配置自动迁移。`

- [ ] **Step 2: 全量验证**

```bash
cd src-tauri && cargo test 2>&1 | tail -5 && cargo clippy 2>&1 | tail -5
cd .. && npm run check 2>&1 | tail -5 && npm run build 2>&1 | tail -5
```

Expected: 测试全过、clippy 无新警告、check/build 通过。

- [ ] **Step 3: 手动冒烟（npm run tauri dev）**

1. Providers 页配置两家服务商（一家仅 Anthropic 端点、一家仅 OpenAI）→ 卡片无星标、无同步按钮。
2. Agents 页:Claude Code 选择器里 OpenAI-only 服务商置灰;选 Anthropic 家 → spinner → 高亮,`~/.claude/settings.json` env 三键写入,`~/.clawbox/backups/` 有备份。
3. Providers 页刷新 → 该服务商显示「1 个 agent 使用中」。
4. 编辑该服务商端点保存 → 顶部提示「已重新下发到 1 个 agent」,settings.json 已更新。
5. Agents 页切到「不由 ClawBox 管理」→ env 三键被清、用户自有键保留。
6. 用旧版配置文件(含 active_provider_id + providers_managed)启动 → 绑定自动迁移,选择器显示旧默认服务商。

- [ ] **Step 4: Commit**

```bash
git add README.md README.zh.md CHANGELOG.md docs/TRANSPARENCY.md docs/TRANSPARENCY.zh.md
git commit -m "docs: per-agent 服务商绑定的 README/CHANGELOG/透明度文档更新"
```
