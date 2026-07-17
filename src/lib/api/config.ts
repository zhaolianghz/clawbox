import { invoke } from '@tauri-apps/api/core';

// 与后端 ProviderSpec(serde camelCase)逐字段对齐,零转换。
export interface ModelProvider {
  id: string;
  name: string;
  apiKey: string;
  baseUrl: string;
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

/** 整表覆盖写入 ~/.clawbox/config.json 的 providers 节 */
export async function save_providers(providers: ModelProvider[]): Promise<void> {
  await invoke('config_providers_set', { providers });
}

/** 测试连接:GET 服务商 models 端点,返回延迟与模型列表。HTTP 失败也走 resolve(ok=false)。 */
export async function provider_test(
  baseUrl: string,
  apiKey: string,
  flavor: ProviderFlavor
): Promise<ProviderTestResult> {
  return await invoke<ProviderTestResult>('provider_test', { baseUrl, apiKey, flavor });
}
