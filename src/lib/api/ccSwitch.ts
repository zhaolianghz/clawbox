import { invoke } from '@tauri-apps/api/core';

// 与后端 ImportCandidate(serde camelCase)对齐,零转换。
export interface ImportCandidate {
  name: string;
  /** Anthropic 兼容端点;空 = 无该槽 */
  anthropicBaseUrl: string;
  /** OpenAI 兼容端点;空 = 无该槽 */
  openaiBaseUrl: string;
  apiKey: string;
  defaultModel: string;
  website: string;
  /** 来源 app_type(去重排序),预览展示「来自 claude+codex」 */
  sourceApps: string[];
}

// 后端 ImportPreview 是内部 tag = "kind" 的枚举:
//   { kind: 'found', candidates: [...] } | { kind: 'needFile' }
export type ImportPreview =
  | { kind: 'found'; candidates: ImportCandidate[] }
  | { kind: 'needFile' };

/**
 * 探测 ~/.cc-switch/config.json(path 省略时),或解析指定的 cc-switch 导出 JSON。
 * 未探测到配置时返回 { kind: 'needFile' },由前端弹文件选择器再带 path 重试。
 */
export async function cc_switch_import_preview(path?: string): Promise<ImportPreview> {
  return await invoke<ImportPreview>('cc_switch_import_preview', { path: path ?? null });
}
