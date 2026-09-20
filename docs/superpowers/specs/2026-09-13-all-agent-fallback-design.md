# 全 agent 故障转移链(Fallback Chain for All Agents)设计

> 日期:2026-09-13
> 状态:待批准(用户已拍板 3 个决策点,待确认后开工)
> 来源:用户现场事故 —— 子 agent 调用失败,上游聚合网关报 `GLM-5.3-Flash 无可用渠道`

## 问题

用户的实际痛点不是「模型不行」,而是**上游服务商整体挂掉时,会话就死了**:

1. ClawBox 把服务商端点写进 agent 配置(如 `~/.claude/settings.json` 的
   `env.ANTHROPIC_BASE_URL`),该 env 在 **agent 启动时**注入进程环境变量。
2. 运行中的会话读不到新值 —— 改配置救不了已开的会话。
3. Claude Code 这类 agent **没有**切换 endpoint 的斜杠命令;
   `--fallback-model` 只切模型名、不切端点,救不了「服务商整体挂掉」。
4. 唯一的过渡手段是退出后 `claude --resume --model <B家模型名>`,要求用户
   手改配置 + 重开会话。

现状能力盘点:**全 16 个适配器条目里,只有 hermes 支持 fallback 链**
(`providers.rs:1646` 是唯一覆写 `supports_fallback()` 的实现,默认 `false`
见 `providers.rs:60`)。前端同样硬编码 `SUPPORTS_FALLBACK = new Set(['hermes'])`
(`src/routes/agents/+page.svelte:153`)。

## 目标

**12 个可见 agent 全部具备 fallback 链**:主服务商挂掉时自动切备用,
且**不丢会话上下文、不用重开会话、不用手改配置**。

## 非目标(v1 明确不做)

- **4 个占位 agent**(`cursor-agent` / `qodercli` / `qwen-code` / `trae-agent`)
  不介入。理由:它们走 `UnsupportedProviderAdapter`(`providers.rs:3494-3504`),
  用户已明确「不介入」,且已由 `PROVIDER_UNSUPPORTED_AGENTS`
  (`+page.svelte:40`,commit `0e8948c`)从 Agents 页隐藏。
- **通用 Anthropic ↔ OpenAI 双向协议翻译**。v1 只保证「链上成员提供服务入口
  所需的那个协议」,不提供就由 UI 拒绝入链。
- 跨服务商的**成本/配额调度**(按价格选路由)、按延迟选路由。
- 改写响应体里的 `model` 字段(见「模型名改写」)。
- 中继的远端/多机形态(只做 `127.0.0.1`)。
- Windows 服务安装器的签名/提权打磨(v1 可用任务计划或前台提示降级)。

## 已批准的决策

1. **实现方式 = 自己写(A 方案)**,纯 Rust,编译进 ClawBox。
   不引入 claude-code-router(node)/ LiteLLM(python)/ new-api(Docker)
   等外部运行时依赖。
2. **运行时形态 = sidecar + 系统常驻服务**(独立二进制 + LaunchAgent /
   systemd user unit / Windows 服务),而不是 GUI 子进程。
3. **v1 只做同协议透传**;协议翻译仅在 codex 必需处做降级
   (Responses → Chat,先试 `/v1/responses`,404 再翻译)。
4. 中继模式 **per-agent opt-in**,默认关闭,保持「只写配置」为默认原则。

## 调查结论(已实测,勿重做)

在动手写中继前,先排查「能否复用机器上已有的网关」,**结论是都不能**,记录备查:

| 候选 | 实测结论 |
|---|---|
| `hermes` api_server<br>`127.0.0.1:8642` | **不是透明代理**。OpenAI 兼容(`/v1/chat/completions` + `/v1/responses` + `/v1/models` 均在),但它把请求包装进 hermes 自己的 agent 系统提示 —— 实测一个 `"say ok"` 请求返回 `prompt_tokens: 29753`。用它当中继会污染所有 agent 的请求。❌ |
| `hermes` proxy<br>默认 `127.0.0.1:8645` | **是**真正的透明转发器(`hermes_cli/proxy/server.py`,298 行;通配路由 `/v1/{tail:.*}`,SSE 原样回传,源码注明 "does NOT mediate, log, transform, or rewrite request/response bodies")。但上游适配器只有 `nous` / `xai` 两家 OAuth 订阅制,且**单上游、无链、无故障转移、只能前台跑**。❌ |
| `openclaw` gateway<br>`ws://127.0.0.1:18789` | WebSocket **控制面**,不是 LLM API;且未安装(`~/.openclaw/openclaw.json` 不存在)。❌ |

