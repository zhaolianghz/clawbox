// 跨组件导航信号 — 让 agents 页头部「用量」按钮能切到 SettingsLayout 的 usage 段。
// 设计:共享 Svelte 5 store,任何组件写值,SettingsLayout 监听切换 section。
import { writable } from 'svelte/store';

export type SettingsSection =
  | 'providers'
  | 'mcp'
  | 'skills'
  | 'memory'
  | 'agents'
  | 'usage'
  | 'about';

/** 当 SettingsLayout 切换 section 时,新值落这里,供其它组件响应(高亮 active 按钮等)。 */
export const currentSection = writable<SettingsSection>('providers');

/** 写入即触发切换。SettingsLayout 订阅并执行跳转。 */
export const sectionRequest = writable<SettingsSection | null>(null);
