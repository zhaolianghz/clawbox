import { invoke } from '@tauri-apps/api/core';
import type { AgentPlan, ApplyResult } from './mcpSync';

// 复用 MCP 同步的返回结构:后端 sync_providers_plan/apply 返回同一组
// AgentPlan / ApplyResult(sync/mod.rs,字段 snake_case)。
export type { AgentPlan, ApplyResult, ChangeItem } from './mcpSync';

/** 计算服务商配置到各 agent 的同步计划(只读,不写任何文件) */
export function sync_providers_plan(): Promise<AgentPlan[]> {
  return invoke<AgentPlan[]>('sync_providers_plan');
}

/** 对勾选的 agent 逐个应用服务商配置(后端逐个备份、写入、汇报) */
export function sync_providers_apply(agentIds: string[]): Promise<ApplyResult[]> {
  return invoke<ApplyResult[]>('sync_providers_apply', { agentIds });
}

/** 每个服务商的同步状态(与后端 sync_providers_status 契约一致,snake_case) */
export interface ProviderSyncStatus {
  provider_id: string;
  /** 已同步一致的 agent */
  synced_agents: string[];
  /** 有待下发变更的 agent */
  pending_agents: string[];
}

/** 各服务商的同步状态(只读;命令内部读各 agent 配置文件,可能 Err) */
export function sync_providers_status(): Promise<ProviderSyncStatus[]> {
  return invoke<ProviderSyncStatus[]>('sync_providers_status');
}

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

/** 读取激活(默认)服务商 id;未选择时为 null */
export function config_active_provider_get(): Promise<string | null> {
  return invoke<string | null>('config_active_provider_get');
}

/** 设置激活(默认)服务商;传 null 取消。未知 id 后端会报错 */
export function config_active_provider_set(id: string | null): Promise<void> {
  return invoke<void>('config_active_provider_set', { id });
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
