<script lang="ts">
  import { _ } from 'svelte-i18n';
  
  interface Props {
    icon: string;
    labelKey: string;
    value: string | number;
    trend?: string;
  }
  
  let { icon, labelKey, value, trend }: Props = $props();
</script>

<div class="stats-card glass-card">
  <div class="icon">{icon}</div>
  <div class="content">
    <span class="label">{$_(labelKey)}</span>
    <span class="value">{value}</span>
    {#if trend}
      <span class="trend">{trend}</span>
    {/if}
  </div>
</div>

<style>
  .stats-card {
    display: flex;
    align-items: center;
    gap: 1rem;
    padding: 1.25rem;
    min-width: 180px;
  }
  
  .icon {
    font-size: 2rem;
    width: 50px;
    height: 50px;
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--bg-tertiary);
    border-radius: 0.75rem;
  }
  
  .content {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
  }
  
  .label {
    font-size: 0.85rem;
    color: var(--text-secondary);
  }
  
  .value {
    font-size: 1.5rem;
    font-weight: 600;
    color: var(--text-primary);
  }
  
  .trend {
    font-size: 0.75rem;
    color: var(--neon-green);
  }
</style>
