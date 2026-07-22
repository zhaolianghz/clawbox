import { get, writable } from 'svelte/store';
import { getCurrentWindow } from '@tauri-apps/api/window';

// 主题(皮肤)切换:跟随系统 / 浅色 / 深色。
// 偏好存 localStorage('clawbox.theme'),启动时在 main.ts(mount 前)应用,避免首帧闪烁。
// 与 i18n locale 的持久化模式一致。

export type ThemeChoice = 'system' | 'light' | 'dark';
type Resolved = 'light' | 'dark';

const STORAGE_KEY = 'clawbox.theme';

/** 读持久化选择;非法/缺失兜底 'system' */
function initialChoice(): ThemeChoice {
  try {
    const s = localStorage.getItem(STORAGE_KEY);
    if (s === 'system' || s === 'light' || s === 'dark') return s;
  } catch {
    /* localStorage 不可用 */
  }
  return 'system';
}

/** 用户当前选择(system|light|dark),UI 切换器绑定它 */
export const themeChoice = writable<ThemeChoice>(initialChoice());

function systemPrefersDark(): boolean {
  try {
    return window.matchMedia('(prefers-color-scheme: dark)').matches;
  } catch {
    return true; // 探测不到时按深色(应用默认外观)
  }
}

/** 把选择解析成实际明暗:system → 跟随系统,否则原值 */
function resolve(choice: ThemeChoice): Resolved {
  if (choice === 'system') return systemPrefersDark() ? 'dark' : 'light';
  return choice;
}

// 原生窗口背景色(RGB),需与 app.css 的 --bg-primary 保持一致。
// macOS 透明标题栏(titleBarStyle: Transparent)区域透出的是原生窗口背景,
// CSS 盖不到它 —— 必须运行时按主题同步,否则浅色下顶部标题栏常驻深色。
const WINDOW_BG: Record<Resolved, [number, number, number]> = {
  light: [246, 247, 249], // #f6f7f9
  dark: [10, 10, 15], // #0a0a0f
};

/** 应用到 <html>:data-theme 驱动 CSS 变量,color-scheme 让原生控件跟随明暗 */
function apply(choice: ThemeChoice): void {
  const r = resolve(choice);
  const el = document.documentElement;
  el.setAttribute('data-theme', r);
  el.style.colorScheme = r;
  // 原生窗口背景随主题走(见 WINDOW_BG 注释)。非 Tauri 环境/权限缺失时静默失败。
  try {
    void getCurrentWindow().setBackgroundColor(WINDOW_BG[r]).catch(() => {});
  } catch {
    /* 非 Tauri 环境 */
  }
}

/** 用户切换:更新 store + 持久化 + 立即应用 */
export function setTheme(choice: ThemeChoice): void {
  themeChoice.set(choice);
  try {
    localStorage.setItem(STORAGE_KEY, choice);
  } catch {
    /* 忽略:存不了下次仍会走系统/默认 */
  }
  apply(choice);
}

let mediaListenerAttached = false;

/** 启动初始化:应用初始选择,并挂系统明暗监听(仅「跟随系统」时联动) */
export function initTheme(): void {
  const choice = initialChoice();
  themeChoice.set(choice);
  apply(choice);
  if (mediaListenerAttached) return;
  try {
    const mq = window.matchMedia('(prefers-color-scheme: dark)');
    mq.addEventListener('change', () => {
      if (get(themeChoice) === 'system') apply('system');
    });
    mediaListenerAttached = true;
  } catch {
    /* matchMedia 不可用 */
  }
}
