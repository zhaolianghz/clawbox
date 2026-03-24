<script lang="ts">
  import { onMount } from 'svelte';
  import { _ } from 'svelte-i18n';
  import { get_log_files, get_log_content, type LogFile, type LogLine } from '$lib/api/logs';
  import LogFileList from '$lib/components/Logs/LogFileList.svelte';
  import LogContent from '$lib/components/Logs/LogContent.svelte';
  
  let files = $state<LogFile[]>([]);
  let selectedFile = $state<string | null>(null);
  let lines = $state<LogLine[]>([]);
  let loading = $state(true);
  let filterText = $state('');
  let searchTimeout: ReturnType<typeof setTimeout>;
  
  async function loadFiles() {
    files = await get_log_files();
    if (files.length > 0 && !selectedFile) {
      selectedFile = files[0].path;
      await loadContent();
    }
  }
  
  async function loadContent() {
    if (!selectedFile) return;
    loading = true;
    try {
      lines = await get_log_content(selectedFile, filterText || undefined);
    } finally {
      loading = false;
    }
  }
  
  function handleSelect(path: string) {
    selectedFile = path;
    loadContent();
  }
  
  function handleFilterInput() {
    clearTimeout(searchTimeout);
    searchTimeout = setTimeout(loadContent, 300);
  }
  
  function handleRefresh() {
    loadFiles();
    loadContent();
  }
  
  onMount(loadFiles);
</script>

<svelte:head>
  <title>{$_('logs.title')} - ClawBox</title>
</svelte:head>

<div class="logs-page">
  <div class="page-header">
    <h1>{$_('logs.title')}</h1>
    <div class="actions">
      <input
        type="text"
        class="search-input"
        placeholder={$_('logs.search')}
        bind:value={filterText}
        oninput={handleFilterInput}
      />
      <button class="neon-button" onclick={handleRefresh}>
        🔄 {$_('logs.refresh')}
      </button>
    </div>
  </div>
  
  <div class="logs-container">
    <LogFileList {files} {selectedFile} onSelect={handleSelect} />
    <LogContent {lines} {loading} />
  </div>
</div>

<style>
  .logs-page {
    display: flex;
    flex-direction: column;
    height: calc(100vh - 60px - 32px - 3rem);
    margin: -1.5rem;
    background: var(--bg-primary);
  }
  
  .page-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 1rem 1.5rem;
    background: var(--bg-secondary);
    border-bottom: 1px solid rgba(255, 255, 255, 0.1);
  }
  
  .page-header h1 {
    font-size: 1.25rem;
    font-weight: 600;
    margin: 0;
  }
  
  .actions {
    display: flex;
    gap: 0.75rem;
    align-items: center;
  }
  
  .search-input {
    background: var(--bg-tertiary);
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 0.5rem;
    padding: 0.5rem 1rem;
    color: var(--text-primary);
    font-size: 0.875rem;
    width: 200px;
    transition: all 0.2s ease;
  }
  
  .search-input:focus {
    outline: none;
    border-color: var(--neon-cyan);
    box-shadow: var(--glow-cyan);
  }
  
  .search-input::placeholder {
    color: var(--text-muted);
  }
  
  .logs-container {
    flex: 1;
    display: flex;
    overflow: hidden;
  }
</style>
