import { invoke } from '@tauri-apps/api/core';

export interface GatewayStatus {
  running: boolean;
  version: string;
  uptime?: number;
}

export async function get_gateway_status(): Promise<GatewayStatus> {
  try {
    return await invoke<GatewayStatus>('get_gateway_status');
  } catch {
    return { running: false, version: 'unknown' };
  }
}

export async function start_gateway(): Promise<void> {
  await invoke('start_gateway');
}

export async function stop_gateway(): Promise<void> {
  await invoke('stop_gateway');
}

export async function restart_gateway(): Promise<void> {
  await invoke('stop_gateway');
  await invoke('start_gateway');
}
