// 配置导入/导出(issue #2)的 typed invoke 封装。
import { invoke } from '@tauri-apps/api/core';

export interface TransferItem {
  name: string;
  /** "add" | "merge" | "overwrite" | "skip" */
  action: string;
  detail: string;
}

export interface TransferPreview {
  providers: TransferItem[];
  mcp: TransferItem[];
  skills: TransferItem[];
}

export interface TransferPicks {
  providers: string[];
  mcp: string[];
  skills: string[];
}

export interface TransferOutcome {
  providersAdded: number;
  providersMerged: number;
  mcpApplied: number;
  skillsInstalled: number;
  errors: string[];
}

export function transfer_export(
  path: string,
  providerIds: string[],
  includeKeys: boolean,
  includeMcp: boolean,
  skillNames: string[]
): Promise<void> {
  return invoke('transfer_export', { path, providerIds, includeKeys, includeMcp, skillNames });
}

export function transfer_import_preview(path: string): Promise<TransferPreview> {
  return invoke<TransferPreview>('transfer_import_preview', { path });
}

export function transfer_import_apply(path: string, picks: TransferPicks): Promise<TransferOutcome> {
  return invoke<TransferOutcome>('transfer_import_apply', { path, picks });
}