**正面收获**:hermes gateway 的 launchd plist
(`~/Library/LaunchAgents/ai.hermes.gateway.plist`)是 sidecar 的现成模板
(`RunAtLoad` 登录自启 + `KeepAlive.SuccessfulExit=false` 崩溃自拉起 +
`StandardOutPath`)。本机端口占用现状:`8642`(hermes api_server)、
`8645`(hermes proxy 默认)、`8077`、`4709`、`9277`、`33331` —— **中继端口
必须动态协商并持久化,不能写死**。

## 架构

```
  agent(claude / codex / opencode / …)
        │  恒定指向 127.0.0.1:<relay_port>   ← 配置从此不再变
        ▼
  ┌──────────────────────────────────────────────┐
  │ ClawBox Relay (sidecar, 127.0.0.1)           │
  │  · token → agent_id                          │
  │  · 读 ~/.clawbox/config.json 解析链           │
  │  · 同协议流式透传 + 换 host/key/model         │
  │  · 失效转移 + 熔断 + 冷却                     │
  └──────────────────────────────────────────────┘
        │  按链顺序试
        ├──▶ chain[0]  ← 主
        ├──▶ chain[1]  ← 备用
        └──▶ chain[2]  ← …
```

中继**不代理 ClawBox 自己的运行**、不参与 UI —— 它是一个「按链转发」的
本地进程,由系统服务管理器保活。这确实与 `docs/ROADMAP.md` 开篇的架构原则
「ClawBox 只写配置、不代理运行时」冲突,该原则需要同步修订(见「ROADMAP 联动」)。

### 关键设计:虚拟 ProviderSpec,零适配器改动

**不给 16 个适配器的 `apply` 加参数**。中继模式下,ClawBox 在调用既有
`apply` 路径前合成一个**虚拟 `ProviderSpec`**:

```rust
ProviderSpec {
    id: "__relay__",
    name: "ClawBox Relay",
    api_key: relay_token_for(agent_id),      // clawbox-relay-<agent>-<rand>
    anthropic_base_url: Some("http://127.0.0.1:<port>"),
    openai_base_url:    Some("http://127.0.0.1:<port>"),
    default_model: <链首 default_model>,      // 保持会话模型名稳定
    models: <链首 models>,
    ..
}
```

然后走原有的 `apply(providers = vec![虚拟 spec], active_id = "__relay__", managed)`
路径。收益:

- **零适配器改动** —— 所有适配器只看见「一个普通服务商」,它们各自的
  `resolve_single_active` / `mapped()` / managed diff 逻辑全部照旧可用;
- 快照、备份、回滚、`validate_or_rollback` **自动覆盖**(走既有
  `touch_paths` + `apply_one` 路径);
- 关闭中继 = 用真实 spec 重放一次 apply,managed diff 自动清掉中继键。

链本身**不需要新数据结构**,直接由既有配置派生:

```
chain = [agent_providers[agent]] ++ agent_fallbacks[agent]
```

即**现有的 primary 绑定就是链首**,`agent_fallbacks`(`config.rs:174`)
就是链余。若 `agent_providers[agent] == "__default__"` 则不启用中继。

### 原生链 vs 中继

| 路径 | 适用 agent | 说明 |
|---|---|---|
| **原生链** | `hermes`(已实现)、`openclaw`(待实现,约 1 天) | agent 自己支持多服务商故障转移。openclaw 的 `agents.defaults.model` 可为 `{primary, fallbacks}`(`providers.rs:2443-2447` 注释)。**优先原生** —— 不依赖 ClawBox 进程。 |
| **中继链** | 其余 10 个可见 agent | 只能靠中继。 |

