<script lang="ts">
  import { _ } from 'svelte-i18n';
  import { onMount } from 'svelte';
  
  interface Metric {
    label: string;
    value: string | number;
    trend?: number;
    color: string;
  }
  
  interface TraceEvent {
    id: string;
    time: string;
    type: 'request' | 'response' | 'error';
    agent: string;
    duration: number;
    tokens: number;
  }
  
  let metrics = $state<Metric[]>([]);
  let traces = $state<TraceEvent[]>([]);
  let isLoading = $state(true);
  
  async function loadData() {
    await new Promise(resolve => setTimeout(resolve, 500));
    metrics = [
      { label: 'Total Tokens', value: '1.2M', trend: 15, color: 'var(--neon-cyan)' },
      { label: 'API Calls', value: '8,432', trend: 8, color: 'var(--neon-green)' },
      { label: 'Avg Latency', value: '1.2s', trend: -5, color: 'var(--neon-orange)' },
      { label: 'Error Rate', value: '0.3%', trend: -2, color: 'var(--neon-pink)' },
    ];
    traces = [
      { id: '1', time: '15:30:01', type: 'request', agent: 'claude-3', duration: 0, tokens: 0 },
      { id: '2', time: '15:30:02', type: 'response', agent: 'claude-3', duration: 1200, tokens: 245 },
      { id: '3', time: '15:30:05', type: 'request', agent: 'gpt-4', duration: 0, tokens: 0 },
      { id: '4', time: '15:30:08', type: 'error', agent: 'gpt-4', duration: 3000, tokens: 0 },
      { id: '5', time: '15:30:10', type: 'request', agent: 'claude-3', duration: 0, tokens: 0 },
      { id: '6', time: '15:30:11', type: 'response', agent: 'claude-3', duration: 980, tokens: 180 },
    ];
    isLoading = false;
  }
  
  function getTypeIcon(type: string): string {
    switch (type) {
      case 'request': return '📤';
      case 'response': return '📥';
      case 'error': return '❌';
      default: return '📄';
    }
  }
  
  function getTypeColor(type: string): string {
    switch (type) {
      case 'request': return 'var(--neon-cyan)';
      case 'response': return 'var(--neon-green)';
      case 'error': return 'var(--neon-pink)';
      default: return 'var(--text-muted)';
    }
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
      <button class="neon-button">🔄 {$_('monitor.refresh')}</button>
      <button class="neon-button">📊 {$_('monitor.export')}</button>
    </div>
  </div>
  
  <div class="metrics-row">
    {#each metrics as metric}
      <div class="metric-card glass-card">
        <div class="metric-label">{metric.label}</div>
        <div class="metric-value" style="color: {metric.color}">{metric.value}</div>
        {#if metric.trend !== undefined}
          <div class="metric-trend" class:positive={metric.trend > 0}>
            {metric.trend > 0 ? '↑' : '↓'} {Math.abs(metric.trend)}%
          </div>
        {/if}
      </div>
    {/each}
  </div>
  
  <div class="monitor-container">
    <div class="trace-panel glass-card">
      <div class="panel-header">
        <h2>{$_('monitor.linkTracing')}</h2>
        <span class="trace-count">{traces.length} {$_('monitor.events')}</span>
      </div>
      
      {#if isLoading}
        <div class="loading">
          <div class="spinner"></div>
        </div>
      {:else}
        <div class="trace-list">
          {#each traces as trace}
            <div class="trace-item">
              <span class="trace-icon">{getTypeIcon(trace.type)}</span>
              <div class="trace-main">
                <div class="trace-header-row">
                  <span class="trace-agent">{trace.agent}</span>
                  <span class="trace-type" style="color: {getTypeColor(trace.type)}">
                    {trace.type}
                  </span>
                </div>
                <div class="trace-details">
                  <span class="trace-time">{trace.time}</span>
                  {#if trace.duration > 0}
                    <span class="trace-duration">{trace.duration}ms</span>
                  {/if}
                  {#if trace.tokens > 0}
                    <span class="trace-tokens">{trace.tokens} tokens</span>
                  {/if}
                </div>
              </div>
            </div>
          {/each}
        </div>
      {/if}
    </div>
    
    <div class="charts-panel glass-card">
      <div class="panel-header">
        <h2>{$_('monitor.resourceUsage')}</h2>
      </div>
      
      <div class="chart-placeholder">
        <div class="placeholder-content">
          <span class="placeholder-icon">📈</span>
          <p>{$_('monitor.chartPlaceholder')}</p>
        </div>
        
        <div class="mock-chart">
          <div class="chart-bar" style="height: 60%"></div>
          <div class="chart-bar" style="height: 80%"></div>
          <div class="chart-bar" style="height: 45%"></div>
          <div class="chart-bar" style="height: 90%"></div>
          <div class="chart-bar" style="height: 70%"></div>
          <div class="chart-bar" style="height: 55%"></div>
          <div class="chart-bar" style="height: 85%"></div>
          <div class="chart-bar" style="height: 40%"></div>
        </div>
        
        <div class="usage-stats">
          <div class="usage-item">
            <span class="usage-label">CPU</span>
            <div class="usage-bar">
              <div class="usage-fill" style="width: 35%; background: var(--neon-cyan)"></div>
            </div>
            <span class="usage-value">35%</span>
          </div>
          <div class="usage-item">
            <span class="usage-label">Memory</span>
            <div class="usage-bar">
              <div class="usage-fill" style="width: 62%; background: var(--neon-purple)"></div>
            </div>
            <span class="usage-value">62%</span>
          </div>
          <div class="usage-item">
            <span class="usage-label">Network</span>
            <div class="usage-bar">
              <div class="usage-fill" style="width: 28%; background: var(--neon-green)"></div>
            </div>
            <span class="usage-value">28%</span>
          </div>
        </div>
      </div>
    </div>
  </div>
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
    grid-template-columns: repeat(4, 1fr);
    gap: 1rem;
    padding: 1rem;
  }
  
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
