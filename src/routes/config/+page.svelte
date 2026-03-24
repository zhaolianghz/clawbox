<script lang="ts">
  import { _ } from 'svelte-i18n';
  import ConfigTabs from '$lib/components/Config/ConfigTabs.svelte';
  import ModelConfig from '$lib/components/Config/ModelConfig.svelte';
  import ChannelConfig from '$lib/components/Config/ChannelConfig.svelte';
  import AgentConfig from '$lib/components/Config/AgentConfig.svelte';
  import SkillConfig from '$lib/components/Config/SkillConfig.svelte';
  import { activeTab, setConfig, loading } from '$lib/stores/config';
  import { get_config, set_config } from '$lib/api/config';
  
  async function loadConfig() {
    loading.set(true);
    try {
      const config = await get_config();
      setConfig(config);
    } finally {
      loading.set(false);
    }
  }
  
  async function saveConfig() {
    const { getConfig } = await import('$lib/stores/config');
    await set_config(getConfig());
  }
  
  loadConfig();
</script>

<svelte:head>
  <title>Config | ClawBox</title>
</svelte:head>

<div class="config-page">
  <div class="page-header">
    <h1>{$_('nav.config')}</h1>
    <button class="neon-button" onclick={saveConfig} disabled={$loading}>
      {$_('config.save')}
    </button>
  </div>
  
  <ConfigTabs />
  
  <div class="config-content">
    {#if $loading}
      <div class="loading">Loading...</div>
    {:else}
      {#if $activeTab === 'models'}
        <ModelConfig />
      {:else if $activeTab === 'channels'}
        <ChannelConfig />
      {:else if $activeTab === 'agents'}
        <AgentConfig />
      {:else if $activeTab === 'skills'}
        <SkillConfig />
      {/if}
    {/if}
  </div>
</div>

<style>
  .config-page {
    padding: 2rem;
    height: 100%;
    overflow-y: auto;
  }
  
  .page-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 1.5rem;
  }
  
  .page-header h1 {
    margin: 0;
    color: var(--text-primary);
    font-size: 1.5rem;
  }
  
  .config-content {
    background: var(--bg-secondary);
    border-radius: 0.75rem;
    padding: 1.5rem;
  }
  
  .loading {
    text-align: center;
    padding: 3rem;
    color: var(--text-secondary);
  }
</style>
