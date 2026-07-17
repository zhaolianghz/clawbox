import { writable, derived, get } from 'svelte/store';
import { get_providers, save_providers, type ModelProvider } from '$lib/api/config';

export const providers = writable<ModelProvider[]>([]);

export const enabledProviders = derived(providers, ($providers) =>
  $providers.filter(p => p.enabled)
);

/** 从 ~/.clawbox/config.json 加载 providers 到 store */
export async function loadProviders(): Promise<void> {
  providers.set(await get_providers());
}

/** 落盘当前 store;失败时回滚到 prev 并抛出,让调用方展示错误 */
async function persist(prev: ModelProvider[]): Promise<void> {
  try {
    await save_providers(get(providers));
  } catch (e) {
    console.warn('save providers failed, rolling back:', e);
    providers.set(prev);
    throw e;
  }
}

export async function addProvider(provider: ModelProvider): Promise<void> {
  const prev = get(providers);
  providers.update(p => [...p, provider]);
  await persist(prev);
}

export async function updateProvider(id: string, data: Partial<ModelProvider>): Promise<void> {
  const prev = get(providers);
  providers.update(p => p.map(item => item.id === id ? { ...item, ...data } : item));
  await persist(prev);
}

export async function deleteProvider(id: string): Promise<void> {
  const prev = get(providers);
  providers.update(p => p.filter(item => item.id !== id));
  await persist(prev);
}
