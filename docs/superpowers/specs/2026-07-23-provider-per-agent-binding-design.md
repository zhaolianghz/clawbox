# 服务商 per-agent 绑定（选中即生效） 设计

**日期**: 2026-07-23
**状态**: 待用户认可
**分支**: v1-config-center

## 背景与目标

当前服务商生效链路繁琐：配置服务商 → 星标设全局默认 → 打开同步面板 → plan 预览 →
勾选 agent → apply；换服务商还要再走一遍同步。本设计把「同步」这个显式步骤删掉：

- **服务商是全局池**：在 Providers 页配置好即对所有 agent 可用（概念上"默认已同步"）。
- **生效在 agent 侧**：每个 agent 独立选择自己用哪个服务商，选中即写入生效；切换即
  重写；选「不由 ClawBox 管理」即恢复原状。
- **旧机制彻底移除**：全局星标默认服务商、同步面板、quickSync 提示条全部下线。

已确认的关键取舍（用户认可）：

1. **每个 agent 独立选择**服务商（非全局单默认）。
2. 交互为**选择器选中即生效**（无开关、无预览确认）。
3. **删除服务商时只解绑，不清理 agent 配置文件**（不打断正在工作的 agent）。
4. **编辑服务商自动重推到绑定它的 agent；个别 agent 推送失败不回滚保存**，只标状态。
5. OpenCode 从「下发全部启用的服务商」**简化为只下发绑定的那一个**（行为变化，已知情）。

## 数据模型（`Config`，`~/.clawbox/config.json`）

```rust
pub struct Config {
    // ...现有字段...
    /// agent_id -> 绑定的服务商 id。无条目 = ClawBox 不管理该 agent 的服务商配置。
    #[serde(default)]
    pub agent_providers: HashMap<String, String>,
    /// 废弃：仅为迁移保留可反序列化；load_config 迁移后清空，不再落盘。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_provider_id: Option<String>,
    /// 保留不动：agent -> 上次写入的键名，驱动「只删我们写过的键」的安全移除。
    pub providers_managed: HashMap<String, Vec<String>>,
}
```

- 删除服务商（`config_providers_set` 后列表里消失）时：自动移除绑定它的
  `agent_providers` 条目；**不写 agent 配置文件**，该 agent 状态回落为「未管理」；
  `providers_managed` 保留，待下次绑定时按 remove 检测自然清理。

## 后端命令

### 新增

```rust
/// 绑定并立即生效。provider_id = None 表示解绑（恢复 agent 原状）。
/// 事务性：先 apply 成功，再落盘绑定；失败则绑定不变、返回错误。
#[tauri::command]
pub async fn agent_provider_bind(agent_id: String, provider_id: Option<String>)
    -> Result<ApplyResult, String>;
```

- 绑定路径：校验 provider 存在且 `enabled`、adapter `supported`、端点槽位兼容
  （复用 `pick_endpoint` 逻辑）→ `apply_one`（沿用 apply 前自动备份）→ 成功后写
  `agent_providers[agent_id]` + 更新 `providers_managed`。
- 解绑路径：以「无激活服务商」语义调 `apply_one`（`providers = []`），按
  `providers_managed` 清掉我们写过的键 → 成功后删除绑定条目与
  `providers_managed[agent_id]`。例外：hermes 适配器无 remove 语义（清空
  model.* 会破坏其运行），解绑仅停止管理、保留现值——沿用适配器既有行为。
- 绑定读取：并入 `agent_sync_overview` 返回（新增 `bound_provider_id` 字段），
  避免多一次 IPC。

### 修改

- `config_providers_set`（编辑/保存服务商）：保存后对所有绑定了**被改动**服务商的
  agent 自动重新 `apply_one`，返回逐 agent 结果供 UI toast。保存本身不因个别推送
  失败回滚；失败 agent 状态由 overview 显示为「待更新」，可在 Agents 页重试（重新
  选一次即重推）。
- `sync_providers_status` / `agent_sync_overview`：per-agent 语义——每个 agent 按
  自己的绑定跑 plan diff，继续支撑漂移检测（外部改过配置显示「已偏离」）。

