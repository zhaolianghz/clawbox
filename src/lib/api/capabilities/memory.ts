import { invoke } from '@tauri-apps/api/core';
import type { BackendId } from '../backends';
import type { TaggedListResult } from './_shared';

export interface MemoryStatus {
  provider: string;
  builtinActive: boolean;
}

export async function list_memory_all(): Promise<TaggedListResult<MemoryStatus>> {
  try {
    return await invoke<TaggedListResult<MemoryStatus>>('memory_status_all');
  } catch {
    return { items: [], errors: [] };
  }
}

export async function memory_index(backend: BackendId): Promise<void> {
  await invoke('memory_index', { backend });
}

export async function memory_reset(backend: BackendId): Promise<void> {
  await invoke('memory_reset', { backend });
}