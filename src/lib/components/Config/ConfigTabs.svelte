<script lang="ts">
  import { _ } from 'svelte-i18n';
  import { activeTab } from '$lib/stores/config';
  
  const tabs = [
    { id: 'models' as const, label: 'config.models' },
    { id: 'channels' as const, label: 'config.channels' },
    { id: 'agents' as const, label: 'config.agents' },
    { id: 'skills' as const, label: 'config.skills' },
  ];
  
  function setTab(id: 'models' | 'channels' | 'agents' | 'skills') {
    activeTab.set(id);
  }
</script>

<div class="config-tabs">
  {#each tabs as tab}
    <button
      class="tab-button"
      class:active={$activeTab === tab.id}
      onclick={() => setTab(tab.id)}
    >
      {$_(tab.label)}
    </button>
  {/each}
</div>

<style>
  .config-tabs {
    display: flex;
    gap: 0.5rem;
    padding: 0.5rem;
    background: var(--bg-secondary);
    border-radius: 0.75rem;
    margin-bottom: 1.5rem;
  }
  
  .tab-button {
    flex: 1;
    padding: 0.75rem 1rem;
    background: transparent;
    border: none;
    border-radius: 0.5rem;
    color: var(--text-secondary);
    font-size: 0.9rem;
    cursor: pointer;
    transition: all 0.2s ease;
  }
  
  .tab-button:hover {
    color: var(--text-primary);
    background: var(--bg-tertiary);
  }
  
  .tab-button.active {
    color: var(--neon-cyan);
    background: rgba(0, 245, 255, 0.1);
    box-shadow: var(--glow-cyan);
  }
</style>