hermes/openclaw 默认走原生,v1 **不提供**「同一个 agent 选原生还是中继」的
开关(减少旋钮);确有需要再加 `Config.fallback_mode`。因此 v1 的
`fallback_mode` 字段**不新增**。

## 协议矩阵与 v1 范围

三个方言(协议 = 路径 + 请求体 JSON + SSE 事件格式,与厂商无关):

| 方言 | 入口路径 | primary 槽 |
|---|---|---|
| Anthropic Messages | `POST /v1/messages`(+`/v1/messages/count_tokens`) | `Slot::Anthropic` |
| OpenAI Chat Completions | `POST /v1/chat/completions` | `Slot::Openai` |
| OpenAI Responses | `POST /v1/responses` | `Slot::Openai`(codex 专用方言) |

12 个可见 agent 的入口方言与端点注入点:

| agent | 端点键 | 入口方言 | v1 |
|---|---|---|---|
| claude-code | `env.ANTHROPIC_BASE_URL` | Anthropic | 透传 |
| cline | `providers.json`(单激活) | Anthropic | 透传 |
| gemini | `GOOGLE_GEMINI_BASE_URL` | ⚠️ 待实测 | 待定 |
| codex | `model_providers.*.base_url`(`wire_api="responses"` 硬编码,`providers.rs:785`) | **Responses** | 透传 + 降级翻译 ⚠️见下 |
| codebuddy | `env.CODEBUDDY_BASE_URL` | Chat | 透传 |
| kimi | `providers.*.base_url` | Chat 优先 | 透传 |
| aider | `openai-api-base` / `anthropic-api-base` | 两者皆可 | 透传 |
| opencode | `provider.*.options.baseURL` | Chat 优先 | 透传 |
| openclaw | `models.providers.*.baseUrl` | 双槽 | **原生链** |
| pi | `providers.*.baseUrl` | Anthropic 优先 | 透传 |
| dsh | `llm-pi-ai.providers.clawbox.baseURL` | 双槽 | 透传 |
| hermes | `custom_providers[].base_url` | Anthropic 优先 | **原生链(已实现)** |

**为什么 v1 不需要通用翻译**:链上的 provider 多是聚合网关
(如 `api.lt4net.org`),**同时提供 Anthropic 与 OpenAI 端点**
(见 `ProviderSpec.anthropic_base_url` / `openai_base_url` 双字段设计)。
因此 claude-code 走 Anthropic 时是纯透传。入链时按
`AGENT_SLOTS`(`+page.svelte:311`)校验成员是否提供所需槽,不满足则拒绝入链
并给出明确文案。(注:`AGENT_SLOTS` 当前**缺 aider**,需一并补上。)

### 模型名改写

- 命中链首 → **不改写**,原样透传(`default_model` 已写入 agent 配置)。
- 命中备用成员 → 改写成该成员的 `default_model`。
- 响应体里的 `model` 字段**一律不改写** —— 真实模型名是成本归因的唯一正确
  来源(`src-tauri/src/usage/` 按 transcript 模型名计价),改写会让报表错价。

### 合成端点

- `GET /v1/models` —— 中继自行合成(部分 agent 启动时会探测)。
- 必须容忍 base_url 带/不带 `/v1`、`x-api-key` 与 `Authorization: Bearer`
  两种鉴权头、查询串透传,以及 `anthropic-version` / `anthropic-beta`
  头透传。

### ⚠️ codex 前置 bug(2026-09-13 已修)

2026-09-13 实测:ClawBox 给 codex 下发的自定义 provider **完全没有发凭据**,
导致 `401 Authentication Fails (governor)`。根因两处,缺一不可(已修,
见 `sync/providers.rs` 的 `CodexProviderAdapter`):

1. `providers.rs:785` 写 `[model_providers.clawbox]` 时**漏了
   `requires_openai_auth = true`** —— codex 只对自己的内置 OpenAI/ChatGPT
   provider 读 `auth.json`,自定义 provider 必须显式声明才会附加凭据。
