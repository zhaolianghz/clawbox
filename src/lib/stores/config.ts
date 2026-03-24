import { writable, derived } from 'svelte/store';
import type { ModelProvider, Channel, Agent, Skill, Config } from '$lib/api/config';

export const providers = writable<ModelProvider[]>([]);
export const channels = writable<Channel[]>([]);
export const agents = writable<Agent[]>([]);
export const skills = writable<Skill[]>([]);
export const activeTab = writable<'models' | 'channels' | 'agents' | 'skills'>('models');
export const loading = writable(false);
export const editingItem = writable<ModelProvider | Channel | Agent | Skill | null>(null);
export const showModal = writable(false);

export const enabledProviders = derived(providers, ($providers) =>
  $providers.filter(p => p.enabled)
);

export const enabledChannels = derived(channels, ($channels) =>
  $channels.filter(c => c.enabled)
);

export const enabledAgents = derived(agents, ($agents) =>
  $agents.filter(a => a.enabled)
);

export const enabledSkills = derived(skills, ($skills) =>
  $skills.filter(s => s.enabled)
);

export function setConfig(config: Config) {
  providers.set(config.providers);
  channels.set(config.channels);
  agents.set(config.agents);
  skills.set(config.skills);
}

export function getConfig(): Config {
  let p: ModelProvider[] = [];
  let c: Channel[] = [];
  let a: Agent[] = [];
  let s: Skill[] = [];
  
  providers.subscribe(v => p = v)();
  channels.subscribe(v => c = v)();
  agents.subscribe(v => a = v)();
  skills.subscribe(v => s = v)();
  
  return { providers: p, channels: c, agents: a, skills: s };
}

export function addProvider(provider: ModelProvider) {
  providers.update(p => [...p, provider]);
}

export function updateProvider(id: string, data: Partial<ModelProvider>) {
  providers.update(p => p.map(item => item.id === id ? { ...item, ...data } : item));
}

export function deleteProvider(id: string) {
  providers.update(p => p.filter(item => item.id !== id));
}

export function addChannel(channel: Channel) {
  channels.update(c => [...c, channel]);
}

export function updateChannel(id: string, data: Partial<Channel>) {
  channels.update(c => c.map(item => item.id === id ? { ...item, ...data } : item));
}

export function deleteChannel(id: string) {
  channels.update(c => c.filter(item => item.id !== id));
}

export function addAgent(agent: Agent) {
  agents.update(a => [...a, agent]);
}

export function updateAgent(id: string, data: Partial<Agent>) {
  agents.update(a => a.map(item => item.id === id ? { ...item, ...data } : item));
}

export function deleteAgent(id: string) {
  agents.update(a => a.filter(item => item.id !== id));
}

export function toggleSkill(id: string) {
  skills.update(s => s.map(item => item.id === id ? { ...item, enabled: !item.enabled } : item));
}
