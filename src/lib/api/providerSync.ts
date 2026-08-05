import { invoke } from '@tauri-apps/api/core';
import type { ApplyResult } from './mcpSync';

// 复用 MCP 同步的返回结构:后端绑定/重推返回同一组
// AgentPlan / ApplyResult(sync/mod.rs,字段 snake_case)。
export type { AgentPlan, ApplyResult, ChangeItem } from './mcpSync';

/** 单条已同步条目(与后端 agent_sync_overview 契约一致,snake_case) */
export interface SyncedItem {
  name: string;
  /** synced=已同步一致;unsynced=从未下发;outdated=下发过但配置已变;removing=将移除 */
  state: 'synced' | 'unsynced' | 'outdated' | 'removing';
}

/** 每个 agent 的同步总览:服务商 + MCP + 技能 + 记忆四个维度 */
export interface AgentSyncOverview {
  agent_id: string;
  provider_supported: boolean;
  mcp_supported: boolean;
  skills_supported: boolean;
  memory_supported: boolean;
  provider_config_path: string;
  /** CLI 管理型 agent(hermes/openclaw)为空串 */
  mcp_config_path: string;
  /** 技能软链目标目录 */
  skills_config_path: string;
  /** 记忆托管区块注入的指令文件路径 */
  memory_config_path: string;
  providers: SyncedItem[];
  mcp: SyncedItem[];
  skills: SyncedItem[];
  memory: SyncedItem[];
  provider_error: string | null;
  mcp_error: string | null;
  skills_error: string | null;
  memory_error: string | null;
}

/** 全部 agent 的同步总览(命令内部一次读全部 agent 配置文件,调用方做页面级缓存) */
export function agent_sync_overview(): Promise<AgentSyncOverview[]> {
  return invoke<AgentSyncOverview[]>('agent_sync_overview');
}

/** 绑定/切换/解绑该 agent 的服务商(选中即写入生效;null = 解绑恢复原状) */
export function agent_provider_bind(
  agentId: string,
  providerId: string | null
): Promise<ApplyResult> {
  return invoke<ApplyResult>('agent_provider_bind', { agentId, providerId });
}

/** 绑定表快照:agent_id → provider_id */
export function agent_providers_get(): Promise<Record<string, string>> {
  return invoke<Record<string, string>>('agent_providers_get');
}

/**
 * 设置该 agent 的 fallback 服务商链(有序;空数组 = 清空)。
 * 仅对原生支持 fallback 的 agent(目前仅 hermes)生效,其它 agent 会报错。
 * primary 不允许同时出现在 fallback 链里(后端会自动去重)。
 */
export function agent_fallbacks_set(
  agentId: string,
  fallbackIds: string[]
): Promise<ApplyResult> {
  return invoke<ApplyResult>('agent_fallbacks_set', { agentId, fallbackIds });
}

/** fallback 链快照:agent_id → 有序 provider_id[] */
export function agent_fallbacks_get(): Promise<Record<string, string[]>> {
  return invoke<Record<string, string[]>>('agent_fallbacks_get');
}
