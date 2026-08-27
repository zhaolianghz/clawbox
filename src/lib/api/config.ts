import { invoke } from '@tauri-apps/api/core';
import type { ApplyResult } from './mcpSync';

// save_providers 现在返回后端自动重推的逐 agent 结果,类型与 MCP 同步共用。
export type { ApplyResult } from './mcpSync';

// 与后端 ProviderSpec(serde camelCase)逐字段对齐,零转换。
// 双端点契约:旧 baseUrl/flavor 已废弃 —— 后端 load 时自动迁移进槽位,
// 序列化不再输出,前端类型里不保留。
/** 内置「官方默认」虚拟服务商 id(后端注入,绑定它=恢复 agent 官方默认) */
export const DEFAULT_PROVIDER_ID = '__default__';

export interface ModelProvider {
  id: string;
  name: string;
  apiKey: string;
  /** Anthropic 兼容端点;未配置为空串。与 openaiBaseUrl 至少一个非空 */
  anthropicBaseUrl: string;
  /** OpenAI 兼容端点;未配置为空串 */
  openaiBaseUrl: string;
  defaultModel: string;
  /** 已配置的模型 id 列表(旧配置无此字段,后端 serde default 补空数组) */
  models: string[];
  enabled: boolean;
}

// 与后端 ProviderTestResult(serde camelCase)对齐。
export interface ProviderTestResult {
  ok: boolean;
  latencyMs: number;
  /** 成功时拉取到的模型 id 列表(可能为空) */
  models: string[];
  /** 失败时的简短英文原因,原样展示 */
  error: string | null;
}

export type ProviderFlavor = 'openai' | 'anthropic';

export async function get_providers(): Promise<ModelProvider[]> {
  return await invoke<ModelProvider[]>('config_providers_get');
}

/** 整表覆盖写入 ~/.clawbox/config.json 的 providers 节;返回编辑后自动重推的逐 agent 结果 */
export async function save_providers(providers: ModelProvider[]): Promise<ApplyResult[]> {
  return await invoke<ApplyResult[]>('config_providers_set', { providers });
}

/** 测试连接:GET 服务商 models 端点,返回延迟与模型列表。HTTP 失败也走 resolve(ok=false)。 */
export async function provider_test(
  baseUrl: string,
  apiKey: string,
  flavor: ProviderFlavor
): Promise<ProviderTestResult> {
  return await invoke<ProviderTestResult>('provider_test', { baseUrl, apiKey, flavor });
}
