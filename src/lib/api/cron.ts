import { invoke } from '@tauri-apps/api/core';
import type { BackendId } from './backends';

export interface CronJob {
  id: string;
  name: string;
  schedule: string;
  enabled: boolean;
  lastRun?: string;
  nextRun?: string;
  agent?: string;
  message?: string;
  [key: string]: unknown;
}

export interface TaggedCronJob {
  backend: BackendId;
  job: CronJob;
}

export interface BackendError {
  backend: BackendId;
  message: string;
}

export interface CronListAllResult {
  jobs: TaggedCronJob[];
  errors: BackendError[];
}

export interface NewCron {
  name: string;
  schedule: string;
  message?: string;
  agent?: string;
}

export async function list_cron_all(): Promise<CronListAllResult> {
  try {
    return await invoke<CronListAllResult>('cron_list_all');
  } catch {
    return { jobs: [], errors: [] };
  }
}

export async function add_cron(backend: BackendId, params: NewCron): Promise<void> {
  await invoke('cron_create', { backend, params });
}

export async function remove_cron(backend: BackendId, id: string): Promise<void> {
  await invoke('cron_remove', { backend, id });
}

export async function set_cron_enabled(backend: BackendId, id: string, enabled: boolean): Promise<void> {
  await invoke('cron_set_enabled', { backend, id, enabled });
}

export async function run_cron(backend: BackendId, id: string): Promise<void> {
  await invoke('cron_run', { backend, id });
}