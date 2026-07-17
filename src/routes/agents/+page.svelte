<script lang="ts">
  import { onMount } from 'svelte';
  import { _ } from 'svelte-i18n';
  import { agents_list, agent_install, type AgentStatus } from '../../lib/api/agents';
  import { checkLatestVersions, extractSemver, type LatestInfo } from '../../lib/api/latest';
  import AgentLogo from '../../lib/components/AgentLogo.svelte';

  let agents = $state<AgentStatus[]>([]);
  let isLoading = $state(true);
  let installing = $state<Record<string, boolean>>({});
  let confirming = $state<string | null>(null); // script 类两步确认:当前展开确认的 agent id
  let errors = $state<Record<string, string>>({});
  let latest = $state<Record<string, LatestInfo>>({});
  let checkingLatest = $state(false);
  let justUpgraded = $state<Record<string, boolean>>({}); // 升级成功后版本号短暂高亮

  async function refresh() {
    isLoading = true;
    try {
      const all = await agents_list();
      // Node.js 是运行时依赖,不是 agent;仅用于依赖检测,不在 agent 列表里展示。
      agents = all.filter((a) => a.kind !== 'runtime');
    } catch (e) {
      console.error('agents_list failed', e);
    } finally {
      isLoading = false;
    }
  }

  async function checkUpdates(force: boolean) {
    if (agents.length === 0 || checkingLatest) return;
    checkingLatest = true;
    try {
      latest = await checkLatestVersions(agents, force);
    } finally {
      checkingLatest = false;
    }
  }

  onMount(async () => {
    await refresh();
    // 页面加载时只消费缓存(1h TTL 内零请求);强制刷新走「检查更新」按钮
    checkUpdates(false);
  });

  function isScript(a: AgentStatus): boolean {
    // unix: curl ... | bash ; windows: irm ... | iex
    const cmd = a.install_command ?? '';
    return cmd.startsWith('curl') || cmd.startsWith('irm');
  }

  async function install(a: AgentStatus) {
    // Script 安装第一击只展开命令确认,第二击才执行
    if (isScript(a) && confirming !== a.id) {
      confirming = a.id;
      return;
    }
    confirming = null;
    const prevVersion = a.version;
    installing = { ...installing, [a.id]: true };
    errors = { ...errors, [a.id]: '' };
    try {
      await agent_install(a.id);
      await refresh();
      // 升级后重新和 registry 比对(用缓存即可,latest 不会因本地安装而变)
      checkUpdates(false);
      const now = agents.find((x) => x.id === a.id);
      if (now?.installed && now.version !== prevVersion) {
        justUpgraded = { ...justUpgraded, [a.id]: true };
        setTimeout(() => (justUpgraded = { ...justUpgraded, [a.id]: false }), 4000);
      }
    } catch (e) {
      errors = { ...errors, [a.id]: String(e) };
    } finally {
      installing = { ...installing, [a.id]: false };
    }
  }

  function kindLabel(k: AgentStatus['kind']): string {
    return $_(`agents.kind.${k}`);
  }

  /** 已装 npm 类的按钮文案状态:'update' 有新版 / 'latest' 已最新 / 'unknown' 未知 */
  function upgradeState(a: AgentStatus): 'update' | 'latest' | 'unknown' {
    const info = latest[a.id];
    if (!info || info.latest === null || extractSemver(a.version) === null) return 'unknown';
    return info.hasUpdate ? 'update' : 'latest';
  }
</script>