2. `providers.rs:815-819` 注释写「只动 `OPENAI_API_KEY` 一个键」,
   **`auth_mode` 从未被写过**。需写 `"apikey"`,否则 codex 发的是 ChatGPT
   OAuth token(实测 `401 … api key: ****Meiw is invalid`)。

**解绑时必须还原 `auth_mode`**(已实现:只在值为我们写过的 `"apikey"` 时改回
`"chatgpt"`),否则残留 `apikey` + 一把第三方 key 会让用户
的 ChatGPT 登录失效(codex 会拿它去连 `api.openai.com`)。

不用 `env_key = "OPENAI_API_KEY"` 的原因:它指的是**环境变量**,而 ClawBox
不负责启动 codex(用户在终端自己敲),把 key 塞进 shell profile 会泄漏到所有
进程。`requires_openai_auth` + `auth.json` 才是配置管理器该走的路。

中继模式下这条链路照旧可用:中继地址写进 provider 块,`auth.json` 的
`OPENAI_API_KEY` 写中继 token。

## 失效转移状态机

| 上游结果 | 动作 |
|---|---|
| 连接失败 / DNS / TLS / 超时 | **转移** |
| `408` / `429` / `5xx` | **转移** |
| `401` / `403` | **转移**(该成员 key 失效) |
| `404` | **转移**(该成员不支持此路径/模型) |
| `400` / `422` | **不转移**,直接回给 agent(请求本身有问题) |
| 流式已吐首字节 | **不可转移**(已提交给 agent) |

- **熔断**:某成员连续 3 次失败 → 冷却 60s,冷却期内跳过。
- **429 原地重试**:带 `retry-after ≤ 2s` → 原地等一次再重试,不换成员。
- **请求体缓存**:必须整体缓存才能重放,上限 32MB;超限退化为不转移
  (直接透传,放弃该次故障转移能力)。
- **熔断状态放内存**(进程内),不落盘 —— 重启即重置是可接受的。

## 运行时形态(sidecar)

```
src-tauri/src/bin/clawbox-relay.rs   # sidecar 入口,复用 clawbox_lib::relay
src-tauri/src/relay/
  mod.rs         # RelayState、端口协商、token 表、配置热读
  server.rs      # HTTP 入口(通配 /v1/{*path})、鉴权、健康检查
  chain.rs       # 链解析、熔断/冷却状态机
  upstream.rs    # reqwest 流式转发
  translate.rs   # (v1 仅 codex)Responses → Chat 降级翻译
```

`src-tauri/Cargo.toml` 的 `[lib] name = "clawbox_lib"` 且 `crate-type` 含
`rlib`,故 `src/bin/*.rs` 可直接 `use clawbox_lib::relay`。

**依赖增量**:

- 新增 `axum`(HTTP 服务)、`tokio-stream`、`bytes`;
- `reqwest` 现有 `features = ["rustls-tls", "json"]` **需补 `"stream"`**
  (SSE 流式);
- `tokio` 现有 features **需补 `"net"`**(监听)和 `"signal"`(优雅退出);
- `tauri-plugin-shell` 已是依赖,但 `tauri.conf.json` 的
  `bundle.externalBin` 尚未配置 —— 需加上 sidecar 条目。

**端口协商**:首选持久化端口(`~/.clawbox/relay.json` 记录);若被占用,
向上试至多 20 个端口;拿到新端口后**必须触发所有中继模式 agent 重下发**
(否则 agent 配置指向死端口),并在 UI 提示。**不使用 port 0** —— 每次重启
端口都会变。

**保活**:macOS 照抄 hermes plist 结构(LaunchAgent,`RunAtLoad` +
`KeepAlive`);Linux 用 systemd user unit;Windows 用服务或任务计划。
ClawBox 退出**不影响**中继运行 —— 这正是选 α 而非 GUI 子进程的理由
(β 会在 ClawBox 崩溃时留下指向死端口的 agent 配置,且每次启停都污染快照)。

## 安全

