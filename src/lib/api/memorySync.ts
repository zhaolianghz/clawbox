import { invoke } from '@tauri-apps/api/core';
import type { AgentPlan, ApplyResult } from './mcpSync';

// 统一指令记忆:真源 = ~/.agents/memory/MEMORY.md,以托管区块注入各 agent
// 指令文件(CLAUDE.md / AGENTS.md 等),区块外内容绝不触碰。
// 计划/应用复用 MCP/服务商/技能同款 AgentPlan / ApplyResult(snake_case)。
export type { AgentPlan, ApplyResult, ChangeItem } from './mcpSync';

/** 单个 agent 的指令文件注入目标 */
export interface MemoryTarget {
  agent_id: string;
  /** 指令文件路径(如 ~/.claude/CLAUDE.md) */
  path: string;
  exists: boolean;
  /** 文件里已有 ClawBox 托管区块 */
  has_block: boolean;
  /** 托管区块之外的内容字符数(0 = 整个文件都是我们管理的) */
  outside_chars: number;
}

/** 读统一记忆库内容(~/.agents/memory/MEMORY.md;不存在返回空串) */
export function memory_read(): Promise<string> {
  return invoke<string>('memory_read');
}

/** 写统一记忆库内容 */
export function memory_write(content: string): Promise<void> {
  return invoke<void>('memory_write', { content });
}

/** 各 agent 指令文件的注入目标状态 */
export function memory_targets(): Promise<MemoryTarget[]> {
  return invoke<MemoryTarget[]>('memory_targets');
}

/** 读取某 agent 指令文件全文(只读查看/导入到库) */
export function memory_target_content(agentId: string): Promise<string> {
  return invoke<string>('memory_target_content', { agentId });
}

/** 计算记忆注入到各 agent 的计划(只读) */
export function sync_memory_plan(): Promise<AgentPlan[]> {
  return invoke<AgentPlan[]>('sync_memory_plan');
}

/** 对选中 agent 应用记忆注入(托管区块写入) */
export function sync_memory_apply(agentIds: string[]): Promise<ApplyResult[]> {
  return invoke<ApplyResult[]>('sync_memory_apply', { agentIds });
}
