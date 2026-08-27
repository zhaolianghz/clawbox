# ClawBox 功能路线图（头脑风暴记录）

> 日期：2026-08-14
> 状态：讨论稿，未排期。条目按主题分组，标注了初步优先级判断，供后续逐项立项。

## 现有能力基线

ClawBox 目前已具备：

- Agent 检测 / 安装 / 升级（claude-code、codex、openclaw、opencode、codebuddy、cursor-agent、kimi、qodercli、gemini、cline、hermes 等，见 `src-tauri/src/agents/mod.rs`）
- 模型 Provider 配置 CRUD + 连通性测试（`src/commands/config.rs`、`provider_test.rs`）
- 按 agent 绑定 provider 并同步下发（`src/commands/sync.rs`、`src/sync/providers.rs`）
- MCP / Skills / Memory 同步（`src/sync/`，OpenClaw / Hermes 走 `backends` capability trait）
- cc-switch 导入、配置导出/导入 `.clawbox.json`（`cc_switch.rs`、`transfer.rs`）
- Gateway 状态查看（openclaw / hermes）

架构原则：**ClawBox 只写配置、不代理运行时**——各 agent 自己跑，ClawBox 负责把配置同步到它们自己的配置文件里。

---

## 一、用量与成本线（进行中：token 统计）

### 1. Token 用量统计（正在设计，讨论记录见下文「附录」）

统计本机各 agent CLI 的 token 消耗：按天 / 按 agent / 按模型聚合，成本估算。

### 2. 预算告警

设定月度 token / 费用预算，接近阈值时发系统通知。依赖 #1 的聚合数据。

### 3. 磁盘清理

分析 `~/.claude`、`~/.codex` 等目录体积，按保留期清理旧会话文件。与 #1 的「聚合固化」天然配合：先把统计数字固化进 ClawBox 自己的存储，再清理原始文件，不丢数。

---

## 二、配置信任线

### 4. 配置快照与回滚 ✅ 已实现（2026-08-14）

每次 sync 前自动快照 agent 的原始配置文件，时间机器式一键恢复。

- 已落地：统一快照层 `~/.clawbox/snapshots/`（file / missing / symlink / dir 四种条目，每 agent 保留 20 份），四条 apply 线（provider / fallback / MCP / skills / memory）全部接入；Agents 页快照面板可浏览与恢复；恢复时自动清对应维度托管记账，防止启动 reconcile 自动「修复」回去（设计：`docs/superpowers/specs/2026-08-14-config-snapshots-rollback-design.md`）

- 价值：ClawBox 直接改写用户的 agent 配置文件，这是采纳的最大心理门槛——「改坏了能撤销」直接消除顾虑
- 成本：低。现有 sync 的 plan/apply 架构正好挂快照钩子

### 4b. 内置「官方默认」服务商 ✅ 已实现（2026-08-27）

agent 一键恢复官方默认配置:绑定选择器置顶「官方默认」,绑定并同步即清掉 ClawBox 下发的全部键(复用解绑路径),用户自有配置不动;绑定关系保留、UI 显式可见,再绑真实服务商即可切回。虚拟条目不落盘、不可编辑删除。(设计:`docs/superpowers/specs/2026-08-27-default-provider-design.md`)

### 5. 安全审计

扫描危险配置并报告：`bypassPermissions` / `--dangerously-skip-permissions`、过宽的 `allowedTools`、明文写在 MCP env 里的 API key 等。

### 6. API key 保险箱

key 集中管理（OS keychain 存储）+ 映射「哪个 agent 的哪个配置在用哪个 key」+ 明文泄漏位置清单。

---

## 三、市场线

### 7. MCP 目录市场

内置常用 MCP server 目录（github、playwright、filesystem...），一键添加，替代手填 JSON。

### 8. Skills 市场

skills 目前是 repo 同步，升级为可浏览 / 搜索 / 按需安装的市场。

### 9. 模型价格表管理

#1 的配套设施：内置可更新的模型价格表 + 用户自定义费率。

---

## 四、健康运维线

### 10. 一键 doctor 体检 ✅

综合检查：PATH、node 版本、配置文件损坏、API key 失效、磁盘占用。provider_test 已有单点能力，缺全局整合。
（已实现：PATH / 运行时依赖 / 孤儿绑定 / 配置漂移 / Provider 拨测 / 后端网关 六项只读体检，Agents 页一键触发；磁盘占用归 #3 未包含。）

### 11. Provider 可用性监测

定时探测已绑定的 provider，展示延迟 / 可用率历史（key 失效早知道）。

### 12. 统一启动器

选项目目录 + 选 agent，一键拉起（集成终端或系统终端）。

---

## 五、记忆与规则线

### 13. AGENTS.md / CLAUDE.md 管理器