- **只绑 `127.0.0.1`**,不监听外部接口。
- **中继持有真 key,给每个 agent 下发自签 token**
  (`clawbox-relay-<agent>-<random>`),token → agent_id 映射存
  `~/.clawbox/relay.json`(`0600`)。agent 配置里从此不再出现真 key ——
  命中 ROADMAP #5(安全审计)/ #6(key 保险箱)。
- **绝不记录请求/响应体**(内含用户源码),只记元数据:agent、链成员、
  状态码、耗时、是否转移。
- 无 token / 错 token → `401`,不泄漏链信息。
- `config.json` 缺失或损坏 → `503` + 明确错误文案(中继不猜测)。

> 顺带发现(非本设计范围但相关):`~/.hermes/config.yaml` 里
> `API_SERVER_KEY: hermes-session-key-2026` 为明文弱口令,同文件还有明文
> `MINIMAX_API_KEY`。这佐证了「key 集中管理」在引入中继后从附加功能变为必需品。

## 数据模型与接线

**复用(零改动)**:`Config.agent_fallbacks`(`config.rs:174`)、
`providers_fallback_managed`(`config.rs:179`)、`agent_fallbacks_set_at`
(`sync.rs:173`)、provider 删除级联清链(`config.rs:360`)、漂移重推
(`sync.rs:432`)、快照层 `"fallback"` 维度、前端链编辑器与拖拽排序。

**需改动的 3 处**:

1. `sync.rs:182` 的 `if !adapter.supports_fallback() { return Err(..) }`
   早退 → 按「原生 / 中继」分派。中继模式下不再要求
   `supports_fallback()`,改为要求「链上全部成员可部署到该 agent
   (即都提供该 agent 需要的槽)」。
2. apply 路径注入虚拟 `ProviderSpec`(见上),**不改适配器签名**。
3. `+page.svelte:153` 的 `SUPPORTS_FALLBACK` 改为动态(按 agent 的能力
   查询后端,而非前端硬编码集合)。

### trait 语义拆分

`ProviderAdapter` 新增两个语义明确的方法,**保留 `supports_fallback()`
作为兼容视图**(避免一次改爆 16 个适配器):

```rust
fn native_fallback(&self) -> bool { false }   // 原生支持,如 hermes/openclaw
fn relayable(&self) -> bool { false }          // 可中继
fn supports_fallback(&self) -> bool { self.native_fallback() || self.relayable() }
```

`relayable()` 的默认值仍需逐个显式声明 —— 12 个可见 agent 里除
`UnsupportedProviderAdapter` 外全部为 `true`。

## 前端

- 链编辑器(`+page.svelte` 已有拖拽排序)对 12 个 agent 全部开放;
  入链校验按 `AGENT_SLOTS` 检查成员是否提供所需槽(`AGENT_SLOTS` 补 aider)。
- 链编辑后提示生效范围:原生链「立即生效」;中继链「新请求立即生效,
  已发出请求不回滚」。
- 状态卡片:中继健康(`GET /health`)、端口、各 agent 链的当前熔断状态。
- 退出中继模式的路径必须显式:重放真实 spec → managed diff 清掉中继键 +
  恢复真 key。文案需说明「恢复后该 agent 不再有故障转移」。
- i18n `en.json` / `zh.json` 同步(仓库硬要求)。

## ROADMAP 联动

- `docs/ROADMAP.md` 开篇架构原则「只写配置、不代理运行时」需修订为
  「**默认只写配置**;故障转移中继是显式 opt-in 的例外」。
- ROADMAP 附录第 143 行曾否决「本地网关代理计量」,理由是「统计不到
  ClawBox 外的使用」—— 该理由**对 fallback 场景不成立**(fallback 的目的
  不是计量而是可用性),需在该条目下加注说明。
- 本设计与 ROADMAP #5(安全审计)、#6(key 保险箱)有协同,应在本文档
  批准后回写 ROADMAP。

## 分期

