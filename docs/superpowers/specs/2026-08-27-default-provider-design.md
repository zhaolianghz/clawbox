# 内置「官方默认」服务商(agent 配置初始化)

日期:2026-08-27 · 状态:已实现

## 问题

ClawBox 的 agent↔provider 绑定没有解绑入口(「一旦绑定即托管」)。用户想
把某个 agent 恢复到官方默认配置(如 codex 走 ChatGPT 登录)只能手改文件,
且下次同步会把托管配置写回去(见 2026-08-27 codex 恢复事故:手动清理后被
再次同步覆盖)。

## 方案

内置一个**虚拟服务商**「官方默认」,不落盘、不可编辑删除:

- id 哨兵 `__default__`,由 `config_providers_get` 动态注入,不进
  `config.json` 的 `providers` 数组;update/delete 拒绝该 id。
- agent 绑定它 = 触发既有「解绑路径」:apply 空 provider 集 →
  各适配器删掉自己下发的键(复用 `bind(None)` 语义),清
  `providers_managed` 记账;**但保留** `agent_providers[agent] = __default__`,
  UI 显式显示「官方默认」状态,后续同步幂等无操作。
- `plan_all` 认哨兵:绑定 default 且 managed 有残留 → 展示 remove 条目
  (防手改配置产生的悬空托管)。
- fallback 链不允许 default(不在 config.providers 里,天然拒入)。

## 不做

- 不做「恢复到出厂文件」(只删 ClawBox 下发的键,用户自有配置不动)——
  与解绑语义一致。
- Providers 管理页不展示这张虚拟卡(只在 agent 绑定选择器里出现)。

## 验证

- bind(默认) → agent 配置文件中我们下发的键被移除、绑定显示默认;
- resync 幂等;plan_all 哨兵 + 残留 managed → remove 条目;
- CRUD 守卫;i18n en/zh 同步。
