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

export interface BackendError {
  backend: BackendId;
  message: string;
}

export interface GatewayStatusAllResult {
  statuses: TaggedGatewayStatus[];
  errors: BackendError[];
}

export async function list_gateway_statuses(): Promise<GatewayStatusAllResult> {
  try {
    const raw = await invoke<GatewayStatusAllResult>('gateway_status_all');
    return raw;
  } catch {
    return { statuses: [], errors: [] };
  }
}

export async function start_gateway(backend: BackendId): Promise<string> {
  return await invoke<string>('gateway_start', { backend });
}

export async function stop_gateway(backend: BackendId): Promise<string> {
  return await invoke<string>('gateway_stop', { backend });
}