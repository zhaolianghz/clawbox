<script lang="ts">
  import { _ } from 'svelte-i18n';
  import { onMount } from 'svelte';
  
  interface AgentFlow {
    id: string;
    name: string;
    status: 'idle' | 'running' | 'completed' | 'error';
    nodes: number;
    createdAt: string;
  }
  
  let flows = $state<AgentFlow[]>([]);
  let selectedFlow = $state<AgentFlow | null>(null);
  let isLoading = $state(true);
  
  async function loadFlows() {
    await new Promise(resolve => setTimeout(resolve, 500));
    flows = [
      { id: '1', name: 'Code Review Pipeline', status: 'running', nodes: 4, createdAt: '2024-03-24 14:30' },
      { id: '2', name: 'Documentation Generator', status: 'completed', nodes: 3, createdAt: '2024-03-24 12:15' },
      { id: '3', name: 'Test & Deploy', status: 'idle', nodes: 5, createdAt: '2024-03-23 18:00' },
      { id: '4', name: 'Data Analysis Flow', status: 'error', nodes: 2, createdAt: '2024-03-23 10:30' },
    ];
    isLoading = false;
  }
  
  function getStatusColor(status: string): string {
    switch (status) {
      case 'running': return 'var(--neon-green)';
      case 'completed': return 'var(--neon-cyan)';
      case 'error': return 'var(--neon-pink)';
      default: return 'var(--text-muted)';
    }
  }
  
  function getStatusIcon(status: string): string {
    switch (status) {
      case 'running': return '▶️';
      case 'completed': return '✅';
      case 'error': return '❌';
      default: return '⏸️';
    }
  }
  
  onMount(loadFlows);
</script>

<svelte:head>
  <title>{$_('agents.title')} - ClawBox</title>
</svelte:head>

