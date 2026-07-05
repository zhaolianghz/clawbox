import { invoke } from '@tauri-apps/api/core';
import type { BackendId } from '../backends';
import type { TaggedListResult } from './_shared';

export interface McpServer {
  name: string;
  transport: string;
  status: string;
}

export async function list_mcp_all(): Promise<TaggedListResult<McpServer>> {
  try {
    return await invoke<TaggedListResult<McpServer>>('mcp_list_all');
  } catch {
    return { items: [], errors: [] };
  }
}

export async function add_mcp(backend: BackendId, name: string, config_json: string): Promise<void> {
  await invoke('mcp_add', { backend, name, configJson: config_json });
}

export async function remove_mcp(backend: BackendId, name: string): Promise<void> {
  await invoke('mcp_remove', { backend, name });
}