跨 agent 的项目规则 / 记忆文件统一编辑、模板化、同步下发（memory 同步已有雏形）。

### 14. Hooks 管理器

Claude Code hooks 的 GUI 化配置。

---

## 六、多机线

### 15. Git 云同步

现在的导出/导入是手动搬 `.clawbox.json`，升级为绑定 git 仓库自动同步多机配置。

---

## 初步优先级

**4（快照回滚）> 1（token 统计，进行中）> 10（doctor）**，随后按需求强度排 7（MCP 市场）、5（安全审计）。理由：

- 4 成本最低、直接消除「ClawBox 改我配置」的信任顾虑
- 1 已在设计中，且其 JSONL 解析基建可被会话浏览器等复用
- 10 把散落的检测能力串成一次「体检报告」，用户感知强

---

## 附录：Token 统计设计讨论记录（2026-08-14，未定稿）

### 数据源事实

各 agent 本身会把每轮会话的 token 用量落盘（`usage` 字段直接来自 API 响应，与计费同源）：

| Agent | 数据源 | 内容 |
|---|---|---|
| Claude Code | `~/.claude/projects/**/*.jsonl` | 每条 assistant 消息带 usage（input/output/cache_read/cache_write）+ model + 时间戳 |
| Codex | `~/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl` | `token_count` 事件，含 total/cached/last 用量与模型信息 |
| OpenClaw | `~/.openclaw/` 自己的会话存储 | gateway 统一记录 |
| Gemini CLI | `~/.gemini/tmp/*/chunks.json` | `usageMetadata` |
| OpenCode | `~/.local/share/opencode/` 消息存储 | tokens 字段 |

### 三种实现路径

- **A. 本地文件解析（推荐）**：Rust 侧新增 `src-tauri/src/usage/`，仿照 capability 模式定义 `UsageProvider` trait，每 agent 一个 adapter。覆盖历史、离线可用、符合「不代理运行时」哲学；需增量缓存应对大文件
- **B. 调 agent 官方命令/接口**：契约稳定但覆盖率极差（多数 CLI 无 usage 报告命令），不可行
- **C. 本地网关代理计量**：最准确但架构改动巨大，且统计不到 ClawBox 外的使用，与现有哲学冲突

第一版建议只做 **Claude Code + Codex**（格式最清楚），`UsageProvider` trait 留扩展位。

### 已识别的风险与对策

**准确性**：usage 字段即 API 计费数字，非本地估算，数值本身可信。真正风险是重复计数（session resume / rewind 分支、subagent sidechain）→ 按 `(session_id, message_id)` 去重，借鉴 ccusage 的成熟逻辑。

**完整性（丢数据）**：

| 丢失来源 | 严重度 | 对策 |
|---|---|---|
| Claude Code 默认 30 天清理旧会话（`cleanupPeriodDays`） | 高 | **聚合固化**：解析后把「天 × agent × 模型」聚合桶写入 ClawBox 自己的存储，与原始格式解耦 |
| 用户删文件 / 重装 / 换机 | 高 | 同上，只丢「ClawBox 未运行期间」的增量 |
| 非本机消耗（网页版、IDE 插件、直调 API） | 中 | 本地统计天花板即「本机 CLI 真实消耗」，任何本地方案都无法突破 |

**格式无官方文档、版本升级可能变**，五层防御：

1. 按「形状」提取而非严格 schema：JSONL 逐行容错，不认识的行跳过并计数；字段名别名映射
2. adapter 内多格式 revision + 采样探测自动选择
3. 匹配率上报：匹配率低于阈值（如 80%）时 UI 亮黄条「格式可能已变更，近期数据可能缺失」，降级可见而非静默丢失
4. 聚合存储与格式解耦：格式变更最坏只影响「变更之后的新增数据」，历史不受影响
5. 故障隔离（照抄 `collect_backends` 的 errors 并列返回模式）+ 仓库内脱敏 fixture 金样本测试；盯 ccusage 等同生态工具的 issue 作变化预警

不采用：数据驱动 / 可热更新的解析规则 DSL（复杂度远超收益）。

### 性能

rayon 并行 + 增量缓存（按 文件路径+size+mtime 只解析新增/变更文件），首次全量、之后秒级。

### 前端形态

新增 `src/routes/usage/+page.svelte`，按天堆叠柱状图（agent 分色）、模型/项目维度下钻、汇总卡片；SVG 图表不引重型库；en/zh 文案同步。

### 待定问题（未决）

1. 统计用途是「看趋势控用量」还是「成本核算」？→ 决定固化机制做多重（首次全量导入 + 后台定期固化 vs 启动顺手扫一遍）
2. 第一版 agent 范围：仅 Claude Code + Codex，还是把 OpenClaw 也纳入？
3. 成本估算（价格表）是否第一版就要？
