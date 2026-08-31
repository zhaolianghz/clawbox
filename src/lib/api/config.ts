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
  /**
   * 中转 model 名 → 官方 model id 别名映射。
   * 例: `{"route-gpt-4o": "gpt-4o", "teamorouter-claude-opus-4-1": "claude-opus-4-1"}`
   * 用途:让中转站 model 也能按官方价算 cost_usd。
   * 后端省略空 map 字段(只有 alias 才出现)。
   */
  modelAliases?: Record<string, string>;
  /** 该 provider 的价格覆盖/自定义。override > alias > builtin。 */
  pricing?: ProviderPricing;
  enabled: boolean;
}

/** 单个 provider 的价格配置。中转站 override 单价 + 别名映射。 */
export interface ProviderPricing {
  /** 中转名 → 官方 canonical model 名 */
  aliases: Record<string, string>;
  /** model → 自定义 ModelPrice(覆盖默认 builtin_prices) */
  overrides: Record<string, ModelPrice>;
}

/** 简化版 ModelPrice(用于 ProviderPricing.overrides) */
export interface ModelPrice {
  input: number;
  cacheRead: number | null;
  cacheCreation: number | null;
  output: number;
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