### 删除（对外命令下线）

`config_active_provider_get` / `config_active_provider_set` / `sync_providers_plan` /
`sync_providers_apply`。`plan_all` 降为 crate 内部函数，仅供 status/overview 使用。

### 适配器层（`sync/providers.rs`，不动刀）

`ProviderAdapter::plan/apply` **签名与九个实现零改动**。调用方对每个 agent 只传
**绑定服务商的单元素列表**（`providers = [bound_spec]`、`active_id = Some(bound_id)`）：

- 单激活适配器（claude-code / codex / codebuddy / hermes）：`resolve_single_active`
  在列表中找到绑定项，行为不变。
- 多服务商适配器（opencode / openclaw / kimi）：只下发列表里这一个；之前写过、
  本次不再下发的条目由 `managed` 差集自然清除（remove 检测靠 `providers_managed`，
  与传入列表无关，已核实 `OpencodeProviderAdapter::apply` 的 remove 逻辑）。
  OpenCode 的「全部启用 → 仅绑定项」行为变化由此自然达成。

## 前端 UI

### Agents 页（`src/routes/agents/+page.svelte`）

- 已安装且 `provider_supported` 的 agent 行内新增**服务商下拉选择器**：
  - 选项 = 全部已启用服务商 + 「不由 ClawBox 管理」（解绑项）。
  - 按该 agent 端点偏好置灰不兼容项（如 Claude Code 只能选配置了 Anthropic 端点的；
    置灰附原因小字）。
  - 选中 → `agent_provider_bind` → 行内 spinner → 成功短暂高亮 / 失败行内红字
    （交互模式与现有安装按钮一致）。
- 当前绑定与漂移状态来自 `agent_sync_overview`。

### Providers 页（`src/routes/providers/+page.svelte`）

- 删除：星标（默认服务商）、同步面板（`syncStage` 整套状态机）、quickSync 提示条、
  `sync_providers_plan/apply` 的调用。
- 每张服务商卡片改显示「N 个 agent 使用中」（来自绑定表）。
- 编辑保存后 toast：「已重新下发到 N 个 agent」/ 列出失败项。

### i18n

`zh.json` / `en.json` 同步增删键：删除同步面板文案，新增选择器、置灰原因、
toast 文案。

## 迁移（`load_config` 内，一次性、幂等）

旧配置若 `active_provider_id = Some(p)` 且 `agent_providers` 为空：

1. 对每个 `providers_managed` 非空（= 之前同步过）的 agent，生成绑定 `agent → p`。
2. 清空 `active_provider_id`。

不触发任何 agent 配置文件写入（内容本来就是同步过的），用户无感。已迁移过
（`agent_providers` 非空）或从未星标的配置原样通过。

## 错误处理与安全

- **两条铁律不变**：apply 前自动备份（`backup_target`）；`ChangeItem.detail` 与所有
  返回结构绝不含 apiKey 明文。
- 绑定 apply 失败：绑定不落盘，UI 行内红字，agent 配置保持 apply 前状态（备份可回滚）。
- 编辑重推部分失败：保存成功 + toast 列出失败 agent，overview 标「待更新」。
- 绑定指向的服务商被禁用（`enabled = false`）：overview 标「待更新」，选择器提示。

## 测试

沿用现有 tempdir-home 单测模式（`src-tauri/src/commands/config.rs` 尾部风格）：

- 绑定即写入 agent 配置 + `providers_managed` 更新。
- 解绑只删我们写过的键，用户自有键不动。
- 切换绑定：旧键清、新键写。
- 迁移：星标 + 已同步 agent → 自动绑定；幂等（跑两遍结果一致）；无星标/已迁移原样通过。
- 删除服务商自动解绑、不写 agent 文件。
- 编辑服务商自动重推到绑定 agent。
- 端点不兼容 / provider 禁用 / 未知 agent → 绑定报错且不落盘。
- 适配器现有测试不动（签名语义不变）。

## 不做的事（YAGNI）

- OpenCode 多服务商多选。
- 绑定级别的模型覆盖（沿用 provider 的 default_model 机制）。
- 同步预览/确认对话框（备份 + 可重试已足够）。
