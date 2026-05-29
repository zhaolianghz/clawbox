import { invoke } from '@tauri-apps/api/core';
import { APP_CONFIG } from '../config';

// Types aligned with Rust GatewayStatus
export interface GatewayStatus {
  status: 'running' | 'stopped' | 'unknown';
  version: string;
  pid: number | null;
}

export interface SystemCheck {
  nodejs: ComponentStatus;
  openclaw: ComponentStatus;
  platform: string;
  is_china: boolean;
}

export interface ComponentStatus {
  installed: boolean;
  version: string | null;
}

// Gateway port from centralized config
const GATEWAY_PORT = APP_CONFIG.gateway.port;

export async function get_gateway_status(): Promise<GatewayStatus> {
  try {
    return await invoke<GatewayStatus>('get_gateway_status');
  } catch {
    return { status: 'unknown', version: 'unknown', pid: null };
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

export async function check_system(): Promise<SystemCheck> {
  return invoke<SystemCheck>('check_system');
}
