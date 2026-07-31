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
