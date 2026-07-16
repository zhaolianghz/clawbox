import { invoke } from '@tauri-apps/api/core';

export type AgentKind = 'native_cli' | 'acp_bridge' | 'runtime' | 'gateway';

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
