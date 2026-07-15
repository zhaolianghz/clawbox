import { addMessages, init } from 'svelte-i18n';
import en from './en.json';
import zh from './zh.json';

// addMessages is synchronous — dictionaries are available before any component
// renders. register() + locale.set() loaded dictionaries asynchronously, which
// raced component mount: $_() threw "Cannot format a message without first
// setting the initial locale", aborting page mounts (spinners never resolved).
addMessages('en', en);
addMessages('zh', zh);

init({
  fallbackLocale: 'en',
  initialLocale: navigator.language.startsWith('zh') ? 'zh' : 'en',
});
