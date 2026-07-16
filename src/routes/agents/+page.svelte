<script lang="ts">
  import { onMount } from 'svelte';
  import { _ } from 'svelte-i18n';
  import { agents_list, agent_install, type AgentStatus } from '../../lib/api/agents';

  let agents = $state<AgentStatus[]>([]);
  let isLoading = $state(true);
  let installing = $state<Record<string, boolean>>({});
  let confirming = $state<string | null>(null); // script 类两步确认:当前展开确认的 agent id
  let errors = $state<Record<string, string>>({});

  async function refresh() {
    isLoading = true;
    try {
      agents = await agents_list();
    } catch (e) {
      console.error('agents_list failed', e);
    } finally {
      isLoading = false;
    }
  }

  onMount(refresh);

  function isScript(a: AgentStatus): boolean {
    return a.install_command?.startsWith('curl') ?? false;
  }

  async function install(a: AgentStatus) {
    // Script 安装第一击只展开命令确认,第二击才执行
    if (isScript(a) && confirming !== a.id) {
      confirming = a.id;
      return;
    }
    confirming = null;
    installing = { ...installing, [a.id]: true };
    errors = { ...errors, [a.id]: '' };
    try {
      await agent_install(a.id);
      await refresh();
    } catch (e) {
      errors = { ...errors, [a.id]: String(e) };
    } finally {
      installing = { ...installing, [a.id]: false };
    }
  }

  function kindLabel(k: AgentStatus['kind']): string {
    return $_(`agents.kind.${k}`);
  }
</script>

<div class="agents-page">
  <header class="page-header">
    <h1>{$_('agents.title')}</h1>
    <p class="subtitle">{$_('agents.subtitle')}</p>
    <button class="refresh-btn" onclick={refresh} disabled={isLoading}>
      {$_('agents.refresh')}
    </button>
  </header>

  {#if isLoading && agents.length === 0}
    <div class="loading glass-card"><span class="spinner"></span> {$_('agents.loading')}</div>
  {:else}
    <div class="agent-list">
      {#each agents as a (a.id)}
        <div class="glass-card agent-row">
          <div class="agent-main">
            <span class="agent-label">{a.label}</span>
            <span class="kind-badge kind-{a.kind}">{kindLabel(a.kind)}</span>
            {#if a.installed}
              <span class="version">{a.version}</span>
            {:else}
              <span class="not-installed">{$_('agents.notInstalled')}</span>
            {/if}
          </div>
          <div class="agent-actions">
            {#if a.install_command}
              <code class="install-cmd">{a.install_command}</code>
              {#if !a.deps_satisfied}
                <span class="deps-hint">{$_('agents.missingDeps')}: {a.missing_deps.join(', ')}</span>
              {:else if confirming === a.id}
                <button class="btn danger" onclick={() => install(a)}>
                  {$_('agents.confirmRun')}
                </button>
                <button class="btn" onclick={() => (confirming = null)}>{$_('agents.cancel')}</button>
              {:else}
                <button class="btn primary" onclick={() => install(a)} disabled={installing[a.id]}>
                  {#if installing[a.id]}
                    <span class="spinner small"></span>
                  {:else}
                    {a.installed ? $_('agents.upgrade') : $_('agents.install')}
                  {/if}
                </button>
              {/if}
            {:else}
              <span class="detect-only">{$_('agents.detectOnly')}</span>
            {/if}
            {#if a.docs_url}
              <a class="docs-link" href={a.docs_url} target="_blank" rel="noreferrer">{$_('agents.docs')}</a>
            {/if}
          </div>
          {#if errors[a.id]}
            <pre class="install-error">{errors[a.id]}</pre>
          {/if}
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .agents-page { padding: 1.5rem; display: flex; flex-direction: column; gap: 1rem; }
  .page-header { display: flex; align-items: baseline; gap: 1rem; }
  .page-header h1 { margin: 0; }
  .subtitle { opacity: 0.6; flex: 1; }
  .agent-list { display: flex; flex-direction: column; gap: 0.75rem; }
  .agent-row { padding: 1rem; display: flex; flex-direction: column; gap: 0.5rem; }
  .agent-main { display: flex; align-items: center; gap: 0.75rem; }
  .agent-label { font-weight: 600; }
  .kind-badge { font-size: 0.7rem; padding: 0.1rem 0.5rem; border-radius: 999px; background: rgba(94, 234, 212, 0.15); color: #5eead4; }
  .version { font-family: monospace; font-size: 0.8rem; opacity: 0.7; }
  .not-installed { font-size: 0.8rem; color: #fbbf24; }
  .agent-actions { display: flex; align-items: center; gap: 0.75rem; flex-wrap: wrap; }
  .install-cmd { font-size: 0.75rem; opacity: 0.55; }
  .deps-hint { font-size: 0.75rem; color: #fbbf24; }
  .detect-only { font-size: 0.8rem; opacity: 0.5; }
  .docs-link { font-size: 0.8rem; color: #5eead4; }
  .install-error { font-size: 0.75rem; color: #f87171; white-space: pre-wrap; margin: 0; }
  .btn { padding: 0.3rem 0.9rem; border-radius: 6px; border: 1px solid rgba(255,255,255,0.15); background: transparent; color: inherit; cursor: pointer; }
  .btn.primary { background: rgba(94, 234, 212, 0.15); border-color: #5eead4; color: #5eead4; }
  .btn.danger { background: rgba(248, 113, 113, 0.15); border-color: #f87171; color: #f87171; }
  .btn:disabled { opacity: 0.5; cursor: default; }
  .loading { padding: 2rem; display: flex; justify-content: center; gap: 0.5rem; }
  .spinner { width: 16px; height: 16px; border: 2px solid rgba(94,234,212,0.3); border-top-color: #5eead4; border-radius: 50%; animation: spin 0.8s linear infinite; display: inline-block; }
  .spinner.small { width: 12px; height: 12px; }
  @keyframes spin { to { transform: rotate(360deg); } }
</style>
