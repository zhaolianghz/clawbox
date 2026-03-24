<script lang="ts">
  import { _ } from 'svelte-i18n';
  import { onMount } from 'svelte';
  
  interface Task {
    id: string;
    name: string;
    schedule: string;
    status: 'enabled' | 'disabled' | 'running';
    lastRun: string;
    nextRun: string;
    type: 'scheduled' | 'triggered';
  }
  
  let tasks = $state<Task[]>([]);
  let isLoading = $state(true);
  
  async function loadTasks() {
    await new Promise(resolve => setTimeout(resolve, 500));
    tasks = [
      { id: '1', name: 'Daily Backup', schedule: '0 2 * * *', status: 'enabled', lastRun: '2024-03-24 02:00', nextRun: '2024-03-25 02:00', type: 'scheduled' },
      { id: '2', name: 'Log Cleanup', schedule: '0 0 * * 0', status: 'enabled', lastRun: '2024-03-17 00:00', nextRun: '2024-03-24 00:00', type: 'scheduled' },
      { id: '3', name: 'Code Review Check', schedule: '*/30 * * * *', status: 'running', lastRun: '2024-03-24 15:00', nextRun: '2024-03-24 15:30', type: 'triggered' },
      { id: '4', name: 'Weekly Report', schedule: '0 9 * * 1', status: 'disabled', lastRun: '2024-03-18 09:00', nextRun: '-', type: 'scheduled' },
      { id: '5', name: 'Health Check', schedule: '*/5 * * * *', status: 'enabled', lastRun: '2024-03-24 15:25', nextRun: '2024-03-24 15:30', type: 'triggered' },
    ];
    isLoading = false;
  }
  
  function getStatusColor(status: string): string {
    switch (status) {
      case 'running': return 'var(--neon-green)';
      case 'enabled': return 'var(--neon-cyan)';
      default: return 'var(--text-muted)';
    }
  }
  
  function getStatusIcon(status: string): string {
    switch (status) {
      case 'running': return '▶️';
      case 'enabled': return '✅';
      default: return '⏸️';
    }
  }
  
  function toggleTask(task: Task) {
    if (task.status === 'running') return;
    task.status = task.status === 'enabled' ? 'disabled' : 'enabled';
  }
  
  onMount(loadTasks);
</script>

<svelte:head>
  <title>{$_('tasks.title')} - ClawBox</title>
</svelte:head>

