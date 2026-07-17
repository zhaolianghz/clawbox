<script lang="ts">
  import type { ProviderCatalogEntry } from '$lib/data/providers';

  let { entry }: { entry: ProviderCatalogEntry } = $props();
</script>

<span
  class="provider-logo"
  style="--brand: {entry.color}; background: color-mix(in srgb, {entry.color} 16%, transparent); color: {entry.color};"
  aria-hidden="true"
>
  {#if entry.id === 'openai'}
    <!-- OpenAI 花瓣 -->
    <svg viewBox="0 0 24 24" fill="currentColor"><path d="M22.28 9.82a5.94 5.94 0 0 0-.51-4.91 6 6 0 0 0-6.5-2.93A6 6 0 0 0 4.98 4.1a5.94 5.94 0 0 0-3.98 2.9 6 6 0 0 0 .74 7.05 5.94 5.94 0 0 0 .51 4.91 6 6 0 0 0 6.5 2.93 6 6 0 0 0 4.51 2.02 6 6 0 0 0 5.74-4.2 5.94 5.94 0 0 0 3.98-2.9 6 6 0 0 0-.7-7.01zM13.26 21.1a4.44 4.44 0 0 1-2.85-1.03l.14-.08 4.74-2.74a.77.77 0 0 0 .39-.67v-6.68l2 1.16v5.55a4.47 4.47 0 0 1-4.42 4.49zm-9.55-4.08a4.44 4.44 0 0 1-.53-3l.14.08 4.75 2.74a.77.77 0 0 0 .77 0l5.79-3.34v2.31l-4.81 2.78a4.47 4.47 0 0 1-6.11-1.57zM2.36 9.39a4.44 4.44 0 0 1 2.32-1.96v5.64a.77.77 0 0 0 .39.67l5.77 3.33-2 1.16-4.8-2.77a4.47 4.47 0 0 1-1.68-6.07zm16.43 3.82L13 9.87l2-1.15 4.8 2.77a4.47 4.47 0 0 1-.67 8.07v-5.64a.77.77 0 0 0-.34-.71zm1.99-3-.14-.09-4.74-2.75a.77.77 0 0 0-.78 0L9.33 10.7V8.39l4.8-2.77a4.47 4.47 0 0 1 6.65 4.6zM8.31 14.78l-2-1.16V8.07a4.47 4.47 0 0 1 7.34-3.43l-.14.08-4.74 2.74a.77.77 0 0 0-.39.67zm1.09-2.34 2.57-1.48 2.57 1.48v2.96L11.97 17l-2.57-1.48z"/></svg>
  {:else if entry.id === 'anthropic'}
    <!-- Claude 星芒 -->
    <svg viewBox="0 0 24 24" fill="currentColor"><path d="M12 2l1.8 6.2L20 6.5l-4.4 4.6 5.6 2.9-6.3.7 1.7 6.1-4.6-4.4-4.6 4.4 1.7-6.1-6.3-.7 5.6-2.9L4 6.5l6.2 1.7z"/></svg>
  {:else if entry.id === 'gemini'}
    <!-- Gemini 菱形星 -->
    <svg viewBox="0 0 24 24" fill="currentColor"><path d="M12 2c.4 4.6 2.4 6.6 7 7-4.6.4-6.6 2.4-7 7-.4-4.6-2.4-6.6-7-7 4.6-.4 6.6-2.4 7-7z"/></svg>
  {:else if entry.id === 'deepseek'}
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M3 12c3-6 9-6 12 0 2 4 5 4 6 0"/><path d="M3 7c3-3 9-3 12 3 2 4 5 4 6 0"/></svg>
  {:else if entry.id === 'ollama'}
    <!-- Ollama 羊驼脸 -->
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"><path d="M12 3c-4 0-6 3-6 7 0 3 1 5 2 6 1 1 2 2 4 2s3-1 4-2c1-1 2-3 2-6 0-4-2-7-6-7z"/><circle cx="9.5" cy="11" r="1" fill="currentColor"/><circle cx="14.5" cy="11" r="1" fill="currentColor"/></svg>
  {:else}
    <span class="initial">{entry.name.slice(0, 1)}</span>
  {/if}
</span>

<style>
  .provider-logo {
    width: 48px;
    height: 48px;
    border-radius: 14px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
    border: 1px solid color-mix(in srgb, var(--brand) 30%, transparent);
  }
  .provider-logo svg {
    width: 26px;
    height: 26px;
  }
  .initial {
    font-size: 1.35rem;
    font-weight: 700;
    line-height: 1;
  }
</style>
