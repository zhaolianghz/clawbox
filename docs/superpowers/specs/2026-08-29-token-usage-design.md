# Token 用量统计（路线图 #1）设计

日期：2026-08-29 · 状态：已批准
上游：路线图 `docs/ROADMAP.md` 附录「Token 统计设计讨论记录（2026-08-14，未定稿）」已定方向 A（本地文件解析 + UsageProvider trait + Claude/Codex 优先 + 聚合固化），本 spec 把方向 A 落成可执行细节。

## 目标

把本机各 agent CLI 真实的 token 消耗，按「天 × agent × model × 口径」聚合固化进 ClawBox 自有存储，向用户呈现：
- 每个 agent 的总消耗、近期趋势、按模型拆分
- 每个 provider 的总消耗（同一 provider 被多个 agent 共享时按该 agent 当日该 provider 的使用占比分摊）
- 解析失败/格式变更时降级可见（黄条提示），而非静默丢数

**非目标**（v1 不做）：
- 成本/费用估算（依赖价格表，留 #9）
- 预算告警（依赖 v1 聚合数据，留 #2）
- 后台定时扫描（v1 只做启动顺手扫一遍 + 增量缓存）
- OpenClaw / Gemini / OpenCode 等其它 agent（v1 仅 Claude Code + Codex）
- 删旧会话文件的磁盘清理（留 #3）

## 架构原则

继承项目核心架构原则：**ClawBox 只写配置、不代理运行时**。本特性纯只读，扫描各 agent 自己落盘的会话日志，不代理任何 LLM 请求。

## 数据源（已验证）

### Claude Code — `~/.claude/projects/**/*.jsonl`
- 每行 JSON 对象，记录类型由 `type` 字段驱动
- 关注的 assistant 消息：`message.role == "assistant" && message.usage != null`
- 字段：
  - `message.usage.input_tokens` — 新输入 token
  - `message.usage.output_tokens` — 输出 token
  - `message.usage.cache_read_input_tokens` — 缓存命中读
  - `message.usage.cache_creation_input_tokens` — 缓存写入
  - `message.model` — 模型 id（如 `claude-sonnet-4-5-...`）
  - `message.id` — 消息 id（同 `(sessionId, messageId)` 重复出现时去重）
  - 顶层 `timestamp` — RFC3339
  - 顶层 `sessionId` — 会话 id
  - 顶层 `cwd` — 项目路径（可作为 project 维度，本期不展示但保留字段）
  - 顶层 `version` — Claude Code 版本号（用于格式兼容性判定）

### Codex — `~/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl`
- 关注的 token 事件：`type == "event_msg" && payload.type == "token_count"`
- `payload.info.total_token_usage` 是**累积**而非单次增量：
  - `input_tokens`、`cached_input_tokens`、`output_tokens`、`reasoning_output_tokens`、`total_tokens`
- `payload.info.last_token_usage` 是本 turn 增量（部分版本/场景缺失）
- **必走差值口径**：相邻两次 `token_count` 事件，`total` 之差即本次 turn 增量；第一个事件的 `total` 即其 turn 的增量（初始累计为 0）
- 模型：`payload.info.model_context_window` 旁边无 model id；模型字段在 `session_meta.payload.model_provider`（"openai"）和 `turn_context.payload.model`（如 "gpt-5.5"）
- 时间戳：顶层 `timestamp`，RFC3339
- session：顶层 `session_meta.payload.id`
- 增量去重 key：`(session_id, turn_id)` 组合（`payload.rate_limits` 里有 turn_id 字段；首版可只按 `session_id + 上次 total_token_usage 的快照` 文件级增量解析，省一次去重）

### 去重与口径

| Agent | 计费字段 | Claude 用量字段 | Codex 差值字段 |
|---|---|---|---|
| 输入 | `input_tokens` + `cache_read` | 同 | `input_tokens - cached_input_tokens`（不计 cache 命中） |
| 缓存读 | `cache_read_input_tokens` | 同 | `cached_input_tokens` |
| 缓存写 | `cache_creation_input_tokens` | 同 | 不单独给，含在 input 里 |
| 输出 | `output_tokens` | 同 | `output_tokens + reasoning_output_tokens`（reasoning 也是输出 token） |