<div class="agents-page">
  <div class="page-header">
    <div>
      <h1>{$_('agents.title')}</h1>
      <p class="subtitle">{$_('agents.subtitle')}</p>
    </div>
    <button class="neon-button">➕ {$_('agents.newFlow')}</button>
  </div>
  
  <div class="agents-container">
    <div class="flows-sidebar glass-card">
      <div class="sidebar-header">
        <span>{$_('agents.flows')}</span>
        <span class="count">{flows.length}</span>
      </div>
      
      {#if isLoading}
        <div class="loading">
          <div class="spinner"></div>
        </div>
      {:else}
        <div class="flow-list">
          {#each flows as flow}
            <button
              class="flow-item"
              class:selected={selectedFlow?.id === flow.id}
              onclick={() => selectedFlow = flow}
            >
              <span class="flow-icon">{getStatusIcon(flow.status)}</span>
              <div class="flow-info">
                <span class="flow-name">{flow.name}</span>
                <span class="flow-meta">{flow.nodes} {$_('agents.nodes')} · {flow.createdAt}</span>
              </div>
              <span class="flow-status" style="color: {getStatusColor(flow.status)}">
                {flow.status}
              </span>
            </button>
          {/each}
        </div>
      {/if}
    </div>
    
    <div class="flow-editor glass-card">
      {#if selectedFlow}
        <div class="editor-header">
          <h2>{selectedFlow.name}</h2>
          <div class="editor-actions">
            <button class="neon-button">▶️ {$_('agents.run')}</button>
            <button class="neon-button">✏️ {$_('agents.edit')}</button>
          </div>
        </div>
        
        <div class="flow-canvas">
          <div class="placeholder-canvas">
            <div class="placeholder-content">
              <span class="placeholder-icon">🔀</span>
              <p>{$_('agents.flowEditorPlaceholder')}</p>
              <p class="placeholder-hint">{$_('agents.flowEditorHint')}</p>
            </div>
            
            <div class="mock-nodes">
              <div class="mock-node node-start">
                <span>🚀</span>
                <span>Start</span>
              </div>
              <div class="node-connector"></div>
              <div class="mock-node node-agent">
                <span>🤖</span>
                <span>Agent 1</span>
              </div>
              <div class="node-connector"></div>
              <div class="mock-node node-agent">
                <span>🤖</span>
                <span>Agent 2</span>
              </div>
              <div class="node-connector"></div>
              <div class="mock-node node-end">
                <span>✅</span>
                <span>Output</span>
              </div>
            </div>
          </div>
        </div>
        
        <div class="message-trace">
          <div class="trace-header">
            <span>{$_('agents.messageTrace')}</span>
          </div>
          <div class="trace-content">
            <div class="trace-item">
              <span class="trace-time">14:30:01</span>
              <span class="trace-node">Agent 1</span>
              <span class="trace-msg">Processing input data...</span>
            </div>
            <div class="trace-item">
              <span class="trace-time">14:30:03</span>
              <span class="trace-node">Agent 1 → 2</span>
              <span class="trace-msg">Transferring context (245 tokens)</span>
            </div>
            <div class="trace-item">
              <span class="trace-time">14:30:05</span>
              <span class="trace-node">Agent 2</span>
              <span class="trace-msg">Generating response...</span>
            </div>
          </div>
        </div>
      {:else}
        <div class="no-selection">
          <span class="no-selection-icon">👈</span>
          <p>{$_('agents.selectFlow')}</p>
        </div>
      {/if}
    </div>
  </div>
</div>

<style>
  .agents-page {
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
  
  .agents-container {
    flex: 1;
    display: flex;
    gap: 1rem;
    padding: 1rem;
    overflow: hidden;
  }
  
  .flows-sidebar {
    width: 280px;
    display: flex;
    flex-direction: column;
    padding: 1rem;
  }
  
  .sidebar-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 1rem;
    font-weight: 600;
  }
  
  .count {
    background: var(--bg-tertiary);
    padding: 0.125rem 0.5rem;
    border-radius: 0.25rem;
    font-size: 0.75rem;
    color: var(--text-muted);
  }
  
  .flow-list {
    flex: 1;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }
  
  .flow-item {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    padding: 0.75rem;
    background: var(--bg-tertiary);
    border: none;
    border-radius: 0.5rem;
    cursor: pointer;
    transition: all 0.2s ease;
    text-align: left;
    width: 100%;
  }
  
  .flow-item:hover, .flow-item.selected {
    background: rgba(0, 245, 255, 0.1);
  }
  
  .flow-icon {
    font-size: 1.25rem;
  }
  
  .flow-info {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
  }
  
  .flow-name {
    font-size: 0.875rem;
    color: var(--text-primary);
  }
  
  .flow-meta {
    font-size: 0.75rem;
    color: var(--text-muted);
  }
  
  .flow-status {
    font-size: 0.75rem;
    text-transform: uppercase;
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
  
  .flow-editor {
    flex: 1;
    display: flex;
    flex-direction: column;
    padding: 1rem;
  }
  
  .editor-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 1rem;
  }
  
  .editor-header h2 {
    margin: 0;
    font-size: 1rem;
  }
  
  .editor-actions {
    display: flex;
    gap: 0.5rem;
  }
  
  .flow-canvas {
    flex: 1;
    background: var(--bg-primary);
    border-radius: 0.5rem;
    margin-bottom: 1rem;
    overflow: hidden;
  }
  
  .placeholder-canvas {
    height: 100%;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    position: relative;
  }
  
  .placeholder-content {
    text-align: center;
    color: var(--text-muted);
    margin-bottom: 2rem;
  }
  
  .placeholder-icon {
    font-size: 3rem;
    display: block;
    margin-bottom: 1rem;
    opacity: 0.5;
  }
  
  .placeholder-hint {
    font-size: 0.75rem;
    opacity: 0.7;
  }
  
  .mock-nodes {
    display: flex;
    align-items: center;
    gap: 0;
    opacity: 0.6;
  }
  
  .mock-node {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 0.25rem;
    padding: 1rem;
    background: var(--bg-secondary);
    border-radius: 0.5rem;
    border: 1px solid rgba(255, 255, 255, 0.1);
    min-width: 80px;
    font-size: 0.75rem;
  }
  
  .node-connector {
    width: 40px;
    height: 2px;
    background: var(--neon-cyan);
    opacity: 0.5;
  }
  
  .node-start {
    border-color: var(--neon-green);
  }
  
  .node-end {
    border-color: var(--neon-cyan);
  }
  
  .message-trace {
    background: var(--bg-primary);
    border-radius: 0.5rem;
    max-height: 150px;
    overflow: hidden;
    display: flex;
    flex-direction: column;
  }
  
  .trace-header {
    padding: 0.5rem 1rem;
    background: var(--bg-tertiary);
    font-size: 0.75rem;
    font-weight: 600;
    color: var(--text-secondary);
  }
  
  .trace-content {
    flex: 1;
    overflow-y: auto;
    padding: 0.5rem 0;
    font-family: monospace;
    font-size: 0.75rem;
  }
  
  .trace-item {
    display: flex;
    gap: 1rem;
    padding: 0.25rem 1rem;
  }
  
  .trace-time {
    color: var(--text-muted);
  }
  
  .trace-node {
    color: var(--neon-cyan);
    min-width: 100px;
  }
  
  .trace-msg {
    color: var(--text-secondary);
  }
  
  .no-selection {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    color: var(--text-muted);
  }
  
  .no-selection-icon {
    font-size: 3rem;
    margin-bottom: 1rem;
    opacity: 0.5;
  }
</style>
