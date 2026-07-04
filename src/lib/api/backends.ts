import { invoke } from '@tauri-apps/api/core';

export type BackendId = 'openclaw' | 'hermes';

export interface BackendInfo {
  id: BackendId;
  displayName: string;
  version: string;
  installed: boolean;
}

export async function list_backends(): Promise<BackendInfo[]> {
  try {
    const raw = await invoke<BackendInfo[]>('list_backends');
    return raw.map((b) => ({
      id: b.id as BackendId,
      displayName: b.displayName,
      version: b.version,
      installed: b.installed,
    }));
  } catch {
    return [];
  }
}