口径选择基于「与官方计费字段同源」原则，不做估算。前端展示时按列名标注 input/cache_read/cache_write/output 4 列，reasoning 不单独列但并入 output。

去重 key：
- Claude Code：`(session_id, message_id)`（message_id 在 `message.id`）
- Codex：(上一份 `total_token_usage` 快照 vs 当前 `total_token_usage` 的差值天然防重复；只在解析单个文件时维护文件内的 last_seen 快照即可)

## 模块与目录结构

```
src-tauri/src/usage/
  mod.rs            # pub use 各 adapter + UsageProvider trait + 聚合入口
  aggregate.rs      # 增量缓存 (path→size+mtime) + 多 agent 并行扫描 + 汇总到 clawbox storage
  store.rs          # ~/.clawbox/usage/usage-YYYY-MM.json 读写;append-only 月桶
  claude_code.rs    # ClaudeCodeUsageProvider
  codex.rs          # CodexUsageProvider
  fixtures/         # 脱敏 JSONL 金样本（单文件 ≤ 50 行）
    claude-code/basic.jsonl
    claude-code/cache_heavy.jsonl
    claude-code/sidechain_dedup.jsonl
    codex/initial_token_count.jsonl
    codex/multiple_turns.jsonl
    codex/missing_model.jsonl

src-tauri/src/commands/usage.rs   # usage_summary, usage_refresh, usage_provider_summary
src-tauri/src/lib.rs              # 注册 pub mod usage
src-tauri/src/commands/mod.rs     # 注册 mod usage

src/lib/api/usage.ts              # 前端 invoke 封装
src/routes/usage/+page.svelte     # /usage 页面
src/routes/agents/+page.svelte    # 头部加「用量」按钮 + 每 agent 卡片加本月小条
src/routes/providers/+page.svelte # 每个 provider 卡片加本月消耗
src/lib/i18n/en.json              # usage.* 新增键
src/lib/i18n/zh.json              # 同上
```

## UsageProvider trait

```rust
// src-tauri/src/usage/mod.rs
pub trait UsageProvider: Send + Sync {
    /// 稳定标识,对应 AgentStatus.id(如 "claude-code"、"codex")。
    fn agent_id(&self) -> &'static str;
    /// 是否在本机可用(检查 agent CLI 是否已装);未装跳过扫描。
    fn available(&self) -> bool;
    /// 扫描所有相关文件 → 一组 UsageEvent。
    /// 容错:逐行解析失败的行计入 ParseStats.skipped_lines,不抛错。
    fn scan(&self, home: &Path) -> Result<UsageScan, UsageError>;
}

pub struct UsageEvent {
    pub ts: DateTime<Utc>,           // 事件时间
    pub session_id: String,          // 跨文件去重键
    pub event_id: String,            // Claude: message.id; Codex: turn_id 或自合成
    pub model: String,               // 未识别模型归入 "unknown"
    pub input_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_creation_tokens: u64,  // Claude 有;Codex 归 0
    pub output_tokens: u64,          // Codex 已含 reasoning
}

pub struct UsageScan {
    pub agent_id: String,
    pub events: Vec<UsageEvent>,
    pub stats: ParseStats,
}
pub struct ParseStats {
    pub files_scanned: usize,
    pub files_skipped: usize,        // 不可读/权限
    pub lines_total: usize,
    pub lines_matched: usize,
    pub lines_skipped: usize,        // 解析失败/不认识
}
```

## 增量缓存

`~/.clawbox/usage/cache.json`（与 config.json 同级，迁移兼容）：
```json
{
  "version": 1,
  "entries": {
    "<绝对路径>": {
      "size": 12345,
      "mtime_ms": 1724000000000,
      "last_event_id": "msg_xxx",   // Claude;Codex 用 last_total_token_usage
      "consumed_total": { "input": 8015, "output": 585 }
    }
  }
}
```

