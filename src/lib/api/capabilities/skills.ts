import { invoke } from '@tauri-apps/api/core';
import type { BackendId } from '../backends';
import type { TaggedListResult } from './_shared';

export interface Skill {
  id: string;
  name: string;
  version: string;
  description: string;
  enabled: boolean;
}

export async function list_skills_all(): Promise<TaggedListResult<Skill>> {
  try {
    return await invoke<TaggedListResult<Skill>>('skills_list_all');
  } catch {
    return { items: [], errors: [] };
  }
}

export async function install_skill(backend: BackendId, id: string): Promise<void> {
  await invoke('skills_install', { backend, id });
}

export async function uninstall_skill(backend: BackendId, id: string): Promise<void> {
  await invoke('skills_uninstall', { backend, id });
}

export async function set_skill_enabled(backend: BackendId, id: string, enabled: boolean): Promise<void> {
  await invoke('skills_set_enabled', { backend, id, enabled });
}