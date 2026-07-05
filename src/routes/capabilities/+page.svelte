<script lang="ts">
  import { _ } from 'svelte-i18n';
  import { onMount } from 'svelte';
  import { list_backends, type BackendInfo, type BackendId } from '$lib/api/backends';
  import {
    list_skills_all, install_skill, uninstall_skill, set_skill_enabled,
    type Skill,
  } from '$lib/api/capabilities/skills';
  import {
    list_tools_all, set_tool_enabled,
    type Tool,
  } from '$lib/api/capabilities/tools';

  type TabId = 'skills' | 'mcp' | 'memory' | 'plugins' | 'tools' | 'hooks';

  const tabs: { id: TabId; key: string }[] = [
    { id: 'skills', key: 'capabilities.tab.skills' },
    { id: 'mcp', key: 'capabilities.tab.mcp' },
    { id: 'memory', key: 'capabilities.tab.memory' },
    { id: 'plugins', key: 'capabilities.tab.plugins' },
    { id: 'tools', key: 'capabilities.tab.tools' },
    { id: 'hooks', key: 'capabilities.tab.hooks' },
  ];

  let activeTab = $state<TabId>('skills');
  let backends = $state<BackendInfo[]>([]);
  let skillsByBackend = $state<Record<BackendId, Skill[]>>({ openclaw: [], hermes: [] });
  let toolsByBackend = $state<Record<BackendId, Tool[]>>({ openclaw: [], hermes: [] });
  let errors = $state<{ backend: string; message: string }[]>([]);
  let isLoading = $state(true);
  let busyKey = $state<string | null>(null);

  async function load() {
    isLoading = true;
    const [bl, skills, tools] = await Promise.all([
      list_backends(),
      list_skills_all(),
      list_tools_all(),
    ]);
    backends = bl;
    errors = [...skills.errors, ...tools.errors];
    const sm: Record<BackendId, Skill[]> = { openclaw: [], hermes: [] };
    for (const t of skills.items) sm[t.backend].push(t.item);
    skillsByBackend = sm;
    const tm: Record<BackendId, Tool[]> = { openclaw: [], hermes: [] };
    for (const t of tools.items) tm[t.backend].push(t.item);
    toolsByBackend = tm;
    isLoading = false;
  }

  async function toggleSkill(s: Skill, backend: BackendId) {
    const key = `skill:${backend}:${s.id}`;
    busyKey = key;
    try {
      await set_skill_enabled(backend, s.id, !s.enabled);
      await load();
    } finally { busyKey = null; }
  }

  async function doInstallSkill(s: Skill, backend: BackendId) {
    const key = `skill-install:${backend}:${s.id}`;
    busyKey = key;
    try {
      await install_skill(backend, s.id);
      await load();
    } finally { busyKey = null; }
  }

  async function doUninstallSkill(s: Skill, backend: BackendId) {
    const key = `skill-uninstall:${backend}:${s.id}`;
    busyKey = key;
    try {
      await uninstall_skill(backend, s.id);
      await load();
    } finally { busyKey = null; }
  }

  async function toggleTool(t: Tool, backend: BackendId) {
    const key = `tool:${backend}:${t.id}`;
    busyKey = key;
    try {
      await set_tool_enabled(backend, t.id, !t.enabled);
      await load();
    } finally { busyKey = null; }
  }

  onMount(load);
</script>

<svelte:head>
  <title>{$_('capabilities.title')} - ClawBox</title>
</svelte:head>

