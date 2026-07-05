import { invoke } from '@tauri-apps/api/core';
import type { BackendId } from '../backends';
import type { TaggedListResult } from './_shared';

export interface Hook {
  id: string;
  name: string;
  event: string;
  enabled: boolean;
}

export async function list_hooks_all(): Promise<TaggedListResult<Hook>> {
  try {
    return await invoke<TaggedListResult<Hook>>('hooks_list_all');
  } catch {
    return { items: [], errors: [] };
  }
}

export async function set_hook_enabled(backend: BackendId, id: string, enabled: boolean): Promise<void> {
  await invoke('hooks_set_enabled', { backend, id, enabled });
}