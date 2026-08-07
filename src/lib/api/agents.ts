import { invoke } from '@tauri-apps/api/core';

export type AgentKind = 'native_cli' | 'runtime' | 'gateway';

export interface AgentStatus {
  id: string;
  label: string;
  kind: AgentKind;
  installed: boolean;
  version: string | null;
  deps_satisfied: boolean;
  missing_deps: string[];
  install_command: string | null;
  docs_url: string | null;
}

export function agents_list(): Promise<AgentStatus[]> {
  return invoke<AgentStatus[]>('agents_list');
}

export function agent_install(id: string): Promise<string> {
  return invoke<string>('agent_install', { id });
}

/**
 * Whether PATH resolution at startup recovered the interactive-shell PATH
 * (`shell_resolved`) or degraded to well-known dirs + GUI PATH (`shell_failed`).
 * Drives the agents-page warning banner: when degraded, an installed agent may
 * read "Not installed" because its binary isn't on the minimal GUI PATH.
 * (GH#3: Claude Code imported from cc-switch but shown as missing.)
 */
export type PathInitStatus = 'shell_resolved' | 'shell_failed';

export function path_env_status(): Promise<PathInitStatus> {
  return invoke<PathInitStatus>('path_env_status');
}