逻辑：
- 启动调用 `usage_refresh`：枚举 JSONL 路径，路径命中缓存且 size+mtime 未变 → 跳过；否则增量解析（新事件加 `last_event_id` 之后的部分；Codex 改用 last_total 快照差值），写回缓存
- 缓存损坏 → 视为全量重扫，UI 提示「缓存重建中」
- 缓存缺失文件 → 视为被删，记 stats.skipped_files

## 聚合存储（**核心：抗原始文件清理**）

`~/.clawbox/usage/usage-YYYY-MM.json`（每月一个文件，append-only 桶）：
```json
{
  "version": 1,
  "month": "2026-08",
  "buckets": {
    "2026-08-29": {
      "claude-code:claude-sonnet-4-5": {
        "input": 12345, "cache_read": 678, "cache_creation": 47780, "output": 585,
        "events": 3
      },
      "codex:gpt-5.5": {
        "input": 17993, "cache_read": 2432, "cache_creation": 0, "output": 341,
        "events": 1
      }
    }
  },
  "agent_to_provider_at_scan": {
    "claude-code": "p-anthropic-1",
    "codex": "p-openai-1"
  },
  "last_scan_at": "2026-08-29T10:00:00Z"
}
```

关键：
- **key 用 `agent_id:model`**——同一 provider 多个 agent 时不丢数据（agent 是天然唯一维度）
- **`agent_to_provider_at_scan`** 每天/扫描时按当时 `Config.agent_providers` 绑定快照落库，后续统计「某 provider 的消耗」= 该日所有 agent 的消耗按当时的 binding 求和。**绑定后续被改**，回填只影响新一天的桶，旧桶保留当时的归属——这是「聚合固化」的核心：先按天固化，归属于当天绑定，跨天回看不会因绑定变化而失真。
- 文件 append-only：增量写入对应月份的桶；旧月份文件只读不写。
- 解析失败只影响当天新增桶，旧桶不受影响。

## Provider 视角汇总

UI「provider 卡片本月消耗」= 扫所有月份文件，按 `agent_to_provider_at_scan` 把每个 bucket 的 (input, output, ...) 求和到 provider 名下。

**关键约束**：agent 的 fallback provider 也算消耗（fallback 时实际调的是 fallback provider），用 `agent_fallbacks_at_scan` 同样快照落库。v1 仅 hermes 支持 fallback（路线图附录「provider 绑定」一节），其它 agent fallback 为空。

## Tauri 命令

```rust
// commands/usage.rs
#[tauri::command]
pub fn usage_summary(window_days: u32) -> Result<UsageSummary, String>;
#[tauri::command]
pub fn usage_refresh() -> Result<UsageRefreshReport, String>;
#[tauri::command]
pub fn usage_provider_summary() -> Result<Vec<ProviderUsage>, String>;

pub struct UsageSummary {
    pub total: UsageTotals,         // 全部时间总和
    pub by_day: Vec<DayUsage>,      // 最近 N 天,含今天
    pub by_agent: Vec<AgentUsage>,  // 按 agent 聚合
    pub parse_health: ParseHealth,  // 见下
}
pub struct UsageTotals { input: u64, cache_read: u64, cache_creation: u64, output: u64 }
pub struct DayUsage { date: String, totals: UsageTotals, by_agent: Vec<AgentUsage> }
pub struct AgentUsage { agent_id: String, label: String, totals: UsageTotals, by_model: Vec<ModelUsage> }
pub struct ModelUsage { model: String, totals: UsageTotals, events: u64 }

pub struct ParseHealth {
    pub last_scan_at: Option<String>,
    pub matched_ratio: f64,         // lines_matched / lines_total; < 0.8 触发 warn
    pub errors: Vec<ParseError>,    // 各 adapter 的失败概要(脱敏,无 key 路径)
}
pub struct ParseError { agent_id: String, kind: String, message: String }

pub struct UsageRefreshReport {
    pub added_events: u64,
    pub added_buckets: u64,
    pub parse_health: ParseHealth,
}
```

`invoke_handler` 注册这三个命令。

