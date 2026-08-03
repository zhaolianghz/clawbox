import "./app.css";
import "./themes/theme-cyberpunk.css";
import "./themes/theme-minimal.css";
import "./themes/theme-liquid-glass.css";
import "./lib/i18n/index.ts";
import { initTheme } from "./lib/theme";
import { mount } from 'svelte';
import App from "./App.svelte";

// mount 前应用主题,首帧即带正确 data-theme,避免深→浅闪烁
initTheme();

mount(App, {
  target: document.getElementById("app")!,
});

// 窗口 visible:false 创建(tauri.conf.json),等首帧画上主题后再显示,消除
// WKWebView 默认白底在 HTML 渲染前露出的启动白/黑闪。
// 注意不能用 requestAnimationFrame:隐藏窗口不触发渲染回调,show 永远不执行。
// 60ms 足够完成首次布局绘制;非 Tauri 环境静默跳过。
setTimeout(async () => {
  try {
    const { getCurrentWindow } = await import('@tauri-apps/api/window');
    const win = getCurrentWindow();
    await win.show();
    await win.setFocus();
  } catch {
    /* 非 Tauri 环境(vite 浏览器调试) */
  }
}, 60);