| 期 | 内容 | 产出 |
|---|---|---|
| **P0** | 中继核心:通配路由 + 鉴权 + 链解析 + 同协议流式透传 + 失效转移/熔断 | 假上游故障注入测试全绿 |
| **P1** | codex 的 Responses 入口(先试 `/v1/responses`,404 降级翻译)+ 合成 `GET /v1/models` | codex 可中继 |
| **P2** | 虚拟 ProviderSpec 接线(3 处改动)+ 前端动态能力查询 + `AGENT_SLOTS` 补 aider | 12 agent 可入链 |
| **P3** | sidecar 打包 + 三平台服务安装器 + 端口协商 + 退出中继还原路径 | 可发布 |
| **P4** | UI 状态卡片 / 熔断可视化 / i18n | 打磨 |

**优先级说明**:P0 可用本机已装 agent(openclaw / opencode / kimi / hermes /
cline / pi / dsh)端到端验证,不必等 codex 与 sidecar 打包。

## 待解决的实验(开工前)

1. **gemini 协议实测**:`GEMINI_SLOTS = [Slot::Anthropic]`(`providers.rs:1902`)
   但键名是 `GOOGLE_GEMINI_BASE_URL`,前端注释称「走 Gemini 协议,端点取
   Anthropic 槽的网关根 URL」(`+page.svelte:316`)。需实测 gemini-cli 是否
   发 `/v1beta/models/<model>:generateContent`(**模型名在路径而非 body**)。
   若是,则它既非 Anthropic 也非 OpenAI 方言 —— 需决定「v1 不支持 gemini」
   还是「专门加一条路径改写规则」。**这决定 §3.4 是否需要 gemini 分支。**
2. **codex 的 Responses 支持面**:已实测 **DeepSeek 支持 `/responses`**(200),
   故对这类 provider P1 是纯透传。但聚合网关未必都支持——「先试 `/v1/responses`,
   404 再降级翻译」需保留为兜底。另注:**codex 已彻底移除 Chat 协议**
   (二进制内有 `wire_api = "chat"` is no longer supported.),协议翻译只能做在
   中继侧,无法通过配置绕过。
3. **claude-code 容忍度**:确认它接受 `http://127.0.0.1:<port>` 作为
   `ANTHROPIC_BASE_URL`(预期可行 —— 第三方网关已在用,但需确认无 host 白名单)。

## 测试

- **假上游故障注入基建**(P0 前置):按指令返回 `500` / `429` / 超时 /
  中途断流,参考既有 `AlwaysInvalidAdapter` 模式(`providers.rs:4938`)。没有
  这个基建,P0 验收无法自动化。
- 单元:链解析(去重保序、primary 不当 fallback)、模型改写规则、失效转移
  判定矩阵、熔断冷却、token → agent 映射、32MB 上限退化行为。
- 集成:12 个 agent 各自在假上游 + 中继后跑一次真实请求,验证「成功」与
  「注入故障后成功转移」两条路径。
- 并发:N 路并行流式请求下互不串流、状态正确。
- 回归:`sync.rs:1361` `fallback_set_writes_chain_and_persists_and_clears`、
  `sync.rs:1396` `fallback_rejects_unsupported_agent_and_undeployable_provider`
  需按新语义更新;`providers.rs:5439` 的 12 agent id 测试同步扩展。
- 快照:中继模式 apply → 快照含中继端点 → 还原路径可回滚。

## 风险

| 风险 | 缓解 |
|---|---|
| 中继是新的单点:进程挂了所有 agent 都挂 | 系统服务保活 + 崩溃自拉起;`/health` 可视化;提供一键退出中继(回到直连) |
| 端口漂移导致 agent 配置失效 | 持久化端口 + 变更后强制重下发 + UI 提示 |
| 请求体缓存 32MB 上限被大上下文触发 → 退化为不转移 | 明确日志 + UI 提示「本次未启用故障转移」 |
| 中继持有全部真 key | 只绑 loopback + `0600` + token 隔离 + 不记 body;与 #6 key 保险箱合并推进 |
| 违反既有架构原则引发评审争议 | 已在本设计显式处理,并同步修订 ROADMAP 措辞 |
| codex 的 Responses 需求扩大范围 | P1 独立成期,可延后;P2 先覆盖其余 11 个 agent |
| hermes/openclaw 走原生链,行为与中继链不一致 | 两条路径的链数据同源(`agent_providers` + `agent_fallbacks`),UI 展示统一 |
