import "./app.css";
import "./lib/i18n/index.ts";
import { mount } from 'svelte';
import App from "./App.svelte";

mount(App, {
  target: document.getElementById("app")!,
});