<div class="capabilities-page">
  <div class="page-header">
    <h1>{$_('capabilities.title')}</h1>
  </div>

  <div class="tab-bar glass-card">
    {#each tabs as tab (tab.id)}
      <button
        class="tab-btn"
        class:active={activeTab === tab.id}
        onclick={() => (activeTab = tab.id)}
      >
        {$_(tab.key)}
      </button>
    {/each}
  </div>

  <div class="tab-content glass-card">
    {#if isLoading}
      <div class="loading"><div class="spinner"></div></div>
    {:else if activeTab === 'skills'}
      <div class="backend-panels">
        {#each backends as backend (backend.id)}
          <section class="backend-section">
            <header class="backend-header">
              <span class="backend-chip" data-backend={backend.id}>{backend.displayName}</span>
              {#if backend.installed}
                <span class="backend-count">{skillsByBackend[backend.id]?.length ?? 0}</span>
              {:else}
                <span class="empty">{$_('capabilities.notInstalled')}</span>
              {/if}
            </header>

            {#if backend.installed && (skillsByBackend[backend.id]?.length ?? 0) > 0}
              {#each skillsByBackend[backend.id] as s (s.id)}
                {@const key = `skill:${backend.id}:${s.id}`}
                <div class="item-row" class:disabled={!s.enabled}>
                  <div class="item-info">
                    <div class="item-name">{s.name}</div>
                    <div class="item-meta">
                      <code class="version">v{s.version}</code>
                      <span class="desc">{s.description}</span>
                    </div>
                  </div>
                  <div class="item-actions">
                    <button class="action-btn" onclick={() => toggleSkill(s, backend.id)} disabled={busyKey === key}
                      title={s.enabled ? $_('capabilities.skills.disable') : $_('capabilities.skills.enable')}>
                      {s.enabled ? '⏸️' : '▶️'}
                    </button>
                    <button class="action-btn" onclick={() => doUninstallSkill(s, backend.id)} disabled={busyKey === key}
                      title={$_('capabilities.skills.uninstall')}>🗑️</button>
                    <button class="action-btn primary" onclick={() => doInstallSkill(s, backend.id)} disabled={busyKey === key}
                      title={$_('capabilities.skills.install')}>＋</button>
                  </div>
                </div>
              {/each}
            {:else if backend.installed}
              <p class="empty">{$_('capabilities.noItems')}</p>
            {/if}
          </section>
        {/each}
      </div>
    {:else if activeTab === 'tools'}
      <div class="backend-panels">
        {#each backends as backend (backend.id)}
          <section class="backend-section">
            <header class="backend-header">
              <span class="backend-chip" data-backend={backend.id}>{backend.displayName}</span>
              {#if backend.installed}
                <span class="backend-count">{toolsByBackend[backend.id]?.length ?? 0}</span>
              {:else}
                <span class="empty">{$_('capabilities.notInstalled')}</span>
              {/if}
            </header>

            {#if backend.installed && (toolsByBackend[backend.id]?.length ?? 0) > 0}
              {#each toolsByBackend[backend.id] as t (t.id)}
                {@const key = `tool:${backend.id}:${t.id}`}
                <div class="item-row" class:disabled={!t.enabled}>
                  <div class="item-info">
                    <div class="item-name">{t.id}</div>
                  </div>
                  <div class="item-actions">
                    <button class="action-btn" onclick={() => toggleTool(t, backend.id)} disabled={busyKey === key}
                      title={t.enabled ? $_('capabilities.skills.disable') : $_('capabilities.skills.enable')}>
                      {t.enabled ? '⏸️' : '▶️'}
                    </button>
                  </div>
                </div>
              {/each}
            {:else if backend.installed}
              <p class="empty">{$_('capabilities.noItems')}</p>
            {/if}
          </section>
        {/each}
      </div>
    {:else}
      <div class="coming-soon">
        <span class="coming-soon-icon">🚧</span>
        <p>{$_('capabilities.comingSoon')}</p>
      </div>
    {/if}

    {#if errors.length > 0}
      <div class="errors">
        {#each errors as err (err.backend + ':' + err.message)}
          <p class="error-line">{err.backend}: {err.message}</p>
        {/each}
      </div>
    {/if}
  </div>
</div>

<style>
  .capabilities-page {
    display: flex;
    flex-direction: column;
    gap: 1rem;
    margin: -1.5rem;
    padding: 0;
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
    font-weight: 600;
  }

  .tab-bar {
    display: flex;
    gap: 0.25rem;
    padding: 0.5rem;
    margin: 0 1rem;
  }

  .tab-btn {
    flex: 1;
    background: transparent;
    border: none;
    color: var(--text-secondary);
    padding: 0.5rem 0.75rem;
    border-radius: 0.375rem;
    cursor: pointer;
    font-size: 0.875rem;
    transition: all 0.2s ease;
  }

  .tab-btn:hover {
    background: var(--bg-tertiary);
    color: var(--text-primary);
  }

  .tab-btn.active {
    background: rgba(0, 245, 255, 0.1);
    color: var(--neon-cyan);
  }

  .tab-content {
    flex: 1;
    padding: 1rem;
    margin: 0 1rem 1rem;
    overflow-y: auto;
  }

  .backend-panels {
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
  }

  .backend-section {
    background: rgba(0,0,0,0.15);
    border-radius: 0.5rem;
    padding: 0.75rem;
  }

  .backend-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 0.5rem;
    font-size: 0.75rem;
  }

  .backend-chip {
    padding: 0.125rem 0.5rem;
    border-radius: 999px;
    font-weight: 600;
    font-size: 0.7rem;
  }
  .backend-chip[data-backend="openclaw"] { background: rgba(0,245,255,0.15); color: var(--neon-cyan); }
  .backend-chip[data-backend="hermes"]   { background: rgba(255,0,200,0.15); color: #ff6ad5; }

  .backend-count { color: var(--text-muted); }
  .empty { color: var(--text-muted); font-style: italic; margin: 0.25rem 0; }

  .item-row {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.5rem;
    margin-top: 0.25rem;
    background: var(--bg-primary);
    border-radius: 0.375rem;
    border-left: 3px solid var(--neon-cyan);
  }
  .item-row.disabled { opacity: 0.5; border-left-color: var(--text-muted); }

  .item-info { flex: 1; min-width: 0; }
  .item-name { font-weight: 600; color: var(--text-primary); }
  .item-meta {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    margin-top: 0.25rem;
    font-size: 0.75rem;
    color: var(--text-muted);
  }
  .version {
    font-size: 0.75rem;
    color: var(--neon-cyan);
    background: rgba(0,245,255,0.1);
    padding: 0.125rem 0.375rem;
    border-radius: 0.25rem;
  }
  .desc { color: var(--text-secondary); }

  .item-actions { display: flex; gap: 0.25rem; }
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
    color: var(--text-primary);
  }
  .action-btn:hover { background: rgba(0,245,255,0.1); }
  .action-btn.primary:hover { background: rgba(0,245,255,0.2); color: var(--neon-cyan); }
  .action-btn:disabled { opacity: 0.5; cursor: not-allowed; }

  .coming-soon {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 1rem;
    padding: 3rem;
    color: var(--text-muted);
  }
  .coming-soon-icon { font-size: 3rem; opacity: 0.6; }

  .errors {
    margin-top: 1rem;
    padding: 0.5rem;
    background: rgba(255,0,110,0.1);
    border-radius: 0.375rem;
  }
  .error-line { color: var(--neon-magenta); font-size: 0.75rem; margin: 0.25rem 0; }

  .loading { display: flex; justify-content: center; padding: 2rem; }
  .spinner {
    width: 24px;
    height: 24px;
    border: 2px solid var(--bg-tertiary);
    border-top-color: var(--neon-cyan);
    border-radius: 50%;
    animation: spin 1s linear infinite;
  }
  @keyframes spin { to { transform: rotate(360deg); } }
</style>