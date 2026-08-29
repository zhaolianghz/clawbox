# Token 用量统计 实现计划

日期：2026-08-29 · Spec: `docs/superpowers/specs/2026-08-29-token-usage-design.md`

## Global Constraints

- 所有逻辑 home 参数化,TempHome 测试,不碰真实用户文件
- 薄命令层；中文提交；每任务一提交；TDD（先失败测试）
- i18n en/zh 同步；项目无弹窗惯例（内联面板 / 顶部跳转）
- 五层防御:形状提取逐行容错 / 多 revision / matched_ratio 阈值 / 聚合与原始格式解耦 / 故障隔离 + fixture 金样本
- 字段口径与官方计费同源,不做估算

---

## Task 1: fixture 金样本(优先落,后续 parser 写完直接吃)

**Files:** `src-tauri/src/usage/fixtures/{claude-code,codex}/*.jsonl`

- [ ] 脱敏 fixture：从 `~/.claude/projects` 和 `~/.codex/sessions` 抓真实样本，手动改成脱敏版（key 替换 `sk-secret-XXX`，路径替换 `/Users/...` 为 `/redacted/path`），每个 fixture ≤ 50 行
- [ ] `claude-code/basic.jsonl`：1 个 session、3 条 assistant usage,带 input/output/cache_read/cache_creation
- [ ] `claude-code/cache_heavy.jsonl`：大 cache_read/cache_creation 对比
- [ ] `claude-code/sidechain_dedup.jsonl`：同一 `(sessionId, message.id)` 出现两次，验证只算一次
- [ ] `codex/initial_token_count.jsonl`：单次 token_count 事件
- [ ] `codex/multiple_turns.jsonl`：连续 3 次 token_count，验证差值口径
- [ ] `codex/missing_model.jsonl`：turn_context 无 model 字段，验证归 `unknown`
- [ ] `cargo test -p clawbox-core usage` fixture 加载测试（仅验证 fixture 合法 JSON）
- [ ] Commit: `test(usage): 脱敏 fixture 金样本(Claude/Codex)`

## Task 2: UsageProvider trait + 公共类型

**Files:** `src-tauri/src/usage/mod.rs`;`src-tauri/src/lib.rs`

- [ ] 失败测试：`UsageEvent`/`UsageScan`/`ParseStats`/`UsageError` 类型构造
- [ ] 定义 trait `UsageProvider { agent_id, available, scan }`
- [ ] 公共辅助：`pub fn all_providers() -> Vec<Box<dyn UsageProvider>>` 返回 Claude + Codex
- [ ] `pub mod usage;` 注册到 `lib.rs`
- [ ] `cargo test usage` 通过
- [ ] Commit: `feat(usage): UsageProvider trait + 公共类型`

## Task 3: Claude Code adapter

**Files:** `src-tauri/src/usage/claude_code.rs`;`usage/mod.rs`

- [ ] 失败测试：fixture `basic` → 3 个 events、字段精确、`model` 正确
- [ ] 失败测试：fixture `cache_heavy` → cache_read/cache_creation 正确
- [ ] 失败测试：fixture `sidechain_dedup` → 同一 `(session, message_id)` 只计一次
- [ ] 失败测试：非法 JSONL 行（截断/格式错） → 跳过且计入 `stats.skipped_lines`，不抛错
- [ ] 失败测试：缺 `message.usage` 的行 → 跳过
- [ ] 实现：`ClaudeCodeUsageProvider` 实现 trait；glob `~/.claude/projects/**/*.jsonl`；逐行 `serde_json::from_str`；按 `(sessionId, message.id)` 内存 HashSet 去重
- [ ] `cargo test usage::claude_code` 全绿
- [ ] Commit: `feat(usage): Claude Code adapter(JSONL 逐行 + 去重)`

## Task 4: Codex adapter(差值口径)

**Files:** `src-tauri/src/usage/codex.rs`;`usage/mod.rs`

- [ ] 失败测试：fixture `initial_token_count` → 单 events 增量 = total_token_usage 全字段
- [ ] 失败测试：fixture `multiple_turns` → 3 个 events 增量 = 相邻 total 差值
- [ ] 失败测试：fixture `missing_model` → 归 `unknown`
- [ ] 失败测试：缺 token_count 事件的文件 → 0 events + stats
- [ ] 实现：`CodexUsageProvider`；glob `~/.codex/sessions/**/rollout-*.jsonl`；维护文件内 last_total 快照；差值即本次 turn 增量；output_tokens + reasoning_output_tokens；模型从 `turn_context.payload.model` 提取
- [ ] 边界：第一个 token_count 事件视为 turn 增量（last_total=0）；`total_tokens` 字段不可信（不参与差值,避免四舍五入抖动）
- [ ] `cargo test usage::codex` 全绿
- [ ] Commit: `feat(usage): Codex adapter(累积差值口径)`

