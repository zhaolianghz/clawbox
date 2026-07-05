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
  import {
    list_mcp_all, add_mcp, remove_mcp,
    type McpServer,
  } from '$lib/api/capabilities/mcp';
  import {
    list_memory_all, memory_index, memory_reset,
    type MemoryStatus,
  } from '$lib/api/capabilities/memory';
  import {
    list_plugins_all, install_plugin, remove_plugin, set_plugin_enabled,
    type Plugin,
  } from '$lib/api/capabilities/plugins';
  import {
    list_hooks_all, set_hook_enabled,
    type Hook,
  } from '$lib/api/capabilities/hooks';
  import type { BackendError } from '$lib/api/capabilities/_shared';

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
  let mcpByBackend = $state<Record<BackendId, McpServer[]>>({ openclaw: [], hermes: [] });
  let memoryByBackend = $state<Record<BackendId, MemoryStatus | null>>({ openclaw: null, hermes: null });
  let pluginsByBackend = $state<Record<BackendId, Plugin[]>>({ openclaw: [], hermes: [] });
  let hooksByBackend = $state<Record<BackendId, Hook[]>>({ openclaw: [], hermes: [] });

  let skillErrors = $state<BackendError[]>([]);
  let toolErrors = $state<BackendError[]>([]);
  let mcpErrors = $state<BackendError[]>([]);
  let memoryErrors = $state<BackendError[]>([]);
  let pluginsErrors = $state<BackendError[]>([]);
  let hooksErrors = $state<BackendError[]>([]);

  let isLoading = $state(true);
  let busyKey = $state<string | null>(null);

  let newMcpName = $state('');
  let newMcpConfig = $state('');
  let newPluginSource = $state('');

  async function load() {
    isLoading = true;
    const [bl, skills, tools, mcp, mem, plugins, hooks] = await Promise.all([
      list_backends(),
      list_skills_all(),
      list_tools_all(),
      list_mcp_all(),
      list_memory_all(),
      list_plugins_all(),
      list_hooks_all(),
    ]);
    backends = bl;
    skillErrors = skills.errors;
    toolErrors = tools.errors;
    mcpErrors = mcp.errors;
    memoryErrors = mem.errors;
    pluginsErrors = plugins.errors;
    hooksErrors = hooks.errors;

    const sm: Record<BackendId, Skill[]> = { openclaw: [], hermes: [] };
    for (const t of skills.items) sm[t.backend].push(t.item);
    skillsByBackend = sm;

    const tm: Record<BackendId, Tool[]> = { openclaw: [], hermes: [] };
    for (const t of tools.items) tm[t.backend].push(t.item);
    toolsByBackend = tm;

    const mm: Record<BackendId, McpServer[]> = { openclaw: [], hermes: [] };
    for (const t of mcp.items) mm[t.backend].push(t.item);
    mcpByBackend = mm;

    const memMap: Record<BackendId, MemoryStatus | null> = { openclaw: null, hermes: null };
    for (const t of mem.items) memMap[t.backend] = t.item;
    memoryByBackend = memMap;

    const pm: Record<BackendId, Plugin[]> = { openclaw: [], hermes: [] };
    for (const t of plugins.items) pm[t.backend].push(t.item);
    pluginsByBackend = pm;

    const hm: Record<BackendId, Hook[]> = { openclaw: [], hermes: [] };
    for (const t of hooks.items) hm[t.backend].push(t.item);
    hooksByBackend = hm;

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

  async function doRemoveMcp(server: McpServer, backend: BackendId) {
    const key = `mcp-remove:${backend}:${server.name}`;
    busyKey = key;
    try {
      await remove_mcp(backend, server.name);
      await load();
    } finally { busyKey = null; }
  }

  async function doAddMcp(backend: BackendId) {
    const name = newMcpName.trim();
    if (!name) return;
    const key = `mcp-add:${backend}`;
    busyKey = key;
    try {
      await add_mcp(backend, name, newMcpConfig);
      newMcpName = '';
      newMcpConfig = '';
      await load();
    } finally { busyKey = null; }
  }

  async function doMemoryIndex(backend: BackendId) {
    const key = `mem-index:${backend}`;
    busyKey = key;
    try { await memory_index(backend); } finally { busyKey = null; }
  }

  async function doMemoryReset(backend: BackendId) {
    const key = `mem-reset:${backend}`;
    busyKey = key;
    try { await memory_reset(backend); } finally { busyKey = null; }
  }

  async function togglePlugin(p: Plugin, backend: BackendId) {
    const key = `plugin:${backend}:${p.id}`;
    busyKey = key;
    try {
      await set_plugin_enabled(backend, p.id, !p.enabled);
      await load();
    } finally { busyKey = null; }
  }

  async function doRemovePlugin(p: Plugin, backend: BackendId) {
    const key = `plugin-remove:${backend}:${p.id}`;
    busyKey = key;
    try {
      await remove_plugin(backend, p.id);
      await load();
    } finally { busyKey = null; }
  }

  async function doInstallPlugin(backend: BackendId) {
    const source = newPluginSource.trim();
    if (!source) return;
    const key = `plugin-install:${backend}`;
    busyKey = key;
    try {
      await install_plugin(backend, source);
      newPluginSource = '';
      await load();
    } finally { busyKey = null; }
  }

  async function toggleHook(h: Hook, backend: BackendId) {
    const key = `hook:${backend}:${h.id}`;
    busyKey = key;
    try {
      await set_hook_enabled(backend, h.id, !h.enabled);
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
        {#if skillErrors.length > 0}
          <div class="errors">
            {#each skillErrors as err (err.backend + ':' + err.message)}
              <p class="error-line">{err.backend}: {err.message}</p>
            {/each}
          </div>
        {/if}
      </div>
    {:else if activeTab === 'mcp'}
      <div class="backend-panels">
        {#each backends as backend (backend.id)}
          <section class="backend-section">
            <header class="backend-header">
              <span class="backend-chip" data-backend={backend.id}>{backend.displayName}</span>
              {#if backend.installed}
                <span class="backend-count">{mcpByBackend[backend.id]?.length ?? 0}</span>
              {:else}
                <span class="empty">{$_('capabilities.notInstalled')}</span>
              {/if}
            </header>

            {#if backend.installed && (mcpByBackend[backend.id]?.length ?? 0) > 0}
              {#each mcpByBackend[backend.id] as s (s.name)}
                {@const key = `mcp-remove:${backend.id}:${s.name}`}
                <div class="item-row">
                  <div class="item-info">
                    <div class="item-name">{s.name}</div>
                    <div class="item-meta">
                      <span class="desc">{s.transport || '—'}</span>
                      <code class="version">{s.status}</code>
                    </div>
                  </div>
                  <div class="item-actions">
                    <button class="action-btn" onclick={() => doRemoveMcp(s, backend.id)} disabled={busyKey === key}
                      title="Remove">🗑️</button>
                  </div>
                </div>
              {/each}
            {:else if backend.installed}
              <p class="empty">{$_('capabilities.noItems')}</p>
            {/if}

            {#if backend.installed}
              <details class="add-form">
                <summary>＋ Add MCP server</summary>
                <div class="form-body">
                  <input
                    class="text-input"
                    type="text"
                    placeholder="name"
                    bind:value={newMcpName}
                  />
                  <textarea
                    class="text-input"
                    rows="3"
                    placeholder='config json (e.g. &lbrace;"command": "uvx my-mcp"&rbrace;)'
                    bind:value={newMcpConfig}
                  ></textarea>
                  <button class="action-btn primary" onclick={() => doAddMcp(backend.id)}
                    disabled={busyKey === `mcp-add:${backend.id}` || !newMcpName.trim()}>
                    Add
                  </button>
                </div>
              </details>
            {/if}
          </section>
        {/each}
        {#if mcpErrors.length > 0}
          <div class="errors">
            {#each mcpErrors as err (err.backend + ':' + err.message)}
              <p class="error-line">{err.backend}: {err.message}</p>
            {/each}
          </div>
        {/if}
      </div>
    {:else if activeTab === 'memory'}
      <div class="backend-panels">
        {#each backends as backend (backend.id)}
          <section class="backend-section">
            <header class="backend-header">
              <span class="backend-chip" data-backend={backend.id}>{backend.displayName}</span>
              {#if !backend.installed}
                <span class="empty">{$_('capabilities.notInstalled')}</span>
              {/if}
            </header>

            {#if backend.installed}
              {@const status = memoryByBackend[backend.id]}
              {#if status}
                <div class="item-row">
                  <div class="item-info">
                    <div class="item-name">{status.provider || '(unknown)'}</div>
                    <div class="item-meta">
                      <span class="desc">built-in:</span>
                      {#if status.builtinActive}
                        <code class="version">active</code>
                      {:else}
                        <span class="empty">inactive</span>
                      {/if}
                    </div>
                  </div>
                  <div class="item-actions">
                    {#if backend.id === 'openclaw'}
                      <button class="action-btn primary" onclick={() => doMemoryIndex(backend.id)}
                        disabled={busyKey === `mem-index:${backend.id}`}
                        title="Index">📥</button>
                    {/if}
                    {#if backend.id === 'hermes'}
                      <button class="action-btn" onclick={() => doMemoryReset(backend.id)}
                        disabled={busyKey === `mem-reset:${backend.id}`}
                        title="Reset">♻️</button>
                    {/if}
                  </div>
                </div>
              {:else}
                <p class="empty">{$_('capabilities.noItems')}</p>
              {/if}
            {/if}
          </section>
        {/each}
        {#if memoryErrors.length > 0}
          <div class="errors">
            {#each memoryErrors as err (err.backend + ':' + err.message)}
              <p class="error-line">{err.backend}: {err.message}</p>
            {/each}
          </div>
        {/if}
      </div>
    {:else if activeTab === 'plugins'}
      <div class="backend-panels">
        {#each backends as backend (backend.id)}
          <section class="backend-section">
            <header class="backend-header">
              <span class="backend-chip" data-backend={backend.id}>{backend.displayName}</span>
              {#if backend.installed}
                <span class="backend-count">{pluginsByBackend[backend.id]?.length ?? 0}</span>
              {:else}
                <span class="empty">{$_('capabilities.notInstalled')}</span>
              {/if}
            </header>

            {#if backend.installed && (pluginsByBackend[backend.id]?.length ?? 0) > 0}
              {#each pluginsByBackend[backend.id] as p (p.id)}
                {@const key = `plugin:${backend.id}:${p.id}`}
                <div class="item-row" class:disabled={!p.enabled}>
                  <div class="item-info">
                    <div class="item-name">{p.name}</div>
                    <div class="item-meta">
                      <code class="version">v{p.version}</code>
                    </div>
                  </div>
                  <div class="item-actions">
                    <button class="action-btn" onclick={() => togglePlugin(p, backend.id)} disabled={busyKey === key}
                      title={p.enabled ? $_('capabilities.skills.disable') : $_('capabilities.skills.enable')}>
                      {p.enabled ? '⏸️' : '▶️'}
                    </button>
                    <button class="action-btn" onclick={() => doRemovePlugin(p, backend.id)} disabled={busyKey === `plugin-remove:${backend.id}:${p.id}`}
                      title="Remove">🗑️</button>
                  </div>
                </div>
              {/each}
            {:else if backend.installed}
              <p class="empty">{$_('capabilities.noItems')}</p>
            {/if}

            {#if backend.installed}
              <div class="inline-form">
                <input
                  class="text-input"
                  type="text"
                  placeholder="source URL or local path"
                  bind:value={newPluginSource}
                />
                <button class="action-btn primary" onclick={() => doInstallPlugin(backend.id)}
                  disabled={busyKey === `plugin-install:${backend.id}` || !newPluginSource.trim()}>
                  Install
                </button>
              </div>
            {/if}
          </section>
        {/each}
        {#if pluginsErrors.length > 0}
          <div class="errors">
            {#each pluginsErrors as err (err.backend + ':' + err.message)}
              <p class="error-line">{err.backend}: {err.message}</p>
            {/each}
          </div>
        {/if}
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
        {#if toolErrors.length > 0}
          <div class="errors">
            {#each toolErrors as err (err.backend + ':' + err.message)}
              <p class="error-line">{err.backend}: {err.message}</p>
            {/each}
          </div>
        {/if}
      </div>
    {:else if activeTab === 'hooks'}
      <div class="backend-panels">
        {#each backends as backend (backend.id)}
          <section class="backend-section">
            <header class="backend-header">
              <span class="backend-chip" data-backend={backend.id}>{backend.displayName}</span>
              {#if backend.installed}
                <span class="backend-count">{hooksByBackend[backend.id]?.length ?? 0}</span>
              {:else}
                <span class="empty">{$_('capabilities.notInstalled')}</span>
              {/if}
            </header>

            {#if backend.id === 'hermes'}
              <p class="hint">Hermes hooks are managed via <code>~/.hermes/config.yaml</code> — toggle the <code>enabled</code> flag there and re-run <code>hermes hooks list</code>.</p>
            {/if}

            {#if backend.installed && (hooksByBackend[backend.id]?.length ?? 0) > 0}
              {#each hooksByBackend[backend.id] as h (h.id)}
                {@const key = `hook:${backend.id}:${h.id}`}
                <div class="item-row" class:disabled={!h.enabled}>
                  <div class="item-info">
                    <div class="item-name">{h.name}</div>
                    <div class="item-meta">
                      <code class="version">{h.event}</code>
                    </div>
                  </div>
                  <div class="item-actions">
                    <button class="action-btn" onclick={() => toggleHook(h, backend.id)} disabled={busyKey === key}
                      title={h.enabled ? $_('capabilities.skills.disable') : $_('capabilities.skills.enable')}>
                      {h.enabled ? '⏸️' : '▶️'}
                    </button>
                  </div>
                </div>
              {/each}
            {:else if backend.installed}
              <p class="empty">{$_('capabilities.noItems')}</p>
            {/if}
          </section>
        {/each}
        {#if hooksErrors.length > 0}
          <div class="errors">
            {#each hooksErrors as err (err.backend + ':' + err.message)}
              <p class="error-line">{err.backend}: {err.message}</p>
            {/each}
          </div>
        {/if}
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
  .hint { color: var(--text-secondary); font-size: 0.75rem; margin: 0.25rem 0; }

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

  .inline-form {
    display: flex;
    gap: 0.5rem;
    align-items: center;
    margin-top: 0.5rem;
  }
  .inline-form .text-input {
    flex: 1;
  }
  .inline-form .action-btn {
    width: auto;
    padding: 0 0.75rem;
    font-size: 0.75rem;
  }

  .add-form {
    margin-top: 0.5rem;
    background: var(--bg-primary);
    border-radius: 0.375rem;
  }
  .add-form summary {
    cursor: pointer;
    padding: 0.375rem 0.5rem;
    font-size: 0.75rem;
    color: var(--text-secondary);
  }
  .add-form summary:hover { color: var(--text-primary); }
  .form-body {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    padding: 0.5rem;
  }
  .form-body .action-btn {
    width: auto;
    align-self: flex-start;
    padding: 0 0.75rem;
    font-size: 0.75rem;
  }

  .text-input {
    background: var(--bg-tertiary);
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 0.25rem;
    color: var(--text-primary);
    padding: 0.375rem 0.5rem;
    font-size: 0.75rem;
    font-family: inherit;
    resize: vertical;
  }
  .text-input:focus { outline: none; border-color: var(--neon-cyan); }

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