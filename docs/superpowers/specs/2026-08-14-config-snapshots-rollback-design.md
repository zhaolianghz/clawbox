# 配置快照与回滚(Config Snapshots & Rollback)设计

> 日期:2026-08-14
> 状态:已批准(用户拍板 4 个决策点后开工)
> 来源:docs/ROADMAP.md 条目 4

## 问题

ClawBox 直接改写用户的 agent 配置文件,这是采纳的最大心理门槛。现状只有一套
**事务性备份**(`sync::backup_target` + providers 的 `validate_or_rollback`):
apply 前把单文件拷到 `~/.clawbox/backups/<时间戳>/`,写入后校验失败立即自动回滚。
它是防程序 bug 的,用户看不见、不能主动恢复,且有三个盲区:

1. **多文件 agent**:Codex 实际写 `config.toml` + `auth.json` + catalog,只备份了
   `config_path` 一个
2. **skills**:技能目录是目录,`backup_target` 对非文件返回 None——完全没备份
3. **CLI 型 agent**(openclaw/hermes):走 CLI 下发,无文件可备

## 目标

每次 ClawBox 写 agent 文件前,对"该操作可能触碰的全部路径"拍统一快照;用户可
浏览快照历史并一键恢复到任意快照时刻。

## 非目标(v1 明确不做)

- CLI 型 agent(openclaw/hermes)的自动恢复——快照记录照存,restore 返回明确
  错误提示手动操作
- ClawBox 自身配置(providers 列表等)的快照/恢复
- cc-switch / transfer 导入路径的快照(它们只写 ClawBox 自己的 config)
- 可配置保留数量(固定 20 份/agent)
- 快照内容 diff 查看器

## 已批准的决策

1. **恢复后记账处理**:恢复成功后清掉该 agent 对应维度的 managed 记账与绑定
   (如 provider 恢复清 `agent_providers[agent]` + `providers_managed[agent]`)。
   agent 页显示 unsynced,用户想再下发可主动触发。不清账会导致启动对账把回滚
   当漂移"安全自愈",悄悄撤销用户的撤销。
2. **CLI 型 agent**:快照存(`restorable: false`),restore 报错引导手动操作。
3. **UI 放 Agents 页弹层**,不做独立路由。
4. **每 agent 保留最近 20 份**,固定不可配。

## 快照存储格式

```
~/.clawbox/snapshots/<agent_id>/<id>/manifest.json
~/.clawbox/snapshots/<agent_id>/<id>/blobs/0          # 文件内容(编号 = entries 下标)
~/.clawbox/snapshots/<agent_id>/<id>/blobs/2/...      # 目录树(entries[2] 为 dir)
```

`<id>` = `yyyyMMdd-HHmmss[-N]-<scope>`,同秒冲突时追加 `-N` 序号。

`manifest.json`:

```json
{
  "id": "20260814-120000-provider",
  "agent_id": "codex",
  "scope": "provider | fallback | mcp | skills | memory",
  "summary": "provider sync: 3 files",
  "restorable": true,
  "created_at": "2026-08-14T12:00:00Z",
  "entries": [
    {"rel_path": ".codex/config.toml", "kind": "file", "blob": "0", "size": 123},
    {"rel_path": ".codex/auth.json", "kind": "missing"},
    {"rel_path": ".config/opencode/skills/foo", "kind": "symlink", "target": "~/.clawbox/skills/library/foo"}
  ]
}
```

entry kind:

- `file` — 拷贝内容到 `blobs/<i>`
- `missing` — 快照时不存在(恢复时删除现存的)
- `symlink` — 记软链目标(恢复时删现存、按目标重建;目标以库内绝对路径存)
- `dir` — 递归拷贝整树到 `blobs/<i>/`(恢复时 `remove_dir_all` 现存后重建,即
  **精确恢复目录快照时刻的状态**;skills 用,UI 确认弹窗明示此语义)

