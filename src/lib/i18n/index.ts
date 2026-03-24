import { init, register } from 'svelte-i18n';

register('en', () => import('./en.json'));
register('zh', () => import('./zh.json'));

init({
  fallbackLocale: 'en',
  initialLocale: navigator.language.startsWith('zh') ? 'zh' : 'en',
});
