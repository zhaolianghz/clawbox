import { invoke } from '@tauri-apps/api/core';

export interface AdapterInfo {
  id: string;
  label: string;
  installed: boolean;
  version: string | null;
  install_hint: string;
}

// serde: ReviewScope is either "whole_project" (unit) or { git_diff: { base } }.
// serde(rename_all="snake_case") on an enum with a struct variant serializes
// the unit variant as the string "whole_project".
export type ReviewScope = 'whole_project' | { git_diff: { base: string } };

export interface RoleAssignment {
  adapter_id: string;
  model: string | null;
}

export type Severity = 'info' | 'warning' | 'error';

export interface Finding {
  file: string;
  line: number | null;
  severity: Severity;
  title: string;
  detail: string;
  reviewer: string;
}

export type ReviewStatus =
  | { state: 'running' }
  | { state: 'completed' }
  | { state: 'failed'; message: string };

export interface ReviewReport {
  task_id: string;
  findings: Finding[];
  summary: string;
  status: ReviewStatus;
  created_at: number;
}

export async function acp_list_adapters(): Promise<AdapterInfo[]> {
  try {
    return await invoke<AdapterInfo[]>('acp_list_adapters');
  } catch {
    return [];
  }
}

export async function acp_install_adapter(id: string): Promise<string> {
  return await invoke<string>('acp_install_adapter', { id });
}

export async function review_run(
  projectPath: string,
  scope: ReviewScope,
  reviewers: RoleAssignment[],
  summarizer: RoleAssignment
): Promise<ReviewReport> {
  return await invoke<ReviewReport>('review_run', {
    projectPath,
    scope,
    reviewers,
    summarizer,
  });
}

export async function review_list(): Promise<ReviewReport[]> {
  try {
    return await invoke<ReviewReport[]>('review_list');
  } catch {
    return [];
  }
}

export async function review_get(taskId: string): Promise<ReviewReport> {
  return await invoke<ReviewReport>('review_get', { taskId });
}
