import { invoke } from '@tauri-apps/api/core';
import type { BackendId } from '../backends';
import type { TaggedListResult } from './_shared';

export interface Tool {
  id: string;
  enabled: boolean;
}

export async function list_tools_all(): Promise<TaggedListResult<Tool>> {
  try {
    return await invoke<TaggedListResult<Tool>>('tools_list_all');
  } catch {
    return { items: [], errors: [] };
  }
}

export async function set_tool_enabled(backend: BackendId, id: string, enabled: boolean): Promise<void> {
  await invoke('tools_set_enabled', { backend, id, enabled });
}