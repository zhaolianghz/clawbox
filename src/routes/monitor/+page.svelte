<script lang="ts">
  import { _ } from 'svelte-i18n';
  import { onMount } from 'svelte';
  import { get_stats, extractMetrics } from '$lib/api/stats';
  import { list_gateway_statuses, start_gateway, stop_gateway } from '$lib/api/gateway';
  import { list_backends, type BackendInfo } from '$lib/api/backends';

  interface Metric {
    label: string;
    value: string | number;
    color: string;
  }

  let metrics = $state<Metric[]>([]);
  let gatewayRunning = $state(false);
  let rawHealth = $state<unknown>(null);
  let isLoading = $state(true);
  let backends = $state<BackendInfo[]>([]);
  let statuses = $state<Record<string, { status: 'running' | 'stopped'; version: string; pid?: number }>>({});

  async function loadData() {
    isLoading = true;
    const [stats, bl, gs] = await Promise.all([
      get_stats(30),
      list_backends(),
      list_gateway_statuses(),
    ]);
    gatewayRunning = stats.gateway_running;
    rawHealth = stats.health;
    backends = bl;
    const map: typeof statuses = {};
    for (const s of gs.statuses) map[s.backend] = s.status;
    statuses = map;

    if (gatewayRunning) {
      const m = extractMetrics(stats);
      metrics = [
        { label: 'Total Tokens (30d)', value: m.totalTokens?.toLocaleString() ?? '—', color: 'var(--neon-cyan)' },
        { label: 'API Calls', value: m.apiCalls?.toLocaleString() ?? '—', color: 'var(--neon-green)' },
        { label: 'Cost (USD)', value: m.totalCost != null ? '$' + m.totalCost.toFixed(2) : '—', color: 'var(--neon-orange)' },
      ];
    } else {
      metrics = [];
    }
    isLoading = false;
  }

  async function toggleBackend(id: BackendInfo['id']) {
    const s = statuses[id];
    if (s?.status === 'running') await stop_gateway(id);
    else await start_gateway(id);
    await loadData();
  }

  onMount(loadData);
</script>

<svelte:head>
  <title>{$_('monitor.title')} - ClawBox</title>
</svelte:head>

