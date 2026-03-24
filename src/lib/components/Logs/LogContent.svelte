<script lang="ts">
  import { _ } from 'svelte-i18n';
  import type { LogLine } from '$lib/api/logs';
  
  interface Props {
    lines: LogLine[];
    loading: boolean;
  }
  
  let { lines, loading }: Props = $props();
  
  function getLevelClass(level: string): string {
    switch (level) {
      case 'error': return 'level-error';
      case 'warn': return 'level-warn';
      case 'debug': return 'level-debug';
      default: return 'level-info';
    }
  }
  
  function getLevelBadge(level: string): string {
    return level.toUpperCase().padEnd(5, ' ');
  }
</script>

<div class="log-content">
  {#if loading}
    <div class="loading">
      <div class="spinner"></div>
      <span>{$_('logs.loading')}</span>
    </div>
  {:else if lines.length === 0}
    <div class="empty">
      <span class="empty-icon">📭</span>
      <span>{$_('logs.noContent')}</span>
    </div>
  {:else}
    <div class="log-lines">
      {#each lines as line}
        <div class="log-line {getLevelClass(line.level)}">
          <span class="timestamp">{line.timestamp}</span>
          <span class="level">{getLevelBadge(line.level)}</span>
          <span class="message">{line.message}</span>
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .log-content {
    flex: 1;
    background: var(--bg-primary);
    overflow: hidden;
    display: flex;
    flex-direction: column;
  }
  
  .loading, .empty {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 1rem;
    color: var(--text-muted);
  }
  
  .spinner {
    width: 32px;
    height: 32px;
    border: 3px solid var(--bg-tertiary);
    border-top-color: var(--neon-cyan);
    border-radius: 50%;
    animation: spin 1s linear infinite;
  }
  
  @keyframes spin {
    to { transform: rotate(360deg); }
  }
  
  .empty-icon {
    font-size: 3rem;
    opacity: 0.5;
  }
  
  .log-lines {
    flex: 1;
    overflow-y: auto;
    font-family: 'SF Mono', 'Fira Code', monospace;
    font-size: 0.8rem;
    line-height: 1.6;
    padding: 0.5rem 0;
  }
  
  .log-line {
    display: flex;
    gap: 1rem;
    padding: 0.25rem 1rem;
    border-left: 3px solid transparent;
    transition: background 0.1s ease;
  }
  
  .log-line:hover {
    background: var(--bg-secondary);
  }
  
  .log-line.level-error {
    border-left-color: var(--neon-pink);
    background: rgba(255, 0, 110, 0.05);
  }
  
  .log-line.level-warn {
    border-left-color: var(--neon-orange);
    background: rgba(255, 136, 0, 0.05);
  }
  
  .log-line.level-debug {
    opacity: 0.7;
  }
  
  .timestamp {
    color: var(--text-muted);
    white-space: nowrap;
    flex-shrink: 0;
  }
  
  .level {
    font-weight: 600;
    white-space: nowrap;
    flex-shrink: 0;
  }
  
  .level-error .level {
    color: var(--neon-pink);
  }
  
  .level-warn .level {
    color: var(--neon-orange);
  }
  
  .level-info .level {
    color: var(--neon-cyan);
  }
  
  .level-debug .level {
    color: var(--neon-purple);
  }
  
  .message {
    color: var(--text-secondary);
    word-break: break-word;
  }
  
  .level-error .message {
    color: var(--text-primary);
  }
</style>
