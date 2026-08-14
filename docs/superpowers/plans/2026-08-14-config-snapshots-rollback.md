# 配置快照与回滚实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 每次 ClawBox 改写 agent 配置文件前拍统一快照,用户可在 Agents 页浏览并一键恢复。

**Architecture:** 新建 `sync/snapshots.rs` 统一快照层(manifest + blobs),替换现有单文件 `backup_target`;四条 apply 线经 adapter 新增的 `touch_paths()` 声明触碰路径;恢复 = 还原文件 + 清对应维度 managed 记账(防启动对账把回滚当漂移自动愈合)。

**Tech Stack:** Rust(Tauri v2, serde, time), Svelte 5 + TypeScript, svelte-i18n。

**Spec:** `docs/superpowers/specs/2026-08-14-config-snapshots-rollback-design.md`(决策、存储格式、记账映射表以 spec 为准)

## Global Constraints

- 所有逻辑 home 参数化,TempHome 测试,绝不碰真实用户配置(项目铁律)
- CLI 型 agent(openclaw/hermes)`restorable: false`,恢复报错引导手动操作
- 每 agent 保留最近 20 份快照,固定常量
- `rel_path` 恢复时拒绝绝对路径与 `..` 组件
- en/zh i18n 文案同步
- 错误信息不含 API key

---

### Task 1: snapshots 核心 — 类型 / capture / list / prune

**Files:** Create `src-tauri/src/sync/snapshots.rs`;Modify `src-tauri/src/sync/mod.rs`(挂模块)

**Interfaces (Produces):**
- `pub const KEEP_PER_AGENT: usize = 20;`
- `pub struct SnapshotEntry { pub rel_path: String, pub kind: String, pub blob: Option<String>, pub target: Option<String>, pub size: Option<u64> }` (kind: "file"|"missing"|"symlink"|"dir")
- `pub struct SnapshotInfo { pub id: String, pub agent_id: String, pub scope: String, pub summary: String, pub restorable: bool, pub created_at: String, pub files: usize }`
- `pub fn snapshots_dir(home: &Path) -> PathBuf` → `~/.clawbox/snapshots`
- `pub fn capture(home: &Path, agent_id: &str, scope: &str, summary: &str, paths: &[PathBuf]) -> Result<SnapshotInfo, String>`
- `pub fn list(home: &Path, agent_id: &str) -> Vec<SnapshotInfo>`(时间倒序)

**Steps:**
- [x] 失败测试:capture 四种 entry(file/missing/symlink/dir)→ manifest.json 与 blobs 落盘正确;list 倒序;prune 超 20 份删最旧;同秒冲突 id 追加 `-N`;空 paths → restorable=false
- [x] 实现:id 生成 `yyyyMMdd-HHmmss[-N]-<scope>`;blob 编号 = entries 下标;dir 递归拷贝 `blobs/<i>/`;symlink 用 `symlink_metadata` 记目标(不跟随);prune 按 id 字典序(时间戳前缀保证可排序)
- [x] `cargo test snapshots` 通过
- [x] Commit: `feat(sync): 统一快照层 capture/list/prune`

### Task 2: restore — 还原 / 安全快照 / 记账清理 / 路径逃逸防护

**Files:** Modify `src-tauri/src/sync/snapshots.rs`

**Interfaces (Consumes/Produces):**
- `pub struct RestoreResult { pub restored: Vec<String>, pub cleared: Vec<String> }`
- `pub fn restore(home: &Path, agent_id: &str, snapshot_id: &str) -> Result<RestoreResult, String>`
- `pub fn restore_blob(home: &Path, agent_id: &str, snapshot_id: &str, rel_path: &str, target: &Path) -> Result<(), String>`(Task 3 的 validate_or_rollback 用:从快照恢复单个 entry 到任意目标)

