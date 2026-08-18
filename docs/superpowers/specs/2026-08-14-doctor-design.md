# 一键 Doctor 体检（路线图 #10）设计

日期:2026-08-14 · 状态:已批准

## 目标

一键汇总 ClawBox 与各 agent 的健康状态:PATH、运行时依赖、绑定孤儿、
配置漂移、Provider 连通性、后端网关。**只读报告 + 修复提示**,不做自动
修复(v2 再考虑)。核心价值:把散落在各页面的检测原语整合成一次体检,
新用户排障和老用户巡检共用一个入口。

## 非目标

- 自动修复(重新同步、改 PATH 等)—— v1 只给 hint 文案。
- 磁盘占用扫描(路线图 #3)、token 用量(#1)、预算告警(#2)。
- 定时后台体检 / 通知。

## 架构

- `src-tauri/src/doctor.rs` — 纯逻辑,home 参数化:
  - `DoctorCheck { id, title, status, detail, hint }`
    `status: ok | warn | error | info`(serde snake_case)
  - `DoctorReport { checks: Vec<DoctorCheck>, ran_at: RFC3339 }`
  - `local_checks(home, &Config, &[AgentStatus]) -> Vec<DoctorCheck>`
    (第 1–4 项,纯本地、全量单测)
  - `network_checks(&Config) -> Vec<DoctorCheck>`(第 5 项,async,tokio
    并发拨测,单项 8s 超时,失败隔离——网络不可用时整体降级为 info 跳过)
  - `backend_checks() -> Vec<DoctorCheck>`(第 6 项,复用
    `backends::entries()` 的 `gateway_status`,未安装后端跳过)
- `src-tauri/src/commands/doctor.rs` — `doctor_run()` 薄封装:加载
  config(读锁)、`agents::list_agent_status()`、拼三组 checks。
- `provider_test.rs` 重构:命令体抽出
  `pub async fn test_endpoint(base_url, api_key, flavor) -> ProviderTestResult`,
  命令与 doctor 共用。

## 体检项与判定

| id | 检查 | 判定 | hint |
|---|---|---|---|
| path | PATH 初始化 | ShellFailed → warn | "agent 可能被误报未安装;检查 shell 启动文件" |
| deps | 运行时依赖 | 任一已装 agent 的 missing_deps 非空 → error | 列出缺失项 |
| orphans | 绑定孤儿 | config.agent_providers 指向未安装 agent → warn | "安装该 agent 或在 Agents 页改绑" |
| drift | 配置漂移 | 绑定的 agent 服务商 state ∈ {outdated, removing} → warn | "可重新同步(有快照兜底)" |
| providers | Provider 连通性 | 每个启用 provider 的已配端点逐一拨测,失败 → error | "检查 Base URL / API key,可在 Providers 页重测" |
| gateways | 后端网关 | 已安装后端 gateway_status Err → warn | "openclaw/hermes 网关未运行或 CLI 异常" |

全部 ok 时各项仍输出 ok(用户要看到"查了什么"),不静默省略。
无 provider 配置 → providers 项输出 info「未配置」;离线(全部网络错误)
→ providers 项降级 info「网络不可达,已跳过」。

## 前端

- Agents 页头部加「体检」按钮(与刷新按钮并排)→ 页面顶部内联报告面板
  (glass-card,无弹窗,遵循项目惯例):按 status 着色的图标 + title +
  detail + hint,「重新体检」按钮。i18n en/zh 同步。

## 测试

- `local_checks` 用 TempHome 全覆盖:PATH 两态、deps 缺失、孤儿绑定、
  漂移(构造 managed 与实际文件不一致)。
- `test_endpoint` 抽取为纯移动,现有 provider_test 测试不动。
- network/backend checks 真实网络/CLI,不做单测,保持薄。

## 决策记录

- a) UI = Agents 页头部按钮 + 内联面板(已批准)
- b) v1 包含拨测:单项 8s 超时、并发、失败隔离、离线降级(已批准)
- c) 只读 + hint,不自动修复(已批准)
