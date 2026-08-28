import { writable, derived, get } from 'svelte/store';
import { get_providers, save_providers, DEFAULT_PROVIDER_ID, type ApplyResult, type ModelProvider } from '$lib/api/config';

export const providers = writable<ModelProvider[]>([]);

export const enabledProviders = derived(providers, ($providers) =>
  $providers.filter(p => p.enabled)
);

/** 从 ~/.clawbox/config.json 加载 providers 到 store */
export async function loadProviders(): Promise<void> {
  providers.set(await get_providers());
}

/** 落盘当前 store;失败时回滚到 prev 并抛出,让调用方展示错误。成功时透传后端自动重推结果 */
async function persist(prev: ModelProvider[]): Promise<ApplyResult[]> {
  try {
    // 虚拟「官方默认」条目不随整表回传(后端也会兜底剥离)
    return await save_providers(get(providers).filter(p => p.id !== DEFAULT_PROVIDER_ID));
  } catch (e) {
    console.warn('save providers failed, rolling back:', e);
    providers.set(prev);
    throw e;
  }
}

export async function addProvider(provider: ModelProvider): Promise<ApplyResult[]> {
  const prev = get(providers);
  providers.update(p => [...p, provider]);
  return persist(prev);
}

export async function updateProvider(id: string, data: Partial<ModelProvider>): Promise<ApplyResult[]> {
  const prev = get(providers);
  providers.update(p => p.map(item => item.id === id ? { ...item, ...data } : item));
  return persist(prev);
}

export async function deleteProvider(id: string): Promise<ApplyResult[]> {
  const prev = get(providers);
  providers.update(p => p.filter(item => item.id !== id));
  return persist(prev);
}
