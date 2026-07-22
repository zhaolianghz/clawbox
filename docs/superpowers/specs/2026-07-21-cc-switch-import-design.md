# cc-switch 服务商配置一键导入 设计

**日期**: 2026-07-21
**状态**: 待用户认可
**参考**: [cc-switch](https://github.com/farion1231/cc-switch)(farion1231/cc-switch,官网 ccswitch.io)

## 背景与目标

大量用户从 cc-switch 迁移到 ClawBox,手动把每家服务商的 name/端点/API key 重新填一遍
成本高。本设计新增 **「从 cc-switch 导入」** 功能:一键把 cc-switch 里已配置的服务商
批量导入 ClawBox 的服务商列表,按 host 智能合并、与现有配置去重,用户预览勾选后落盘。

## 现状核对(2026-07-21 本机实测)

- **cc-switch 存储分两个时代**:v3.7.0 起改用 SQLite(`~/.cc-switch/cc-switch.db`)作为
  唯一数据源(SSOT);旧版才用 `~/.cc-switch/config.json`。当前版本还支持
  「导出 JSON」(Settings → 数据管理 → 导出)。
- **本机实测**:`~/.cc-switch/` 下只有 `cc-switch.db`(20.6M)+ `settings.json`,
  **没有** `config.json`,`backups/` 里全是 `.db` 备份。证明当前版本用户手上不存在
  config.json,文件兜底(导出 JSON)是他们的实际路径;自动探测覆盖旧版用户。
- **cc-switch `Provider` 结构**(源码 `src-tauri/src/provider.rs` + `src/types.ts` 确认):
  `{ id, name, settingsConfig, websiteUrl?, category?, notes?, icon?, iconColor?, ... }`。
  其中 `settingsConfig` 是「该 app 的原生配置对象」,各 app_type 结构不同(下详)。
- **ClawBox 服务商模型**(`ProviderSpec` / `ModelProvider`):单条**双端点**——
  `anthropicBaseUrl` + `openaiBaseUrl` 两个槽,至少填一个;外加 `apiKey`、`defaultModel`、
  `models[]`、`enabled`。API key **明文存** `~/.clawbox/config.json`,无加密/keychain。
- **可用工具**:`@tauri-apps/plugin-dialog`(文件选择器)、`toml_edit`(解析 Codex TOML)
  均已是依赖,无需新增。**不引入 `rusqlite`**(用户已否决直接读 SQLite 方案)。

## 设计决策(已与用户确认)

1. **数据来源**:自动探测 `~/.cc-switch/config.json`,找不到 → 弹文件选择器让用户选
   cc-switch 导出的 JSON。(否决了直接读 SQLite。)
2. **映射策略**:智能合并——按归一化 host 把同一家的 anthropic 端条目与 openai 端条目
   合成一条双端点 ClawBox provider。
3. **导入范围**:**全部 app_type,不 skip**(含 gemini)。
4. **合并到已有 provider 时**:只填**空槽 / 补空 key**,**不覆盖**用户已填好的字段。
5. **文件权限加固(0o600)**:本次**不做**,维持与现状一致(现状即明文普通权限),
   列为将来可选跟进项,避免本功能范围膨胀。

## 第 1 节:数据来源与探测

前端点击「从 cc-switch 导入」→ 调后端 `cc_switch_import_preview(path: None)`:

1. 后端先探测 `~/.cc-switch/config.json`(用 `dirs::home_dir()` 拼接)。存在 → 读它。
2. 不存在 → 后端返回一个「未找到,请选择文件」的可区分结果(如 `Err` 带专用错误码
   或 `enum` 变体)。前端据此用 `plugin-dialog` 的 `open()` 弹文件选择器
   (`.json` 过滤),拿到路径后再调 `cc_switch_import_preview(path: Some(picked))`。

解析器写成**内容驱动、容错**:不假设精确顶层包装。递归遍历 JSON,凡是「带
`settingsConfig`(或等价的配置对象)且带 `name`」的对象即视为一条 provider 候选,
并从其**父级 key** 推断 `app_type`(`claude` / `codex` / `gemini` / …)。这样旧版
`config.json`(`{providers:{claude:{...},codex:{...}}}` 或 `{providers:{claude:{providers:
{id:P},current}}}`)与导出 JSON 两种外层包装都能吃下。app_type 拿不到时,再靠
**内容嗅探**兜底(见第 2 节末)。

## 第 2 节:app_type → 端点协议映射

各 app_type 的 `settingsConfig` 结构(2026-07-21 直接读本机 cc-switch.db 得到,值已脱敏):

| cc-switch app_type | 端点 URL 取值 | API key 取值 | 默认模型 | 落入 ClawBox 槽 |
|---|---|---|---|---|
| `claude` / `claude-desktop` | `env.ANTHROPIC_BASE_URL` | `env.ANTHROPIC_AUTH_TOKEN` ‖ `env.ANTHROPIC_API_KEY` | `env.ANTHROPIC_MODEL` | **anthropic 槽** |
| `codex` | 解析 `config`(TOML)取 `[model_providers.*].base_url` | `auth.OPENAI_API_KEY` | TOML `model` | **openai 槽** |
| `opencode` | `options.baseURL` | `options.apiKey` | `models` 的首个 key | **openai 槽** |
| `hermes` | `base_url` | `api_key` | `models[0]` | 按 `api_mode` 定槽(含 "anthropic" → anthropic,否则 openai) |
| `gemini` | `env.GOOGLE_GEMINI_BASE_URL` ‖ `env.GEMINI_BASE_URL` | `env.GEMINI_API_KEY` | `env.GEMINI_MODEL` | **openai 槽** |

> gemini 落 openai 槽是刻意为之:ClawBox 目录里 Gemini 卡片的 `apiHost` =
> `generativelanguage.googleapis.com/...`,导入后 host 对得上,会正确显示成
> 「合并到已有 Gemini」而非野条目。

**Codex TOML 提取细节**(用 `toml_edit`):
- 端点:优先读顶层 `model_provider = "<name>"` → 查 `[model_providers.<name>].base_url`;
  取不到则取**第一个** `[model_providers.*].base_url`;都没有(如 OpenAI 官方直连)→
  openai 槽留空,该条目仅在能合并进别的槽时才有意义(见第 3 节)。
- 模型:顶层 `model`。
- key 用 `auth.OPENAI_API_KEY`(cc-switch 把真实 key 放这里;TOML 里多为 env 引用)。

**内容嗅探兜底**(app_type 缺失或未知时):`settingsConfig` 里出现
`env.ANTHROPIC_BASE_URL` → 判为 anthropic;出现 `auth.OPENAI_API_KEY` / `options.baseURL`
/ `env.GOOGLE_GEMINI_BASE_URL` → 判为 openai。

**跳过条件**:端点 URL 与 key 都取不到的条目静默跳过(如本机那条空 gemini
`{env:{},config:{}}`),不进预览列表。

## 第 3 节:智能合并 + 与现有配置去重

抽取阶段产出一批 `Extracted { host, protocol(anthropic|openai), url, apiKey, model, name, website, appType }`。

**① 按 host 合并**:以归一化 host(`new URL(url).host`)分组。同组内:
- anthropic 协议的条目 → 填 `anthropicBaseUrl` + 相应 key;
- openai 协议的条目 → 填 `openaiBaseUrl` + 相应 key;
- 一组产出**一条**候选 ClawBox provider(name 取组内首个非空;website 同理;
  defaultModel 取首个非空 model)。
- 若同组两个槽的 key 不同,保留各自 slot 的 key 无法表达(ClawBox 单 provider 只有一个
  `apiKey`)——策略:**anthropic 槽的 key 优先**,openai 槽 key 作为兜底(与现有
  `fetchSlot()` 的 anthropic 优先语义一致)。此细节在预览里对用户透明(显示最终 key 掩码)。

**② 与现有 ClawBox providers 去重**:对每条合并候选,按 host 在现有 `providers` 里找
(anthropic 或 openai 任一槽 host 命中)。
- **命中** → 标记 `merge`,目标 = 那条现有 provider。合并时**只填空槽 / 补空 key**
  (现有 `anthropicBaseUrl` 已非空则不动;`apiKey` 已非空则不动),**绝不覆盖**已填字段。
- **未命中** → 标记 `add`,生成新 `id`(`crypto.randomUUID()` 在前端 apply 时给)。

## 第 4 节:前后端分工

**后端** 新增 `src-tauri/src/commands/cc_switch.rs`,注册进 `commands/mod.rs` + `lib.rs`
的 `invoke_handler`:

```rust
/// 一条导入候选(已按 host 合并;去重/落盘由前端做)
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportCandidate {
    pub name: String,
    pub anthropic_base_url: String,   // 空 = 无该槽
    pub openai_base_url: String,
    pub api_key: String,
    pub default_model: String,
    pub website: String,              // 便于前端展示,可空
    pub source_apps: Vec<String>,     // 来源 app_type,预览里展示「来自 claude+codex」
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ImportPreview {
    Found { candidates: Vec<ImportCandidate> },
    NeedFile,   // 未探测到 config.json,前端应弹文件选择器
}

#[tauri::command]
pub async fn cc_switch_import_preview(path: Option<String>) -> Result<ImportPreview, String>;
```

- 解析(JSON 遍历)、Codex TOML 解析、host 合并全在后端。
- 后端**只产出候选**,不碰 `~/.clawbox/config.json`——去重命中判断与落盘留给前端,
  复用既有 `providers` store + `save_providers`,持久化逻辑不新增第二套。

**前端**:
- `src/lib/api/ccSwitch.ts`:`cc_switch_import_preview(path?)` 的 `invoke` 封装 +
  `ImportCandidate` 类型(与后端 camelCase 对齐)。
- providers 页 `+page.svelte`:导入入口 + 预览面板 + 勾选 + apply。apply 时对每条候选按
  host 在 `$providers` 里查:命中则 `updateProvider`(只填空槽/补空 key),未命中则
  `addProvider({ id: crypto.randomUUID(), ... })`。全部走现有 store 动作。

## 第 5 节:UI

页头「同步到 Agent」按钮旁新增 **「从 cc-switch 导入」** 按钮。点击流程:
1. 调 `cc_switch_import_preview(undefined)`。
2. 返回 `NeedFile` → `plugin-dialog` `open({ filters:[{name:'JSON',extensions:['json']}] })`;
   用户取消则静默收场;选了文件 → `cc_switch_import_preview(path)`。
3. 拿到 `candidates` → 展开**内联预览面板**(复用现有 `.sync-panel` 视觉语言,无弹窗):
   - 每条:name、端点徽章(Anthropic/OpenAI,复用 `.endpoint-chip`)、key 掩码、
     `新增` / `合并到已有 X` 标签、`来自 claude+codex` 来源小字、复选框。
   - 顶部全选;底部「导入选中 (N)」+「取消」。
   - 候选为空 → 面板显示空态文案(如「cc-switch 里没有可导入的服务商」)。
4. 确认 → 逐条 apply → 成功后关面板、刷新卡片同步状态、触发既有「去同步」快捷入口。
   单条失败行内红字,不中断其余。

Esc 关闭预览面板(对齐现有 editor/sync 面板的 Esc 行为)。

## 第 6 节:i18n

`src/lib/i18n/zh.json` / `en.json` 新增 `providers.import.*`:
`button`(从 cc-switch 导入 / Import from cc-switch)、`previewTitle`、`selectFile`、
`badgeAdd`(新增)、`badgeMerge`(合并到已有 {name})、`sourceFrom`(来自 {apps})、
`confirm`(导入选中 ({count}))、`empty`(没有可导入的服务商)、`notFoundHint`
(未找到 cc-switch 配置,请选择导出的 JSON)、各类错误文案。

## 第 7 节:测试

**后端单测**(`cc_switch.rs` 内 `#[cfg(test)]`,喂内嵌 JSON 字符串,不碰真实 home):
- claude 条目 → anthropic 槽 + key 从 `ANTHROPIC_AUTH_TOKEN` 取(且 `ANTHROPIC_API_KEY`
  兜底路径)。
- codex 条目 → openai 槽,base_url 从 `[model_providers.*].base_url` 提取,model 从顶层
  `model`,key 从 `auth.OPENAI_API_KEY`。
- opencode / hermes / gemini 各一条 → 落对应槽。
- **host 合并**:同 host 的 claude+codex 两条 → 合成一条双端点候选。
- **两种外层包装**(旧 config.json 嵌套 vs 扁平)都能解析出相同候选(容错性)。
- 空/损坏 JSON、空 gemini 条目 → 不 panic、跳过或返回可读错误。
- 断言:测试快照里 key 用假值(`sk-test-*`),不含真实密钥。

**前端**:apply 的合并逻辑(命中已有只填空槽 / 未命中新增)靠类型 + 手动走查覆盖;
`npm run check` 通过。

## 非目标 / 明确不做

- 不引入 `rusqlite`、不直接读 `cc-switch.db`。
- 不做配置文件 0o600 权限加固(维持现状,列为将来可选)。
- 不导入 cc-switch 的 MCP servers / skills / prompts(本次只做**服务商**;可另起功能)。
- 不做反向导出(ClawBox → cc-switch)。
- 合并时不覆盖用户已填字段;不删除现有 provider。
