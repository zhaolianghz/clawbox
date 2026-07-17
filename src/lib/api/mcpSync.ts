import { invoke } from '@tauri-apps/api/core';

/** MCP 服务器定义(与后端 McpServerSpec 一致,字段 snake_case) */
export interface McpServerSpec {
  kind: 'stdio' | 'http';
  command?: string | null;
  args: string[];
  env: Record<string, string>;
  url?: string | null;
  headers: Record<string, string>;
  enabled: boolean;
}

/** 单条变更明细 */
export interface ChangeItem {
  name: string;
  action: 'add' | 'update' | 'remove' | 'unchanged' | 'skip';
  detail: string;
}

/** 每个 agent 的同步计划 */
export interface AgentPlan {
  agent_id: string;
  supported: boolean;
  config_path: string;
  changes: ChangeItem[];
  error: string | null;
}

/** 每个 agent 的应用结果 */
export interface ApplyResult {
  agent_id: string;
  ok: boolean;
  backup_path: string | null;
  applied: number;
  error: string | null;
}

export function config_mcp_list(): Promise<Record<string, McpServerSpec>> {
  return invoke<Record<string, McpServerSpec>>('config_mcp_list');
}

export function config_mcp_upsert(name: string, spec: McpServerSpec): Promise<void> {
  return invoke<void>('config_mcp_upsert', { name, spec });
}

export function config_mcp_remove(name: string): Promise<void> {
  return invoke<void>('config_mcp_remove', { name });
}

export function sync_mcp_plan(): Promise<AgentPlan[]> {
  return invoke<AgentPlan[]>('sync_mcp_plan');
}

export function sync_mcp_apply(agentIds: string[]): Promise<ApplyResult[]> {
  return invoke<ApplyResult[]>('sync_mcp_apply', { agentIds });
}
