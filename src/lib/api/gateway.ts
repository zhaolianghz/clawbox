import { invoke } from '@tauri-apps/api/core';
import type { BackendId } from './backends';

export interface GatewayStatus {
  status: 'running' | 'stopped';
  version: string;
  pid?: number;
}

export interface TaggedGatewayStatus {
  backend: BackendId;
  status: GatewayStatus;
}

export async function list_gateway_statuses(): Promise<TaggedGatewayStatus[]> {
  try {
    const raw = await invoke<{ backend: string; status: GatewayStatus }[]>('gateway_status_all');
    return raw.map((r) => ({
      backend: r.backend as BackendId,
      status: r.status,
    }));
  } catch {
    return [];
  }
}

export async function start_gateway(backend: BackendId): Promise<string> {
  return await invoke<string>('gateway_start', { backend });
}

export async function stop_gateway(backend: BackendId): Promise<string> {
  return await invoke<string>('gateway_stop', { backend });
}