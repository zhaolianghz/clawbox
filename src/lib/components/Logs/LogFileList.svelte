<script lang="ts">
  import { _ } from 'svelte-i18n';
  import type { LogFile } from '$lib/api/logs';
  
  interface Props {
    files: LogFile[];
    selectedFile: string | null;
    onSelect: (path: string) => void;
  }
  
  let { files, selectedFile, onSelect }: Props = $props();
  
  function formatSize(bytes: number): string {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  }
  
  function getFileIcon(name: string): string {
    if (name.includes('error')) return '🔴';
    if (name.includes('agent')) return '🤖';
    if (name.includes('gateway')) return '🌐';
    return '📄';
  }
</script>

<div class="log-file-list">
  <div class="list-header">
    <span class="title">{$_('logs.files')}</span>
    <span class="count">{files.length}</span>
  </div>
  
  <div class="file-list">
    {#each files as file}
      <button
        class="file-item"
        class:selected={selectedFile === file.path}
        onclick={() => onSelect(file.path)}
      >
        <span class="file-icon">{getFileIcon(file.name)}</span>
        <div class="file-info">
          <span class="file-name">{file.name}</span>
          <span class="file-meta">
            {formatSize(file.size)} · {file.modified.split(' ')[0]}
          </span>
        </div>
      </button>
    {:else}
      <div class="empty">{$_('logs.noFiles')}</div>
    {/each}
  </div>
</div>

<style>
  .log-file-list {
    width: 280px;
    background: var(--bg-secondary);
    border-right: 1px solid rgba(255, 255, 255, 0.1);
    display: flex;
    flex-direction: column;
    height: 100%;
  }
  
  .list-header {
    padding: 1rem;
    border-bottom: 1px solid rgba(255, 255, 255, 0.1);
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  
  .title {
    font-weight: 600;
    color: var(--text-primary);
  }
  
  .count {
    background: var(--bg-tertiary);
    padding: 0.25rem 0.5rem;
    border-radius: 0.25rem;
    font-size: 0.75rem;
    color: var(--text-muted);
  }
  
  .file-list {
    flex: 1;
    overflow-y: auto;
    padding: 0.5rem;
  }
  
  .file-item {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    padding: 0.75rem;
    background: transparent;
    border: none;
    border-radius: 0.5rem;
    cursor: pointer;
    transition: all 0.2s ease;
    width: 100%;
    text-align: left;
    margin-bottom: 0.25rem;
  }
  
  .file-item:hover {
    background: var(--bg-tertiary);
  }
  
  .file-item.selected {
    background: rgba(0, 245, 255, 0.1);
    border-left: 3px solid var(--neon-cyan);
  }
  
  .file-icon {
    font-size: 1.25rem;
  }
  
  .file-info {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    flex: 1;
    min-width: 0;
  }
  
  .file-name {
    font-size: 0.875rem;
    color: var(--text-primary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  
  .file-meta {
    font-size: 0.75rem;
    color: var(--text-muted);
  }
  
  .empty {
    padding: 2rem;
    text-align: center;
    color: var(--text-muted);
  }
</style>