<div class="agents-page">
  <header class="page-header">
    <h1>{$_('agents.title')}</h1>
    <p class="subtitle">{$_('agents.subtitle')}</p>
    <button class="refresh-btn" onclick={() => checkUpdates(true)} disabled={checkingLatest || isLoading}>
      {#if checkingLatest}<span class="spinner small"></span>{/if}
      {$_('agents.checkUpdates')}
    </button>
    <button class="refresh-btn" onclick={refresh} disabled={isLoading}>
      {$_('agents.refresh')}
    </button>
  </header>

  {#if isLoading && agents.length === 0}
    <div class="loading glass-card"><span class="spinner"></span> {$_('agents.loading')}</div>
  {:else}
    <div class="agent-grid">
      {#each agents as a (a.id)}
        <div class="glass-card agent-card">
          <div class="card-head">
            <AgentLogo id={a.id} label={a.label} />
            <div class="head-info">
              <div class="title-line">
                <span class="agent-label">{a.label}</span>
                <span class="kind-badge kind-{a.kind}">{kindLabel(a.kind)}</span>
              </div>
              {#if a.installed}
                <span class="version" class:upgraded={justUpgraded[a.id]} title={a.version}>
                  {a.version}
                  {#if upgradeState(a) === 'update'}
                    <span class="update-badge">→ {latest[a.id].latest}</span>
                  {/if}
                </span>
              {:else}
                <span class="not-installed">{$_('agents.notInstalled')}</span>
              {/if}
            </div>
          </div>
          <div class="card-body">
            {#if a.install_command}
              <code class="install-cmd" title={a.install_command}>{a.install_command}</code>
            {:else}
              <span class="detect-only">{$_('agents.detectOnly')}</span>
            {/if}
          </div>
          <div class="card-actions">
            {#if a.install_command}
              {#if !a.deps_satisfied}
                <span class="deps-hint">{$_('agents.missingDeps')}: {a.missing_deps.join(', ')}</span>
              {:else if confirming === a.id}
                <button class="btn danger" onclick={() => install(a)}>
                  {$_('agents.confirmRun')}
                </button>
                <button class="btn" onclick={() => (confirming = null)}>{$_('agents.cancel')}</button>
              {:else}
                {@const st = a.installed ? upgradeState(a) : null}
                <button
                  class="btn"
                  class:primary={!a.installed || st === 'update'}
                  class:subtle={st === 'latest'}
                  onclick={() => install(a)}
                  disabled={installing[a.id]}
                  title={st === 'latest' ? $_('agents.reinstallHint') : undefined}
                >
                  {#if installing[a.id]}
                    <span class="spinner small"></span>
                  {:else if !a.installed}
                    {$_('agents.install')}
                  {:else if st === 'latest'}
                    {$_('agents.upToDate')}
                  {:else}
                    {$_('agents.upgrade')}
                  {/if}
                </button>
              {/if}
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
  .agent-grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 0.9rem;
  }
  @media (max-width: 720px) {
    .agent-grid { grid-template-columns: 1fr; }
  }
  .agent-card { padding: 1.1rem; display: flex; flex-direction: column; gap: 0.7rem; }
  .card-head { display: flex; align-items: center; gap: 0.8rem; }
  .head-info { display: flex; flex-direction: column; gap: 0.2rem; min-width: 0; }
  .title-line { display: flex; align-items: center; gap: 0.5rem; }
  .agent-label { font-weight: 600; }
  .kind-badge { font-size: 0.7rem; padding: 0.1rem 0.5rem; border-radius: 999px; background: rgba(94, 234, 212, 0.15); color: #5eead4; white-space: nowrap; }
  .version { font-family: monospace; font-size: 0.75rem; opacity: 0.7; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; transition: color 0.3s; }
  .version.upgraded { color: #4ade80; opacity: 1; }
  .update-badge { color: #fbbf24; margin-left: 0.3rem; }
  .not-installed { font-size: 0.8rem; color: #fbbf24; }
  .card-body { min-height: 1.2rem; }
  .install-cmd { font-size: 0.72rem; opacity: 0.55; display: block; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .card-actions { display: flex; align-items: center; gap: 0.75rem; flex-wrap: wrap; margin-top: auto; }
  .deps-hint { font-size: 0.75rem; color: #fbbf24; }
  .detect-only { font-size: 0.8rem; opacity: 0.5; }
  .docs-link { font-size: 0.8rem; color: #5eead4; margin-left: auto; }
  .install-error { font-size: 0.75rem; color: #f87171; white-space: pre-wrap; margin: 0; }
  .btn { padding: 0.3rem 0.9rem; border-radius: 6px; border: 1px solid rgba(255,255,255,0.15); background: transparent; color: inherit; cursor: pointer; }
  .btn.primary { background: rgba(94, 234, 212, 0.15); border-color: #5eead4; color: #5eead4; }
  .btn.danger { background: rgba(248, 113, 113, 0.15); border-color: #f87171; color: #f87171; }
  .btn.subtle { opacity: 0.55; }
  .btn:disabled { opacity: 0.5; cursor: default; }
  .loading { padding: 2rem; display: flex; justify-content: center; gap: 0.5rem; }
  .spinner { width: 16px; height: 16px; border: 2px solid rgba(94,234,212,0.3); border-top-color: #5eead4; border-radius: 50%; animation: spin 0.8s linear infinite; display: inline-block; }
  .spinner.small { width: 12px; height: 12px; }
  @keyframes spin { to { transform: rotate(360deg); } }
</style>
