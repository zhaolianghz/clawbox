import { invoke } from '@tauri-apps/api/core';

// 配置快照(与后端 sync::snapshots 一致,字段 snake_case)。

export interface SnapshotInfo {
  id: string;
  agent_id: string;
  /** "provider" | "fallback" | "mcp" | "skills" | "memory" */
  scope: string;
  summary: string;
  /** false = CLI 型下发,无本地文件,恢复需人工 */
  restorable: boolean;
  created_at: string;
  files: number;
}

export interface RestoreResult {
  /** 恢复的 home 相对路径清单 */
  restored: string[];
  /** 清掉的托管记账字段名 */
  cleared: string[];
}

export function snapshots_list(agentId?: string | null): Promise<SnapshotInfo[]> {
  return invoke<SnapshotInfo[]>('snapshots_list', { agentId: agentId ?? null });
}

export function snapshots_restore(agentId: string, snapshotId: string): Promise<RestoreResult> {
  return invoke<RestoreResult>('snapshots_restore', { agentId, snapshotId });
}