## 前端形态

### Agents 页（`/agents`）
- 头部「用量」按钮（与现有刷新/体检按钮并排）→ 跳 `/usage`
- 每个 agent 卡片底部加一行小字：`本月: 1.2M tokens · claude-sonnet-4-5 78% · claude-haiku 22%`（若该 agent 无数据则隐藏该行）

### 新增 `/usage` 页
- 顶部 4 张卡片：今日 / 7 天 / 30 天 / 全部
- 中部「按天堆叠柱状图」(SVG，无外部库)：每个柱子一天，分色按 agent；hover 显示该日明细
- 下部「按 agent 折叠列表」：每 agent 一行 `agent 名 · 30 天总量 · 模型构成 chip · 占比`
- 顶栏右上角「刷新」按钮 + 上次扫描时间（相对时间）+ 解析健康徽章（✓ 绿色 / ⚠ 黄色：matched_ratio < 0.8）
- i18n en/zh 同步

### Providers 页
- 每个 provider 卡片加一行小字：`本月: X tokens`（按 `agent_to_provider_at_scan` 汇总）

## 已识别的风险与对策（继承路线图附录）

| 风险 | 严重度 | 对策 |
|---|---|---|
| Claude Code 默认 30 天清旧会话 | 高 | **聚合固化**到 `usage-YYYY-MM.json`，原始清理不丢数 |
| 用户删文件/重装/换机 | 高 | 同上；只丢未运行 ClawBox 期间的增量 |
| Codex `token_count` 是累积值 | 高 | 解析时按文件维护 last_total 快照、差值即增量；首批事件视为 turn 增量 |
| Codex `last_token_usage` 不一定存在 | 中 | 不依赖，**只用 `total_token_usage` 差值** |
| Codex `token_count` 事件未必每 turn 都出 | 中 | 缺失 turn 视为「无法归因」，跳过；解析健康上报 |
| 格式无官方文档、版本升级可能变 | 中 | 五层防御：①形状提取逐行容错 ②adapter 内多 revision ③matched_ratio 低于 0.8 → UI 黄条 ④聚合与原始格式解耦 ⑤故障隔离(各 adapter 独立 panic catch) + 仓库金样本 fixture 测试 |
| 非本机消耗(网页版/IDE/直调 API) | 不可控 | 本地统计天花板即「本机 CLI 真实消耗」，any 本地方案都突破不了 |
| 同 provider 多 agent 共享时归属 | 中 | 按当时 binding 快照落库，跨天回看不变 |
| fallback provider 消耗归属 | 低 | v1 hermes only，照快照落 `agent_fallbacks_at_scan` |

## 测试

- `usage/aggregate.rs`:增量缓存命中/失效/损坏三种；启动顺手扫一遍只跑新增
- `usage/claude_code.rs`:fixture `basic` / `cache_heavy` / `sidechain_dedup`(同一 message_id 出现两次只算一次)
- `usage/codex.rs`:fixture `initial_token_count`(单事件) / `multiple_turns`(差值口径) / `missing_model`(归 unknown)
- `usage/store.rs`:append-only 月桶；同月二次扫描去重；跨月桶读取
- `commands/usage.rs`:summary / refresh / provider_summary 三命令,空数据/有数据/混合数据三态
- 不做真实网络/真实 CLI 调用单测（与 doctor 一致，保持薄）

## 决策记录

- a) 路径 A(本地文件解析)而非 B(调 agent 命令)/ C(本地代理)— 已批准（路线图附录）
- b) v1 仅 Claude Code + Codex,其它 agent 留位 — 已批准
- c) 聚合桶 key 用 `agent:model`,provider 视角靠快照汇总 — 已批准
- d) 扫描时机:启动顺手扫一遍 + mtime 增量缓存;后台定时下版 — 已批准
- e) UI:agents 按钮+卡片 / 新增 /usage 顶级页 / providers 卡片附加 — 已批准
- f) 数字粒度分 4 列(input/cache_read/cache_write/output) — 已批准
- g) 不做成本/费用/预算 — 留 #9 / #2
