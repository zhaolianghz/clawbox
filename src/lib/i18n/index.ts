import { register, init, locale } from 'svelte-i18n';
import en from './en.json';
import zh from './zh.json';

register('en', () => Promise.resolve(en));
register('zh', () => Promise.resolve(zh));

init({
  fallbackLocale: 'en',
  initialLocale: 'en',
});

locale.set(navigator.language.startsWith('zh') ? 'zh' : 'en');