<div class="tasks-page">
  <div class="page-header">
    <div>
      <h1>{$_('tasks.title')}</h1>
      <p class="subtitle">{$_('tasks.subtitle')}</p>
    </div>
    <button class="neon-button">➕ {$_('tasks.newTask')}</button>
  </div>
  
  <div class="tasks-container">
    <div class="tasks-list glass-card">
      <div class="list-header">
        <span>{$_('tasks.scheduledTasks')}</span>
        <span class="count">{tasks.length}</span>
      </div>
      
      {#if isLoading}
        <div class="loading">
          <div class="spinner"></div>
        </div>
      {:else}
        <div class="task-items">
          {#each tasks as task}
            <div class="task-item" class:disabled={task.status === 'disabled'}>
              <div class="task-status">
                <span class="status-icon">{getStatusIcon(task.status)}</span>
                <span class="status-dot" style="background: {getStatusColor(task.status)}"></span>
              </div>
              
              <div class="task-info">
                <div class="task-header">
                  <span class="task-name">{task.name}</span>
                  <span class="task-type">{task.type}</span>
                </div>
                <div class="task-schedule">
                  <code>{task.schedule}</code>
                </div>
                <div class="task-meta">
                  <span>{$_('tasks.lastRun')}: {task.lastRun}</span>
                  <span>{$_('tasks.nextRun')}: {task.nextRun}</span>
                </div>
              </div>
              
              <div class="task-actions">
                <button
                  class="action-btn"
                  onclick={() => toggleTask(task)}
                  disabled={task.status === 'running'}
                  title={task.status === 'enabled' ? $_('tasks.disable') : $_('tasks.enable')}
                >
                  {task.status === 'enabled' || task.status === 'running' ? '⏸️' : '▶️'}
                </button>
                <button class="action-btn" title={$_('tasks.edit')}>
                  ✏️
                </button>
                <button class="action-btn delete" title={$_('tasks.delete')}>
                  🗑️
                </button>
              </div>
            </div>
          {/each}
        </div>
      {/if}
    </div>
    
    <div class="flow-editor-panel glass-card">
      <div class="panel-header">
        <h2>{$_('tasks.flowEditor')}</h2>
      </div>
      
      <div class="editor-content">
        <div class="placeholder-content">
          <span class="placeholder-icon">📋</span>
          <p>{$_('tasks.flowEditorPlaceholder')}</p>
        </div>
        
        <div class="schedule-builder">
          <h3>{$_('tasks.scheduleBuilder')}</h3>
          <div class="builder-form">
            <div class="form-group">
              <label>{$_('tasks.frequency')}</label>
              <select>
                <option>{$_('tasks.everyMinute')}</option>
                <option>{$_('tasks.hourly')}</option>
                <option selected>{$_('tasks.daily')}</option>
                <option>{$_('tasks.weekly')}</option>
                <option>{$_('tasks.custom')}</option>
              </select>
            </div>
            <div class="form-group">
              <label>{$_('tasks.time')}</label>
              <input type="time" value="02:00" />
            </div>
            <div class="form-row">
              <div class="form-group">
                <label>{$_('tasks.cronExpression')}</label>
                <input type="text" value="0 2 * * *" readonly />
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  </div>
</div>

<style>
  .tasks-page {
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
  
  .tasks-container {
    flex: 1;
    display: flex;
    gap: 1rem;
    padding: 1rem;
    overflow: hidden;
  }
  
  .tasks-list {
    width: 400px;
    display: flex;
    flex-direction: column;
    padding: 1rem;
  }
  
  .list-header {
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
  
  .task-items {
    flex: 1;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }
  
  .task-item {
    display: flex;
    align-items: flex-start;
    gap: 0.75rem;
    padding: 0.75rem;
    background: var(--bg-primary);
    border-radius: 0.5rem;
    border-left: 3px solid var(--neon-cyan);
    transition: opacity 0.2s ease;
  }
  
  .task-item.disabled {
    opacity: 0.5;
    border-left-color: var(--text-muted);
  }
  
  .task-status {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 0.25rem;
  }
  
  .status-icon {
    font-size: 1rem;
  }
  
  .status-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
  }
  
  .task-info {
    flex: 1;
    min-width: 0;
  }
  
  .task-header {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    margin-bottom: 0.25rem;
  }
  
  .task-name {
    font-weight: 600;
    color: var(--text-primary);
  }
  
  .task-type {
    font-size: 0.625rem;
    padding: 0.125rem 0.375rem;
    background: var(--bg-tertiary);
    border-radius: 0.25rem;
    color: var(--text-muted);
    text-transform: uppercase;
  }
  
  .task-schedule {
    margin-bottom: 0.25rem;
  }
  
  .task-schedule code {
    font-size: 0.75rem;
    color: var(--neon-cyan);
    background: rgba(0, 245, 255, 0.1);
    padding: 0.125rem 0.375rem;
    border-radius: 0.25rem;
  }
  
  .task-meta {
    display: flex;
    flex-direction: column;
    font-size: 0.75rem;
    color: var(--text-muted);
  }
  
  .task-actions {
    display: flex;
    gap: 0.25rem;
  }
  
  .action-btn {
    width: 28px;
    height: 28px;
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--bg-tertiary);
    border: none;
    border-radius: 0.25rem;
    cursor: pointer;
    transition: all 0.2s ease;
  }
  
  .action-btn:hover {
    background: rgba(0, 245, 255, 0.1);
  }
  
  .action-btn.delete:hover {
    background: rgba(255, 0, 110, 0.1);
  }
  
  .action-btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
  
  .flow-editor-panel {
    flex: 1;
    display: flex;
    flex-direction: column;
    padding: 1rem;
  }
  
  .panel-header {
    margin-bottom: 1rem;
  }
  
  .panel-header h2 {
    margin: 0;
    font-size: 1rem;
  }
  
  .editor-content {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 1.5rem;
  }
  
  .placeholder-content {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    text-align: center;
    color: var(--text-muted);
    background: var(--bg-primary);
    border-radius: 0.5rem;
  }
  
  .placeholder-icon {
    font-size: 3rem;
    margin-bottom: 1rem;
    opacity: 0.5;
  }
  
  .schedule-builder {
    background: var(--bg-primary);
    border-radius: 0.5rem;
    padding: 1rem;
  }
  
  .schedule-builder h3 {
    margin: 0 0 1rem;
    font-size: 0.875rem;
    color: var(--text-secondary);
  }
  
  .builder-form {
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
  }
  
  .form-group {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
  }
  
  .form-group label {
    font-size: 0.75rem;
    color: var(--text-muted);
  }
  
  .form-group input,
  .form-group select {
    background: var(--bg-tertiary);
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 0.375rem;
    padding: 0.5rem;
    color: var(--text-primary);
    font-size: 0.875rem;
  }
  
  .form-group input:focus,
  .form-group select:focus {
    outline: none;
    border-color: var(--neon-cyan);
  }
  
  .form-group input[readonly] {
    opacity: 0.7;
    cursor: not-allowed;
  }
  
  .form-row {
    margin-top: 0.5rem;
    padding-top: 0.75rem;
    border-top: 1px solid rgba(255, 255, 255, 0.1);
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
