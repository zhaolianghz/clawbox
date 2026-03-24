<script lang="ts">
  import { _ } from 'svelte-i18n';
  import { get_gateway_status, start_gateway, stop_gateway, restart_gateway, type GatewayStatus } from '$lib/api/gateway';
  
  let status = $state<GatewayStatus>({ running: false, version: 'unknown' });
  let loading = $state(false);
  
  async function refresh() {
    status = await get_gateway_status();
  }
  
  async function handleStart() {
    loading = true;
    try {
      await start_gateway();
      await refresh();
    } finally {
      loading = false;
    }
  }
  
  async function handleStop() {
    loading = true;
    try {
      await stop_gateway();
      await refresh();
    } finally {
      loading = false;
    }
  }
  
  async function handleRestart() {
    loading = true;
    try {
      await restart_gateway();
      await refresh();
    } finally {
      loading = false;
    }
  }
  
  refresh();
</script>

<div class="gateway-card glass-card">
  <div class="header">
    <h3>{$_('gateway.title')}</h3>
    <div class="status-badge" class:running={status.running} class:stopped={!status.running}>
      <span class="neon-dot" class:active={status.running}></span>
      <span>{status.running ? $_('gateway.running') : $_('gateway.stopped')}</span>
    </div>
  </div>
  
  <div class="info">
    <div class="info-item">
      <span class="label">{$_('gateway.version')}</span>
      <span class="value">{status.version}</span>
    </div>
  </div>
  
  <div class="actions">
    {#if status.running}
      <button class="neon-button stop" onclick={handleStop} disabled={loading}>
        {$_('gateway.stop')}
      </button>
    {:else}
      <button class="neon-button start" onclick={handleStart} disabled={loading}>
        {$_('gateway.start')}
      </button>
    {/if}
    <button class="neon-button" onclick={handleRestart} disabled={loading || !status.running}>
      {$_('gateway.restart')}
    </button>
  </div>
</div>

<style>
  .gateway-card {
    padding: 1.5rem;
  }
  
  .header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 1rem;
  }
  
  .header h3 {
    margin: 0;
    font-size: 1.1rem;
    color: var(--text-primary);
  }
  
  .status-badge {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.35rem 0.75rem;
    border-radius: 1rem;
    font-size: 0.85rem;
    background: var(--bg-tertiary);
  }
  
  .status-badge.running {
    color: var(--neon-green);
  }
  
  .status-badge.stopped {
    color: var(--neon-pink);
  }
  
  .neon-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--neon-pink);
  }
  
  .neon-dot.active {
    background: var(--neon-green);
    box-shadow: 0 0 8px var(--neon-green), 0 0 16px var(--neon-green);
    animation: pulse 2s infinite;
  }
  
  @keyframes pulse {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.5; }
  }
  
  .info {
    margin-bottom: 1rem;
  }
  
  .info-item {
    display: flex;
    gap: 0.5rem;
    font-size: 0.9rem;
  }
  
  .info-item .label {
    color: var(--text-secondary);
  }
  
  .info-item .value {
    color: var(--text-primary);
  }
  
  .actions {
    display: flex;
    gap: 0.5rem;
  }
  
  .neon-button {
    min-width: 80px;
  }
  
  .neon-button.start {
    border-color: var(--neon-green);
    color: var(--neon-green);
  }
  
  .neon-button.start:hover {
    box-shadow: var(--glow-green);
    background: rgba(0, 255, 136, 0.1);
  }
  
  .neon-button.stop {
    border-color: var(--neon-pink);
    color: var(--neon-pink);
  }
  
  .neon-button.stop:hover {
    box-shadow: 0 0 20px rgba(255, 0, 110, 0.5);
    background: rgba(255, 0, 110, 0.1);
  }
  
  .neon-button:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
</style>
