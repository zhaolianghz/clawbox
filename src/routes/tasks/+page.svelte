<script lang="ts">
  import { _ } from 'svelte-i18n';
  import { onMount } from 'svelte';
  import {
    list_cron_all, add_cron, remove_cron, set_cron_enabled, run_cron,
    type TaggedCronJob, type NewCron,
  } from '$lib/api/cron';
  import { list_backends, type BackendInfo, type BackendId } from '$lib/api/backends';

  let backends = $state<BackendInfo[]>([]);
  let grouped = $state<Record<string, TaggedCronJob[]>>({});
  let errors = $state<{ backend: string; message: string }[]>([]);
  let isLoading = $state(true);
  let busyId = $state<string | null>(null);

  let newName = $state('');
  let newSchedule = $state('0 2 * * *');
  let newMessage = $state('');
  let newBackend = $state<BackendId>('openclaw');
  let formError = $state('');
  let creating = $state(false);

  async function loadTasks() {
    isLoading = true;
    const [bl, cl] = await Promise.all([list_backends(), list_cron_all()]);
    backends = bl;
    errors = cl.errors;
    const map: Record<string, TaggedCronJob[]> = {};
    for (const b of bl) map[b.id] = [];
    for (const t of cl.jobs) {
      (map[t.backend] ??= []).push(t);
    }
    grouped = map;
    if (bl.length > 0 && !bl.some((b) => b.id === newBackend && b.installed)) {
      const firstInstalled = bl.find((b) => b.installed);
      if (firstInstalled) newBackend = firstInstalled.id;
    }
    isLoading = false;
  }

  async function toggleTask(t: TaggedCronJob) {
    const key = `${t.backend}:${t.job.id}`;
    busyId = key;
    try {
      await set_cron_enabled(t.backend, t.job.id, !t.job.enabled);
      await loadTasks();
    } finally {
      busyId = null;
    }
  }

  async function deleteTask(t: TaggedCronJob) {
    const key = `${t.backend}:${t.job.id}`;
    busyId = key;
    try {
      await remove_cron(t.backend, t.job.id);
      await loadTasks();
    } finally {
      busyId = null;
    }
  }

  async function runTask(t: TaggedCronJob) {
    const key = `${t.backend}:${t.job.id}`;
    busyId = key;
    try {
      await run_cron(t.backend, t.job.id);
    } finally {
      busyId = null;
    }
  }

  async function createTask() {
    formError = '';
    if (!newName.trim()) { formError = 'Name is required'; return; }
    if (!newSchedule.trim()) { formError = 'A schedule is required'; return; }
    creating = true;
    try {
      const params: NewCron = {
        name: newName.trim(),
        schedule: newSchedule.trim(),
        message: newMessage.trim() || undefined,
      };
      await add_cron(newBackend, params);
      newName = '';
      newMessage = '';
      await loadTasks();
    } catch (e) {
      formError = e instanceof Error ? e.message : String(e);
    } finally {
      creating = false;
    }
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
  </div>

  <div class="tasks-container">
    <div class="tasks-list glass-card">
      <div class="list-header">
        <span>{$_('tasks.scheduledTasks')}</span>
        <span class="count">{Object.values(grouped).reduce((n, list) => n + list.length, 0)}</span>
      </div>

      {#if isLoading}
        <div class="loading"><div class="spinner"></div></div>
      {:else}
        <div class="task-items">
          {#each backends as backend (backend.id)}
            <section class="backend-section">
              <header class="backend-header">
                <span class="backend-chip" data-backend={backend.id}>{backend.displayName}</span>
                {#if backend.installed}
                  <span class="backend-count">
                    {grouped[backend.id]?.length ?? 0} jobs
                  </span>
                {:else}
                  <span class="empty">{$_('backend.notInstalled')}</span>
                {/if}
              </header>

              {#if backend.installed && (grouped[backend.id]?.length ?? 0) > 0}
                {#each grouped[backend.id] as t (t.backend + ':' + t.job.id)}
                  {@const key = t.backend + ':' + t.job.id}
                  <div class="task-item" class:disabled={!t.job.enabled}>
                    <div class="task-info">
                      <div class="task-header">
                        <span class="task-name">{t.job.name}</span>
                      </div>
                      <div class="task-schedule">
                        <code>{t.job.schedule}</code>
                      </div>
                    </div>
                    <div class="task-actions">
                      <button class="action-btn" onclick={() => toggleTask(t)} disabled={busyId === key}
                        title={t.job.enabled ? $_('tasks.disable') : $_('tasks.enable')}>
                        {t.job.enabled ? '⏸️' : '▶️'}
                      </button>
                      <button class="action-btn" onclick={() => runTask(t)} disabled={busyId === key}
                        title="Run now">▶️</button>
                      <button class="action-btn delete" onclick={() => deleteTask(t)} disabled={busyId === key}
                        title={$_('tasks.delete')}>🗑️</button>
                    </div>
                  </div>
                {/each}
              {/if}
            </section>
          {/each}

          {#if errors.length > 0}
            <div class="errors">
              {#each errors as err (err.backend + ':' + err.message)}
                <p class="error-line">{err.backend}: {err.message}</p>
              {/each}
            </div>
          {/if}
        </div>
      {/if}
    </div>

    <div class="flow-editor-panel glass-card">
      <div class="panel-header">
        <h2>{$_('tasks.scheduleBuilder')}</h2>
      </div>

      <div class="editor-content">
        <form class="builder-form" onsubmit={createTask}>
          <div class="form-group">
            <label>{$_('task.newBackend')}</label>
            <select bind:value={newBackend}>
              {#each backends.filter((b) => b.installed) as b (b.id)}
                <option value={b.id}>{b.displayName}</option>
              {/each}
            </select>
          </div>
          <div class="form-group">
            <label>{$_('tasks.newTask')}</label>
            <input type="text" bind:value={newName} placeholder="Daily report" required />
          </div>
          <div class="form-group">
            <label>{$_('tasks.cronExpression')}</label>
            <input id="task-cron" type="text" bind:value={newSchedule} placeholder="0 2 * * *" required />
          </div>
          <div class="form-group">
            <label>Message (optional)</label>
            <input type="text" bind:value={newMessage} placeholder="What to run" />
          </div>
          {#if formError}
            <p class="form-error">{formError}</p>
          {/if}
          <button class="neon-button" type="submit" disabled={creating}>
            {creating ? '...' : $_('tasks.newTask')}
          </button>
        </form>
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

  .page-header h1 { margin: 0; font-size: 1.25rem; }

  .subtitle { margin: 0.25rem 0 0; color: var(--text-muted); font-size: 0.875rem; }

  .tasks-container {
    flex: 1;
    display: flex;
    gap: 1rem;
    padding: 1rem;
    overflow: hidden;
  }

  .tasks-list { width: 460px; display: flex; flex-direction: column; padding: 1rem; }

  .list-header {
    display: flex; justify-content: space-between; align-items: center;
    margin-bottom: 1rem; font-weight: 600;
  }

  .count {
    background: var(--bg-tertiary);
    padding: 0.125rem 0.5rem; border-radius: 0.25rem;
    font-size: 0.75rem; color: var(--text-muted);
  }

  .task-items {
    flex: 1; overflow-y: auto;
    display: flex; flex-direction: column; gap: 0.5rem;
  }

  .backend-section {
    background: rgba(0,0,0,0.15);
    border-radius: 0.5rem;
    padding: 0.5rem;
  }

  .backend-header {
    display: flex; justify-content: space-between; align-items: center;
    margin-bottom: 0.5rem; font-size: 0.75rem;
  }

  .backend-chip {
    padding: 0.125rem 0.5rem; border-radius: 999px;
    font-weight: 600; font-size: 0.7rem;
  }
  .backend-chip[data-backend="openclaw"] { background: rgba(0,245,255,0.15); color: var(--neon-cyan); }
  .backend-chip[data-backend="hermes"]   { background: rgba(255,0,200,0.15); color: #ff6ad5; }

  .backend-count { color: var(--text-muted); }
  .empty { color: var(--text-muted); font-style: italic; }

  .task-item {
    display: flex; align-items: center; gap: 0.5rem;
    padding: 0.5rem; background: var(--bg-primary);
    border-radius: 0.375rem; border-left: 3px solid var(--neon-cyan);
  }
  .task-item.disabled { opacity: 0.5; border-left-color: var(--text-muted); }

  .task-info { flex: 1; min-width: 0; }
  .task-header { display: flex; align-items: center; gap: 0.5rem; }
  .task-name { font-weight: 600; color: var(--text-primary); }
  .task-schedule code {
    font-size: 0.75rem; color: var(--neon-cyan);
    background: rgba(0,245,255,0.1); padding: 0.125rem 0.375rem; border-radius: 0.25rem;
  }

  .task-actions { display: flex; gap: 0.25rem; }
  .action-btn {
    width: 28px; height: 28px; display: flex; align-items: center; justify-content: center;
    background: var(--bg-tertiary); border: none; border-radius: 0.25rem; cursor: pointer;
  }
  .action-btn:hover { background: rgba(0,245,255,0.1); }
  .action-btn.delete:hover { background: rgba(255,0,110,0.1); }
  .action-btn:disabled { opacity: 0.5; cursor: not-allowed; }

  .errors {
    margin-top: 0.5rem; padding: 0.5rem;
    background: rgba(255,0,110,0.1); border-radius: 0.375rem;
  }
  .error-line { color: var(--neon-magenta); font-size: 0.75rem; margin: 0.25rem 0; }

  .flow-editor-panel { flex: 1; display: flex; flex-direction: column; padding: 1rem; }
  .panel-header { margin-bottom: 1rem; }
  .panel-header h2 { margin: 0; font-size: 1rem; }
  .editor-content { flex: 1; }
  .builder-form { display: flex; flex-direction: column; gap: 0.75rem; }

  .form-group { display: flex; flex-direction: column; gap: 0.25rem; }
  .form-group label { font-size: 0.75rem; color: var(--text-muted); }
  .form-group input, .form-group select {
    background: var(--bg-tertiary);
    border: 1px solid rgba(255,255,255,0.1);
    border-radius: 0.375rem; padding: 0.5rem; color: var(--text-primary); font-size: 0.875rem;
  }
  .form-group input:focus, .form-group select:focus { outline: none; border-color: var(--neon-cyan); }

  .form-error { color: var(--neon-magenta); font-size: 0.75rem; margin: 0; }

  .neon-button {
    background: var(--neon-cyan); color: var(--bg-primary);
    border: none; padding: 0.5rem 1rem; border-radius: 0.375rem;
    font-weight: 600; cursor: pointer;
  }
  .neon-button:disabled { opacity: 0.5; cursor: not-allowed; }

  .loading { display: flex; justify-content: center; padding: 2rem; }
  .spinner {
    width: 24px; height: 24px;
    border: 2px solid var(--bg-tertiary); border-top-color: var(--neon-cyan);
    border-radius: 50%; animation: spin 1s linear infinite;
  }
  @keyframes spin { to { transform: rotate(360deg); } }
</style>
