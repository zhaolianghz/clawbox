import { addMessages, init } from 'svelte-i18n';
import en from './en.json';
import zh from './zh.json';

// addMessages is synchronous — dictionaries are available before any component
// renders. register() + locale.set() loaded dictionaries asynchronously, which
// raced component mount: $_() threw "Cannot format a message without first
// setting the initial locale", aborting page mounts (spinners never resolved).
addMessages('en', en);
addMessages('zh', zh);

/** 初始语言:localStorage 持久化值 → 系统语言前缀 → 'en';非法值一律兜底 'en'。 */
function initialLocale(): 'en' | 'zh' {
  try {
    const saved = localStorage.getItem('clawbox.locale');
    if (saved === 'en' || saved === 'zh') return saved;
  } catch { /* 存储不可用时走系统语言 */ }
  return navigator.language?.startsWith('zh') ? 'zh' : 'en';
}

init({
  fallbackLocale: 'en',
  initialLocale: initialLocale(),
});
