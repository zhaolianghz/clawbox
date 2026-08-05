# 配置导入 / 导出设计（issue #2）

日期：2026-08-05 · 状态：已批准（用户拍板：三块内容各自勾选；熟人分享场景，不做加密档）

## 目标

把本机 ClawBox 的服务商 / MCP 服务器 / 技能来源打包成单个 JSON 文件，另一台 ClawBox 导入后经预览确认入库。解决"朋友间共享一整套配置要逐条手填"的痛点（issue #2）。

## 范围

**导出（三块独立勾选，默认全选）**
- 服务商：name、anthropicBaseUrl、openaiBaseUrl、apiKey（可整体剥离）、defaultModel、models、enabled
- MCP 服务器：完整 McpServerSpec（stdio/http 均可）
- 技能：安装来源 `{repo, subdir}`（不导文件本体与 commit——导入方从 git 装最新版）

**不导出**：agent↔服务商绑定、*_managed 内部状态、主题/语言偏好、内部 UUID。

**不做（v1）**：云端同步（文件即同步载体，格式留版本号备将来复用）；口令加密（熟人分享场景，明文+警告足够）。

## 文件格式

扩展名 `.clawbox.json`，明文可读：

```json
{
  "clawboxExport": 1,
  "exportedAt": "2026-08-05T10:00:00Z",
  "providers": [ { "name": "...", "anthropicBaseUrl": "...", "openaiBaseUrl": "...", "apiKey": "sk-... 或空串", "defaultModel": "...", "models": [], "enabled": true } ],
  "mcpServers": { "<名>": { "kind": "stdio|http", "command": "...", "args": [], "env": {}, "url": null, "headers": {}, "enabled": true } },
  "skills": { "<技能名>": { "repo": "...", "subdir": "..." } }
}
```

- 三个 section 均可缺省（按导出勾选）。
- 导入遇到未知 `clawboxExport` 版本 → 明确报"文件来自更新版本的 ClawBox,请升级"。
- 服务商不带内部 id,导入方重新生成 UUID。

## API Key 两档

- **含密钥**（默认）：导出面板红字警告"文件包含 API 密钥,仅发送给信任的人"。
- **不含密钥**：apiKey 置空串导出;导入后该服务商编辑面板补 key。

## 导入语义

- 预览面板逐条展示,标注「新增」/「合并到已有 X」,逐条勾选(默认全勾),复用 cc-switch 导入的 UI 骨架与判重口径:
  - 服务商:任一端点 URL 与已有条目相同 → 合并(空字段补齐,非空不覆盖;与 cc_switch.rs 现行为一致)
  - MCP:同名且内容相同 → 跳过;同名内容不同 → 标注「覆盖」;新名 → 新增
  - 技能:库内已有同名 → 跳过;否则走 skills_repo_install(克隆需数秒,预览面板提示)
- 应用成功后出现现有「去同步」快捷条。

## 实现

- Rust 新模块 `commands/transfer.rs`:
  - `config_export(path, provider_ids, include_keys, include_mcp, skill_names) -> Result<(), String>`
  - `config_import_preview(path) -> ImportPreview`(纯函数解析+判重,home 参数化可测)
  - `config_import_apply(path, picks) -> ImportOutcome`(技能项内部调 skills 安装)
- 前端:providers 页工具条「导入」「导出」;`lib/api/transfer.ts`;导出选择面板 + 导入预览面板(内联,无弹窗,与全站一致)
- 测试:roundtrip、判重合并、剥离密钥、坏 JSON/未知版本报错、技能已存在跳过

## 入口位置

服务商页头工具条(与「从 cc-switch 导入」并排)。MCP/技能虽也在导出范围,但入口统一放服务商页——分享场景的心智起点是"把我的服务商给你"。