安全约束:`rel_path` 一律 home 相对、恢复时拒绝绝对路径与 `..` 组件(防手改
manifest 后的路径逃逸)。

## 核心模块:`src-tauri/src/sync/snapshots.rs`

home 参数化 + TempHome 测试,与 `sync` 其余模块同铁律。

- `capture(home, agent_id, scope, summary, paths) -> Result<SnapshotInfo>`
  逐路径按 kind 记录;写 manifest;按 agent 修剪到最近 20 份。
- `list(home, agent_id) -> Vec<SnapshotInfo>`(只读 manifest,不载 blobs)
- `restore(home, agent_id, snapshot_id) -> Result<RestoreResult>`
  - 先对当前状态拍 **pre-restore 安全快照**(撤销也可撤销)
  - `restorable: false` → Err(CLI 引导文案)
  - 逐 entry 恢复;成功后清对应维度记账并 save_config
- `RestoreResult { restored: Vec<String>, cleared: Vec<String> }`
  (restored = 恢复的 rel_path 清单;cleared = 清掉的记账字段名,供 UI 汇报)

记账清理映射:

| scope | 清理 |
|---|---|
| provider | `agent_providers` + `providers_managed` |
| fallback | `agent_fallbacks` + `providers_fallback_managed` |
| mcp | `mcp_managed` |
| skills | `skills_managed` |
| memory | `memory_managed` |

## 触碰路径声明

- `ConfigAdapter`(MCP)与 `ProviderAdapter` 各加默认方法
  `touch_paths(&self, home) -> Vec<PathBuf>`,默认 `[config_path]`;
  Codex 两个 adapter 覆写为 `[config.toml, auth.json, catalog]`
- skills `apply_one`:`agent_skills_dir` 的一级子项(目录不存在则记 dir missing)
- memory `apply_one`:`agent_memory_path` 文件
- CLI 型(空 config_path):capture 空清单 + `restorable: false`

## 接线(替换旧机制)

四个 apply 站点(`sync::apply_one` MCP、`providers::apply_one`、
`providers::apply_fallbacks_one`、`skills::apply_one`、`memory::apply_one`)
的 `backup_target` 调用全部替换为 `snapshots::capture`。providers 的
`validate_or_rollback` 事务回滚改从快照 blob 恢复。`backup_target` 与
`~/.clawbox/backups/` 旧目录:函数删除,旧目录遗留不迁移不清理(无害)。
`ApplyResult.backup_path` 字段语义改为快照 id(改名 `snapshot_id`,前端
同步更新)。

## Tauri 命令(新 `src-tauri/src/commands/snapshots.rs`)

- `snapshots_list(agent_id: Option<String>) -> Vec<SnapshotInfo>`(None = 全部
  agent,按时间倒序;持 CONFIG_LOCK)
- `snapshots_restore(agent_id, snapshot_id) -> RestoreResult`(持 CONFIG_LOCK)

## 前端

Agents 页每个 agent 卡片/行加「快照历史」入口 → 弹层:

- 列表:时间、scope 徽章(provider/MCP/skills/memory)、摘要、文件数、
  `restorable: false` 灰化并标「不可自动恢复」
- 恢复按钮 → 确认弹窗(明示:文件将恢复到快照时刻;skills 为目录精确恢复;
  对应托管记账将被清除,ClawBox 显示未同步)→ 成功 toast 展示
  restored/cleared 摘要
- `src/lib/api/snapshots.ts` 类型化封装;en/zh 文案同步

## 测试

snapshots.rs 单测(TempHome):file/symlink/missing/dir 四种 entry 的
capture→restore 往返、恢复删除 apply 产物、pre-restore 安全快照存在、记账
清理、prune 保留 20、同秒 id 冲突、rel_path 逃逸拒绝、restorable:false 拒绝
恢复。接线后跑全量 `cargo test` + `npm run check`。
