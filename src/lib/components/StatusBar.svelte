<script lang="ts">
  import { _ } from 'svelte-i18n';
  
  let gatewayRunning = $state(true);
  let version = $state('1.0.0');
  let tokenCount = $state(0);
</script>

<footer class="statusbar">
  <div class="status-item">
    <span class="status-dot" class:running={gatewayRunning}></span>
    <span>{gatewayRunning ? $_('gateway.running') : $_('gateway.stopped')}</span>
  </div>
  
  <div class="status-item">
    <span>v{version}</span>
  </div>
  
  <div class="status-item">
    <span>🔢 {tokenCount.toLocaleString()} tokens</span>
  </div>
</footer>

<style>
  .statusbar {
    height: 32px;
    background: var(--bg-secondary);
    border-top: 1px solid rgba(255, 255, 255, 0.1);
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 1rem;
    font-size: 0.8rem;
    color: var(--text-secondary);
  }
  
  .status-item {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }
  
  .status-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--neon-pink);
    box-shadow: 0 0 8px var(--neon-pink);
    transition: all 0.3s ease;
  }
  
  .status-dot.running {
    background: var(--neon-green);
    box-shadow: var(--glow-green);
  }
</style>
