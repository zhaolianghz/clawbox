import { invoke } from '@tauri-apps/api/core';

export interface ModelProvider {
  id: string;
  name: string;
  apiKey: string;
  baseUrl: string;
  defaultModel: string;
  enabled: boolean;
}

export interface Channel {
  id: string;
  name: string;
  endpoint: string;
  priority: number;
  loadBalance: 'round-robin' | 'weighted' | 'least-connections';
  enabled: boolean;
}

export interface Agent {
  id: string;
  name: string;
  systemPrompt: string;
  model: string;
  temperature: number;
  maxTokens: number;
  enabled: boolean;
}

export interface Skill {
  id: string;
  name: string;
  description: string;
  version: string;
  enabled: boolean;
  config: Record<string, unknown>;
}

export interface Config {
  providers: ModelProvider[];
  channels: Channel[];
  agents: Agent[];
  skills: Skill[];
}

const mockProviders: ModelProvider[] = [
  { id: '1', name: 'OpenAI', apiKey: 'sk-***', baseUrl: 'https://api.openai.com/v1', defaultModel: 'gpt-4', enabled: true },
  { id: '2', name: 'Anthropic', apiKey: 'sk-ant-***', baseUrl: 'https://api.anthropic.com', defaultModel: 'claude-3-opus', enabled: true },
  { id: '3', name: 'MiniMax', apiKey: '***', baseUrl: 'https://api.minimax.chat', defaultModel: 'abab6.5-chat', enabled: false },
];

const mockChannels: Channel[] = [
  { id: '1', name: 'Primary Channel', endpoint: 'https://api.openai.com/v1', priority: 1, loadBalance: 'round-robin', enabled: true },
  { id: '2', name: 'Backup Channel', endpoint: 'https://backup.example.com/v1', priority: 2, loadBalance: 'weighted', enabled: true },
];

const mockAgents: Agent[] = [
  { id: '1', name: 'General Assistant', systemPrompt: 'You are a helpful AI assistant.', model: 'gpt-4', temperature: 0.7, maxTokens: 4096, enabled: true },
  { id: '2', name: 'Code Helper', systemPrompt: 'You are an expert programmer.', model: 'claude-3-opus', temperature: 0.5, maxTokens: 8192, enabled: true },
  { id: '3', name: 'Creative Writer', systemPrompt: 'You are a creative writing assistant.', model: 'gpt-4', temperature: 0.9, maxTokens: 4096, enabled: false },
];

const mockSkills: Skill[] = [
  { id: '1', name: 'Web Search', description: 'Search the web for information', version: '1.0.0', enabled: true, config: {} },
  { id: '2', name: 'Code Interpreter', description: 'Execute and analyze code', version: '2.1.0', enabled: true, config: {} },
  { id: '3', name: 'Image Generator', description: 'Generate images from text', version: '1.5.0', enabled: false, config: {} },
  { id: '4', name: 'Document Parser', description: 'Parse and extract text from documents', version: '1.2.0', enabled: true, config: {} },
];

export async function get_config(): Promise<Config> {
  try {
    return await invoke<Config>('get_config');
  } catch {
    return {
      providers: mockProviders,
      channels: mockChannels,
      agents: mockAgents,
      skills: mockSkills,
    };
  }
}

export async function set_config(config: Config): Promise<void> {
  try {
    await invoke('set_config', { config });
  } catch (error) {
    console.warn('Config save failed (using mock):', error);
  }
}

export async function get_providers(): Promise<ModelProvider[]> {
  const config = await get_config();
  return config.providers;
}

export async function get_channels(): Promise<Channel[]> {
  const config = await get_config();
  return config.channels;
}

export async function get_agents(): Promise<Agent[]> {
  const config = await get_config();
  return config.agents;
}

export async function get_skills(): Promise<Skill[]> {
  const config = await get_config();
  return config.skills;
}