## Task 5: 聚合存储 + 增量缓存

**Files:** `src-tauri/src/usage/store.rs`;`src-tauri/src/usage/aggregate.rs`

- [ ] 失败测试：`store::append_bucket(month, day, agent_model, deltas)` → 月桶文件 append,二次调用同 key 累加
- [ ] 失败测试：`store::read_all() -> HashMap<String, DayBucket>` → 跨月桶读取
- [ ] 失败测试：`cache::should_rescan(path) -> bool` size+mtime 命中缓存返回 false
- [ ] 失败测试：`cache::update(path, stats)` 写入/损坏恢复
- [ ] 实现：append-only 月桶文件 `~/.clawbox/usage/usage-YYYY-MM.json`；增量缓存 `cache.json`；目录不存在时自动创建
- [ ] 失败测试：`aggregate::merge_into_buckets(events, config, at_scan)` 把 events 按 (day, agent, model) 聚合到月桶；同时快照 `agent_to_provider_at_scan` / `agent_fallbacks_at_scan`
- [ ] `cargo test usage::store` / `usage::aggregate` 全绿
- [ ] Commit: `feat(usage): append-only 月桶 + 增量缓存 + 聚合入口`

## Task 6: Tauri 命令 + lib.rs 注册

**Files:** `src-tauri/src/commands/usage.rs`;`src-tauri/src/commands/mod.rs`;`src-tauri/src/lib.rs`

- [ ] 实现：`usage_summary(window_days) -> UsageSummary` 读月桶聚合(按 window 过滤)
- [ ] 实现：`usage_refresh() -> UsageRefreshReport` 跑所有 provider 增量扫描 → 写月桶 + 写缓存
- [ ] 实现：`usage_provider_summary() -> Vec<ProviderUsage>` 按快照汇总到 provider 名下
- [ ] 失败测试：空数据/有数据/混合数据三态
- [ ] 注册到 `lib.rs::invoke_handler`
- [ ] `cargo test` 全量通过
- [ ] Commit: `feat(usage): usage_summary / refresh / provider_summary Tauri 命令`

## Task 7: 前端 — API 封装 + /usage 页面

**Files:** `src/lib/api/usage.ts`(新);`src/routes/usage/+page.svelte`(新);`src/lib/i18n/{en,zh}.json`;`src/lib/config.ts`(加路由)

- [ ] `usage.ts`:封装 `usageSummary(windowDays)` / `usageRefresh()` / `usageProviderSummary()` 三函数,完整 TS 类型对齐 snake_case
- [ ] `/usage` 页面：4 张汇总卡(今日/7天/30天/全部)+ SVG 堆叠柱状图(自写,无外部库)+ 按 agent 折叠列表 + 刷新按钮 + 解析健康徽章(matched_ratio < 0.8 黄色)
- [ ] i18n：`usage.*` 新增键(标题、4 卡 label、刷新按钮、健康徽章文案、空状态);en/zh 同步
- [ ] 路由注册(若项目有手动路由表)
- [ ] `npm run check` 0 错误
- [ ] Commit: `feat(ui): /usage 页面 + API 封装`

## Task 8: 前端 — Agents 页接入 + Providers 页附加

**Files:** `src/routes/agents/+page.svelte`;`src/routes/providers/+page.svelte`;`src/lib/i18n/{en,zh}.json`

- [ ] Agents 页头部加「用量」按钮(与现有体检/刷新并排)→ 跳 `/usage`
- [ ] Agents 页每个 agent 卡片加一行小字:本月总量 + 模型构成 chip(无数据隐藏)
- [ ] Providers 页每个 provider 卡片加一行小字:本月 tokens 数字(无数据隐藏)
- [ ] i18n:同步新增键
- [ ] `npm run check` 0 错误
- [ ] Commit: `feat(ui): Agents 页用量入口 + Providers 页本月消耗`

## Task 9: 收尾

- [ ] `cargo test` + `npm run check` 全绿
- [ ] ROADMAP #1 标 ✅,spec 链接挂上
- [ ] CHANGELOG 加 entry
- [ ] Commit: `docs: token 用量统计收尾(ROADMAP #1 ✅)`
