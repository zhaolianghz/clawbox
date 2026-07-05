import { invoke } from '@tauri-apps/api/core';
import type { BackendId } from '../backends';
import type { TaggedListResult } from './_shared';

export interface Plugin {
  id: string;
  name: string;
  version: string;
  enabled: boolean;
}

export async function list_plugins_all(): Promise<TaggedListResult<Plugin>> {
  try {
    return await invoke<TaggedListResult<Plugin>>('plugins_list_all');
  } catch {
    return { items: [], errors: [] };
  }
}

export async function install_plugin(backend: BackendId, source: string): Promise<void> {
  await invoke('plugins_install', { backend, source });
}

export async function remove_plugin(backend: BackendId, id: string): Promise<void> {
  await invoke('plugins_remove', { backend, id });
}

export async function set_plugin_enabled(backend: BackendId, id: string, enabled: boolean): Promise<void> {
  await invoke('plugins_set_enabled', { backend, id, enabled });
}