**Steps:**
- [x] 失败测试:file 往返;missing → 删除现存;symlink 重建;dir 精确恢复(后加的多余子项被删);恢复前生成 pre-restore 安全快照;restorable=false → Err;manifest 手改为 `/abs` 或 `../x` → Err;scope→cleared 映射(provider/fallback/mcp/skills/memory,写 ClawBox config 后 reload 验证)
- [x] 实现:rel_path 规范化校验;file 经 `.clawbox-swap` 临时文件 + rename;清记账直接 load_config/save_config(scope 映射见 spec 表)
- [x] `cargo test snapshots` 通过
- [x] Commit: `feat(sync): 快照 restore + 记账清理 + 逃逸防护`

### Task 3: 接线四条 apply 线,替换 backup_target

**Files:** Modify `src-tauri/src/sync/mod.rs`(ConfigAdapter::touch_paths 默认方法、apply_one、删 backup_target、ApplyResult.backup_path→snapshot_id)、`providers.rs`(ProviderAdapter::touch_paths、CodexAdapter 覆写 `[config.toml, auth.json, catalog]`、apply_one/apply_fallbacks_one/validate_or_rollback/rollback_config 改造)、`skills.rs`、`memory.rs`;前端 `src/lib/api/mcpSync.ts` 等类型字段。

**Interfaces:**
- `ConfigAdapter::touch_paths(&self, home: &Path) -> Vec<PathBuf>` 默认 `vec![self.config_path(home)]`;codex MCP adapter 覆写
- `ProviderAdapter::touch_paths` 同上
- skills `apply_one`:touch = `[agent_skills_dir]` 整目录单个 dir entry(实现偏差:子项清单无法在恢复时清掉 apply 新建的软链,整目录精确恢复语义更正确;目录不存在自然记 missing)
- `ApplyResult { snapshot_id: Option<String>, ... }`(backup_path 删除,前端 3 处引用同步改名)

**Steps:**
- [x] 旧 backup_target 测试删除,替换为:各 apply 站点产生快照(capture 后 `list` 可见、scope 正确);codex provider apply 快照含 3 条 entry;validate_or_rollback 失败路径从快照 blob 恢复(现有 rollback 测试改造)
- [x] 实现 + `cargo test` 全量通过(现有一处 `.clawbox/backups` 断言删除)
- [x] 前端 grep `backup_path` 改 `snapshotId`(mcp/capabilities 页展示文案改「已快照」)
- [x] Commit: `feat(sync): 四条 apply 线接入统一快照,移除 backup_target`

### Task 4: Tauri 命令

**Files:** Create `src-tauri/src/commands/snapshots.rs`;Modify `src-tauri/src/commands/mod.rs`、`src-tauri/src/lib.rs`(invoke_handler)

**Interfaces:**
- `#[tauri::command] pub async fn snapshots_list(agent_id: Option<String>) -> Result<Vec<SnapshotInfo>, String>`(None = 全 agent,倒序)
- `#[tauri::command] pub async fn snapshots_restore(agent_id: String, snapshot_id: String) -> Result<RestoreResult, String>`(薄封装,持 CONFIG_LOCK)

**Steps:**
- [x] 实现 + 注册;`cargo test` 通过
- [x] Commit: `feat(commands): snapshots_list / snapshots_restore 命令`

### Task 5: 前端 — API 封装 + Agents 页快照弹层 + i18n

**Files:** Create `src/lib/api/snapshots.ts`;Modify `src/routes/agents/+page.svelte`(每 agent「快照历史」入口 + 弹层列表 + 恢复确认)、`src/lib/i18n/{en,zh}.json`

**Steps:**
- [x] snapshots.ts 类型化 invoke 封装
- [x] 弹层:时间/scope 徽章/摘要/文件数;restorable=false 灰化标「不可自动恢复」;恢复确认文案明示 skills 目录精确恢复语义与记账清除后果;成功 toast 展示 restored/cleared
- [x] en/zh 文案;`npm run check` 通过
- [x] Commit: `feat(ui): Agents 页快照历史与恢复`

### Task 6: 全量验证

**Steps:**
- [x] `cd src-tauri && cargo test` 全绿
- [x] `npm run check` 全绿
- [x] ROADMAP.md 条目 4 标注已实现
- [x] Commit: `chore: 快照回滚收尾`
