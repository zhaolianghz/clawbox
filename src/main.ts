import "./app.css";
import "./lib/i18n/index.ts";
import { initTheme } from "./lib/theme";
import { mount } from 'svelte';
import App from "./App.svelte";

// mount 前应用主题,首帧即带正确 data-theme,避免深→浅闪烁
initTheme();

mount(App, {
  target: document.getElementById("app")!,
});
