<script lang="ts">
  import { locale } from 'svelte-i18n';
  import type { ProviderCatalogEntry } from '$lib/data/providers';
  import { localize } from '$lib/data/localized';
  import { PROVIDER_ICONS } from '$lib/data/providerIcons';

  let { entry }: { entry: ProviderCatalogEntry } = $props();

  // lobe-icons 真实品牌 logo(构建期内联的静态资产,{@html} 内容不含用户输入,
  // 无注入风险);单色版 svg 用 currentColor,外层品牌色上色。未命中回退首字母。
  const icon = $derived(PROVIDER_ICONS[entry.id]);
</script>

<span
  class="provider-logo"
  style="--brand: {entry.color}; background: color-mix(in srgb, {entry.color} 16%, transparent); color: {entry.color};"
  aria-hidden="true"
>
  {#if icon}
    <span class="brand-icon">{@html icon}</span>
  {:else}
    <span class="initial">{localize(entry.name, $locale).slice(0, 1)}</span>
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
  .brand-icon {
    width: 26px;
    height: 26px;
    display: inline-flex;
  }
  .brand-icon :global(svg) {
    width: 100%;
    height: 100%;
  }
  .initial {
    font-size: 1.35rem;
    font-weight: 700;
    line-height: 1;
  }
</style>