<div class="monitor-page">
  <div class="page-header">
    <div>
      <h1>{$_('monitor.title')}</h1>
      <p class="subtitle">{$_('monitor.subtitle')}</p>
    </div>
    <div class="header-actions">
      <button class="neon-button" onclick={loadData}>🔄 {$_('monitor.refresh')}</button>
    </div>
  </div>

  {#if isLoading}
    <div class="loading"><div class="spinner"></div></div>
  {:else}
    <div class="gateway-grid">
      {#each backends.filter((b) => b.installed) as b (b.id)}
        <div class="gateway-card glass-card">
          <header>
            <span class="backend-chip" data-backend={b.id}>{b.displayName}</span>
            <span class="version">v{b.version}</span>
          </header>
          <div class="status" class:running={statuses[b.id]?.status === 'running'}>
            {statuses[b.id]?.status ?? 'unknown'}
            {#if statuses[b.id]?.pid}<span class="pid">PID {statuses[b.id]?.pid}</span>{/if}
          </div>
          <button class="neon-button" onclick={() => toggleBackend(b.id)}>
            {statuses[b.id]?.status === 'running' ? 'Stop' : 'Start'}
          </button>
        </div>
      {/each}
    </div>

    {#if !gatewayRunning}
      <div class="empty-state glass-card">
        <span class="empty-icon">🔌</span>
        <p>Gateway is not running.</p>
        <p class="empty-hint">Start the gateway to view live metrics and health.</p>
      </div>
    {:else}
    <div class="metrics-row">
      {#each metrics as metric}
        <div class="metric-card glass-card">
          <div class="metric-label">{metric.label}</div>
          <div class="metric-value" style="color: {metric.color}">{metric.value}</div>
        </div>
      {/each}
    </div>

    <div class="monitor-container">
      <div class="trace-panel glass-card">
        <div class="panel-header">
          <h2>{$_('monitor.resourceUsage')}</h2>
        </div>
        <div class="health-content">
          {#if rawHealth}
            <pre class="health-json">{JSON.stringify(rawHealth, null, 2)}</pre>
          {:else}
            <div class="empty-state">
              <span class="empty-icon">📈</span>
              <p>No health data available.</p>
            </div>
          {/if}
        </div>
      </div>
    </div>
    {/if}
  {/if}
</div>

<style>
  .monitor-page {
    height: calc(100vh - 60px - 32px - 3rem);
    display: flex;
    flex-direction: column;
    margin: -1.5rem;
    background: var(--bg-primary);
  }
  
  .page-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 1rem 1.5rem;
    background: var(--bg-secondary);
    border-bottom: 1px solid rgba(255, 255, 255, 0.1);
  }
  
  .page-header h1 {
    margin: 0;
    font-size: 1.25rem;
  }
  
  .subtitle {
    margin: 0.25rem 0 0;
    color: var(--text-muted);
    font-size: 0.875rem;
  }
  
  .header-actions {
    display: flex;
    gap: 0.5rem;
  }
  
  .metrics-row {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 1rem;
    padding: 1rem;
  }

  .empty-state {
    margin: 1rem;
    padding: 3rem 2rem;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    text-align: center;
    color: var(--text-muted);
  }

  .empty-icon {
    font-size: 2.5rem;
    margin-bottom: 0.75rem;
    opacity: 0.6;
  }

  .empty-state p {
    margin: 0.25rem 0;
  }

  .empty-hint {
    font-size: 0.75rem;
    opacity: 0.7;
  }

  .health-content {
    flex: 1;
    overflow: auto;
  }

  .health-json {
    margin: 0;
    font-family: monospace;
    font-size: 0.75rem;
    color: var(--text-secondary);
    white-space: pre-wrap;
    word-break: break-word;
  }
  
  .gateway-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
    gap: 1rem;
    margin-bottom: 1rem;
  }
  .gateway-card {
    padding: 1rem;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }
  .gateway-card header {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }
  .backend-chip {
    padding: 0.125rem 0.5rem;
    border-radius: 999px;
    font-weight: 600;
    font-size: 0.7rem;
  }
  .backend-chip[data-backend="openclaw"] { background: rgba(0,245,255,0.15); color: var(--neon-cyan); }
  .backend-chip[data-backend="hermes"]   { background: rgba(255,0,200,0.15); color: #ff6ad5; }
  .version { color: var(--text-muted); font-size: 0.75rem; }
  .gateway-card .status { font-size: 0.875rem; }
  .gateway-card .status.running { color: var(--neon-green); }
  .gateway-card .pid { color: var(--text-muted); margin-left: 0.5rem; font-size: 0.75rem; }

  .metric-card {
    padding: 1rem;
    text-align: center;
  }
  
  .metric-label {
    font-size: 0.75rem;
    color: var(--text-muted);
    margin-bottom: 0.5rem;
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }
  
  .metric-value {
    font-size: 1.75rem;
    font-weight: 700;
  }
  
  .metric-trend {
    font-size: 0.75rem;
    margin-top: 0.5rem;
    color: var(--neon-pink);
  }
  
  .metric-trend.positive {
    color: var(--neon-green);
  }
  
  .monitor-container {
    flex: 1;
    display: flex;
    gap: 1rem;
    padding: 0 1rem 1rem;
    overflow: hidden;
  }
  
  .trace-panel, .charts-panel {
    display: flex;
    flex-direction: column;
    padding: 1rem;
  }
  
  .trace-panel {
    flex: 1;
  }
  
  .charts-panel {
    flex: 1;
  }
  
  .panel-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 1rem;
  }
  
  .panel-header h2 {
    margin: 0;
    font-size: 1rem;
  }
  
  .trace-count {
    font-size: 0.75rem;
    color: var(--text-muted);
  }
  
  .trace-list {
    flex: 1;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }
  
  .trace-item {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    padding: 0.75rem;
    background: var(--bg-primary);
    border-radius: 0.5rem;
    border-left: 3px solid var(--neon-cyan);
  }
  
  .trace-icon {
    font-size: 1.25rem;
  }
  
  .trace-main {
    flex: 1;
  }
  
  .trace-header-row {
    display: flex;
    justify-content: space-between;
    margin-bottom: 0.25rem;
  }
  
  .trace-agent {
    font-weight: 600;
    color: var(--text-primary);
  }
  
  .trace-type {
    font-size: 0.75rem;
    text-transform: uppercase;
  }
  
  .trace-details {
    display: flex;
    gap: 1rem;
    font-size: 0.75rem;
    color: var(--text-muted);
  }
  
  .chart-placeholder {
    flex: 1;
    display: flex;
    flex-direction: column;
  }
  
  .placeholder-content {
    text-align: center;
    color: var(--text-muted);
    padding: 1rem 0;
  }
  
  .placeholder-icon {
    font-size: 2rem;
    display: block;
    margin-bottom: 0.5rem;
    opacity: 0.5;
  }
  
  .mock-chart {
    flex: 1;
    display: flex;
    align-items: flex-end;
    justify-content: center;
    gap: 1rem;
    padding: 1rem;
    min-height: 150px;
  }
  
  .chart-bar {
    width: 40px;
    background: linear-gradient(to top, var(--neon-cyan), var(--neon-purple));
    border-radius: 0.25rem 0.25rem 0 0;
    opacity: 0.6;
  }
  
  .usage-stats {
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
    padding-top: 1rem;
    border-top: 1px solid rgba(255, 255, 255, 0.1);
  }
  
  .usage-item {
    display: flex;
    align-items: center;
    gap: 0.75rem;
  }
  
  .usage-label {
    width: 60px;
    font-size: 0.75rem;
    color: var(--text-muted);
  }
  
  .usage-bar {
    flex: 1;
    height: 8px;
    background: var(--bg-tertiary);
    border-radius: 4px;
    overflow: hidden;
  }
  
  .usage-fill {
    height: 100%;
    border-radius: 4px;
    transition: width 0.3s ease;
  }
  
  .usage-value {
    width: 40px;
    font-size: 0.75rem;
    text-align: right;
    color: var(--text-secondary);
  }
  
  .loading {
    display: flex;
    justify-content: center;
    padding: 2rem;
  }
  
  .spinner {
    width: 24px;
    height: 24px;
    border: 2px solid var(--bg-tertiary);
    border-top-color: var(--neon-cyan);
    border-radius: 50%;
    animation: spin 1s linear infinite;
  }
  
  @keyframes spin {
    to { transform: rotate(360deg); }
  }
</style>
