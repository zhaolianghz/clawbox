<script lang="ts">
  import { onMount } from 'svelte';
  import { _ } from 'svelte-i18n';
  import { agents_list, agent_install, type AgentStatus } from '../../lib/api/agents';
  import { checkLatestVersions, extractSemver, type LatestInfo } from '../../lib/api/latest';
  import { agent_sync_overview, type AgentSyncOverview, type SyncedItem } from '../../lib/api/providerSync';
  import AgentLogo from '../../lib/components/AgentLogo.svelte';
  import { providers, loadProviders } from '../../lib/stores/config';
  import { agent_provider_bind, agent_providers_get } from '../../lib/api/providerSync';
  import type { ModelProvider } from '../../lib/api/config';

  let agents = $state<AgentStatus[]>([]);
  let isLoading = $state(true);
  let probing = $state(false); // 后台静默重探测中(有缓存兜底时不置 isLoading)
  let installing = $state<Record<string, boolean>>({});
  let confirming = $state<string | null>(null); // script 类两步确认:当前展开确认的 agent id
  let errors = $state<Record<string, string>>({});
  let latest = $state<Record<string, LatestInfo>>({});
  let checkingLatest = $state(false);
  let justUpgraded = $state<Record<string, boolean>>({}); // 升级成功后版本号短暂高亮

  // 上次探测结果缓存:打开页面先渲染旧状态,探测(最慢的 CLI ~2s)后台跑完再覆盖
  const STATUS_CACHE_KEY = 'clawbox.agents.status';

  function readStatusCache(): AgentStatus[] | null {
    try {
      const raw = localStorage.getItem(STATUS_CACHE_KEY);
      const parsed = raw ? (JSON.parse(raw) as AgentStatus[]) : null;
      return Array.isArray(parsed) && parsed.length > 0 ? parsed : null;
    } catch {
      return null;
    }
  }

  async function refresh() {
    // 有缓存兜底时静默刷新,不清列表不闪全屏 spinner
    isLoading = agents.length === 0;
    probing = true;
    try {
      const all = await agents_list();
      // Node.js 是运行时依赖,不是 agent;仅用于依赖检测,不在 agent 列表里展示。
      agents = all.filter((a) => a.kind !== 'runtime');
      try {
        localStorage.setItem(STATUS_CACHE_KEY, JSON.stringify(agents));
      } catch { /* storage 满/禁用则下次仍走全屏加载 */ }
    } catch (e) {
      console.error('agents_list failed', e);
    } finally {
      isLoading = false;
      probing = false;
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

  onMount(() => {
    // 秒开:先渲染上次探测结果,真实探测后台进行
    const cached = readStatusCache();
    if (cached) {
      agents = cached;
      isLoading = false;
    }
    // 服务商列表/绑定与 agent 探测互不依赖:并行启动,不让 2s+ 的探测
    // 拖住服务商选择器的首次渲染
    void loadProviders();
    agent_providers_get()
      .then((b) => (bindings = b))
      .catch((e) => console.warn('agent_providers_get failed', e));
    void refresh().then(() => {
      // 版本徽章依赖探测结果;页面加载时只消费缓存(1h TTL 内零请求)
      checkUpdates(false);
    });
  });

  // ---------- 服务商绑定(选中即生效) ----------
  let bindings = $state<Record<string, string>>({});
  let bindApplying = $state<Record<string, boolean>>({});
  let bindErrors = $state<Record<string, string>>({});
  let bindFlash = $state<Record<string, boolean>>({}); // 成功短暂高亮

  // 各 agent 的端点槽位偏好(与 src-tauri/src/sync/providers.rs 各适配器一致;
  // 不在表里的 agent 不支持服务商下发,不渲染选择器)
  const AGENT_SLOTS: Record<string, ('anthropic' | 'openai')[]> = {
    'claude-code': ['anthropic'],
    codex: ['openai'],
    codebuddy: ['openai'],
    hermes: ['anthropic', 'openai'],
    // gemini-cli 走 Gemini 协议,端点取 Anthropic 槽的网关根 URL
    gemini: ['anthropic'],
    // cline 经 `cline auth -p anthropic -b <url>`;pi 双协议,Anthropic 优先
    cline: ['anthropic'],
    pi: ['anthropic', 'openai'],
    opencode: ['openai', 'anthropic'],
    openclaw: ['anthropic', 'openai'],
    kimi: ['openai', 'anthropic'],
  };

  /** 该服务商能否下发给该 agent:任一偏好槽已配置端点,且有 API key */
  function compatible(agentId: string, p: ModelProvider): boolean {
    const slots = AGENT_SLOTS[agentId];
    if (!slots || !p.apiKey) return false;
    return slots.some((s) => (s === 'anthropic' ? !!p.anthropicBaseUrl : !!p.openaiBaseUrl));
  }

  const enabledProviders = $derived($providers.filter((p) => p.enabled));

  async function bindProvider(agentId: string, providerId: string) {
    if (providerId === '') return; // 占位符不可选;UI 不提供解绑入口
    const prev = bindings[agentId] ?? '';
    bindApplying = { ...bindApplying, [agentId]: true };
    bindErrors = { ...bindErrors, [agentId]: '' };
    try {
      await agent_provider_bind(agentId, providerId);
      bindings = { ...bindings, [agentId]: providerId };
      bindFlash = { ...bindFlash, [agentId]: true };
      setTimeout(() => (bindFlash = { ...bindFlash, [agentId]: false }), 2000);
      if (overview !== null) void loadOverview(true); // 漂移状态跟着刷新
    } catch (e) {
      bindErrors = { ...bindErrors, [agentId]: String(e) };
      bindings = prev === '' ? (({ [agentId]: _drop, ...rest }) => rest)(bindings) : { ...bindings, [agentId]: prev };
    } finally {
      bindApplying = { ...bindApplying, [agentId]: false };
    }
  }

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

  // ---------- 同步详情内联展开(全局无弹窗;面板整行插在所点卡片所在行的行尾之后) ----------
  let syncDetailId = $state<string | null>(null); // 当前展开的卡片;再点收起
  let overview = $state<Record<string, AgentSyncOverview> | null>(null); // null = 未加载(页面级缓存)
  let overviewLoading = $state(false);
  let overviewError = $state('');

  // 网格实际列数:读浏览器解析后的 grid-template-columns(兼容媒体查询变列),
  // ResizeObserver 跟踪变化——面板插入位置(行尾)依赖它。
  let gridEl = $state<HTMLElement | null>(null);
  let gridCols = $state(1);
  $effect(() => {
    if (!gridEl) return;
    const update = () => {
      gridCols = getComputedStyle(gridEl!).gridTemplateColumns.split(' ').filter(Boolean).length || 1;
    };
    update();
    const ro = new ResizeObserver(update);
    ro.observe(gridEl);
    return () => ro.disconnect();
  });

  /** 展开卡片所在行的行尾索引;面板渲染在该索引的卡片之后(整行插入,不挤动同行卡片) */
  const syncRowEnd = $derived.by(() => {
    if (syncDetailId === null) return -1;
    const idx = agents.findIndex((a) => a.id === syncDetailId);
    if (idx < 0) return -1;
    return Math.min(Math.floor(idx / gridCols) * gridCols + gridCols - 1, agents.length - 1);
  });

  /** 首次展开时拉一次全量总览并缓存;force 供展开区「刷新」按钮重拉 */
  async function loadOverview(force = false) {
    if (overviewLoading) return;
    if (overview !== null && !force) return;
    overviewLoading = true;
    overviewError = '';
    try {
      const list = await agent_sync_overview();
      overview = Object.fromEntries(list.map((o) => [o.agent_id, o]));
    } catch (e) {
      // 后端未就绪(命令不存在)或读文件失败:展开区显示红字,不炸页面
      overviewError = String(e);
    } finally {
      overviewLoading = false;
    }
  }

  function toggleSyncDetail(id: string) {
    if (syncDetailId === id) {
      syncDetailId = null;
      return;
    }
    syncDetailId = id;
    void loadOverview();
  }

  /** 数据未加载时先渲染按钮(点击即触发加载);加载后仅 supported 的 agent 保留 */
  function showSyncDetailBtn(id: string): boolean {
    if (overview === null) return true;
    const o = overview[id];
    return !!o && (o.provider_supported || o.mcp_supported);
  }

  /** 条目显示名:后端个别条目是配置键名(openclaw/kimi 的默认模型键),转成人话 */
  function itemLabel(name: string): string {
    if (name === 'agents.defaults.model' || name === 'default_model') {
      return $_('agents.syncDetail.defaultModel');
    }
    return name;
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
    <button class="refresh-btn" onclick={refresh} disabled={probing}>
      {#if probing}<span class="spinner small"></span>{/if}
      {$_('agents.refresh')}
    </button>
  </header>

  {#if isLoading && agents.length === 0}
    <div class="loading glass-card"><span class="spinner"></span> {$_('agents.loading')}</div>
  {:else}
    <div class="agent-grid" bind:this={gridEl}>
      {#each agents as a, i (a.id)}
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
          {#if AGENT_SLOTS[a.id]}
            <div class="provider-bind" class:flash={bindFlash[a.id]}>
              <span class="bind-label">{$_('agents.provider.label')}</span>
              <select
                class="bind-select"
                class:empty={!bindings[a.id]}
                disabled={bindApplying[a.id]}
                value={bindings[a.id] ?? ''}
                onchange={(e) => bindProvider(a.id, e.currentTarget.value)}
              >
                <!-- 未绑定时的占位:不可选,绑定后自动隐藏(不提供解绑入口) -->
                <option value="" disabled hidden>{$_('agents.provider.placeholder')}</option>
                {#each enabledProviders as p (p.id)}
                  <option value={p.id} disabled={!compatible(a.id, p)}>
                    {p.name}{compatible(a.id, p) ? '' : ` (${$_('agents.provider.incompatible')})`}
                  </option>
                {/each}
                {#if bindings[a.id] && !enabledProviders.some((p) => p.id === bindings[a.id])}
                  <!-- 绑定的服务商已被禁用:保留一个占位项让选择器如实回显,提示重选 -->
                  <option value={bindings[a.id]} disabled>{$_('agents.provider.stale')}</option>
                {/if}
              </select>
              {#if bindApplying[a.id]}<span class="spinner small"></span>{/if}
            </div>
            {#if bindErrors[a.id]}
              <pre class="install-error">{bindErrors[a.id]}</pre>
            {/if}
          {/if}
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
            {#if showSyncDetailBtn(a.id)}
              <button
                class="btn"
                class:active={syncDetailId === a.id}
                onclick={() => toggleSyncDetail(a.id)}
              >{$_('agents.syncDetail.button')}</button>
            {/if}
            {#if a.docs_url}
              <a class="docs-link" href={a.docs_url} target="_blank" rel="noreferrer">{$_('agents.docs')}</a>
            {/if}
          </div>
          {#if errors[a.id]}
            <pre class="install-error">{errors[a.id]}</pre>
          {/if}
        </div>

        <!-- 同步详情:插在展开卡片所在行的行尾之后,整行占满,同行卡片位置不动 -->
        {#if i === syncRowEnd}
          <div class="glass-card sync-detail">
            <div class="sync-detail-title">
              {agents.find((x) => x.id === syncDetailId)?.label} · {$_('agents.syncDetail.button')}
            </div>
            {#if overviewLoading}
              <div class="sync-loading"><span class="spinner small"></span> {$_('agents.syncDetail.loading')}</div>
            {:else if overviewError}
              <pre class="install-error">{overviewError}</pre>
              <div class="sync-detail-foot">
                <button class="btn" onclick={() => loadOverview(true)}>{$_('agents.syncDetail.refresh')}</button>
              </div>
            {:else}
              {@const o = syncDetailId !== null ? overview?.[syncDetailId] : undefined}
              {#if !o}
                <span class="sync-muted">{$_('agents.syncDetail.empty')}</span>
              {:else if o.providers.length === 0 && o.mcp.length === 0 && o.skills.length === 0 && o.memory.length === 0 && !o.provider_error && !o.mcp_error && !o.skills_error && !o.memory_error}
                <p class="sync-empty">{$_('agents.syncDetail.empty')}</p>
                <p class="sync-muted">{$_('agents.syncDetail.emptyHint')}</p>
                <div class="sync-detail-foot">
                  <button class="btn" onclick={() => loadOverview(true)}>{$_('agents.syncDetail.refresh')}</button>
                </div>
              {:else}
                {@render syncSection($_('agents.syncDetail.providers'), o.provider_supported, o.providers, o.provider_config_path, o.provider_error, false)}
                {@render syncSection($_('agents.syncDetail.mcp'), o.mcp_supported, o.mcp, o.mcp_config_path, o.mcp_error, true)}
                {@render syncSection($_('agents.syncDetail.skills'), o.skills_supported, o.skills, o.skills_config_path, o.skills_error, false)}
                {@render syncSection($_('agents.syncDetail.memory'), o.memory_supported, o.memory, o.memory_config_path, o.memory_error, false)}
                <div class="sync-detail-foot">
                  <button class="btn" onclick={() => loadOverview(true)}>{$_('agents.syncDetail.refresh')}</button>
                </div>
              {/if}
            {/if}
          </div>
        {/if}
      {/each}
    </div>
  {/if}
</div>

<!-- 同步详情的一个小节(服务商 / MCP 同构;MCP 的 CLI 型 path 为空串 → 「经 CLI 管理」) -->
{#snippet syncSection(
  title: string,
  supported: boolean,
  items: SyncedItem[],
  configPath: string,
  err: string | null,
  cliWhenNoPath: boolean
)}
  <div class="sync-section">
    <span class="sync-section-title">{title}</span>
    {#if !supported}
      <span class="sync-muted">{$_('agents.syncDetail.unsupported')}</span>
    {:else}
      {#if err}
        <pre class="install-error">{err}</pre>
      {/if}
      {#if items.length > 0}
        <div class="sync-chips">
          {#each items as item (item.name)}
            <span class="sync-chip state-{item.state}">
              {itemLabel(item.name)}
              <span class="chip-state">{$_(`agents.syncDetail.state.${item.state}`)}</span>
            </span>
          {/each}
        </div>
      {:else if !err}
        <span class="sync-muted">{$_('agents.syncDetail.none')}</span>
      {/if}
      {#if configPath}
        <code class="sync-path" title={configPath}>{configPath}</code>
      {:else if cliWhenNoPath}
        <span class="sync-muted">{$_('agents.syncDetail.viaCli')}</span>
      {/if}
    {/if}
  </div>
{/snippet}

<style>
  .agents-page { padding: 1.5rem; display: flex; flex-direction: column; gap: 1rem; }
  .page-header { display: flex; align-items: baseline; gap: 1rem; }
  .page-header h1 { margin: 0; font-size: 1.25rem; }
  .subtitle { opacity: 0.6; flex: 1; font-size: 0.85rem; }
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
  .agent-label { font-weight: 600; font-size: 0.9rem; }
  .kind-badge { font-size: 0.7rem; padding: 0.1rem 0.5rem; border-radius: 999px; background: rgba(94, 234, 212, 0.15); color: var(--accent-teal); white-space: nowrap; }
  .version { font-family: monospace; font-size: 0.75rem; opacity: 0.7; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; transition: color 0.3s; }
  .version.upgraded { color: #4ade80; opacity: 1; }
  .update-badge { color: #fbbf24; margin-left: 0.3rem; }
  .not-installed { font-size: 0.8rem; color: #fbbf24; }
  .card-body { min-height: 1.2rem; }
  .install-cmd { font-size: 0.72rem; opacity: 0.55; display: block; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .card-actions { display: flex; align-items: center; gap: 0.75rem; flex-wrap: wrap; margin-top: auto; }
  .deps-hint { font-size: 0.75rem; color: #fbbf24; }
  .detect-only { font-size: 0.8rem; opacity: 0.5; }
  .docs-link { font-size: 0.8rem; color: var(--accent-teal); margin-left: auto; }
  .install-error { font-size: 0.75rem; color: #f87171; white-space: pre-wrap; margin: 0; }
  .btn { padding: 0.3rem 0.9rem; border-radius: 6px; border: 1px solid var(--border-strong); background: transparent; color: inherit; cursor: pointer; font-size: 0.75rem; }
  /* 页头按钮:无样式定义时会继承根字号显得过大,与 .btn 同基准 */
  .refresh-btn {
    padding: 0.3rem 0.9rem; border-radius: 6px; border: 1px solid var(--border-strong);
    background: transparent; color: inherit; cursor: pointer; font-size: 0.75rem;
    display: inline-flex; align-items: center; gap: 0.4rem;
  }
  .refresh-btn:disabled { opacity: 0.5; cursor: default; }
  .btn.primary { background: rgba(94, 234, 212, 0.15); border-color: var(--accent-teal); color: var(--accent-teal); }
  .btn.danger { background: rgba(248, 113, 113, 0.15); border-color: #f87171; color: #f87171; }
  .btn.subtle { opacity: 0.55; }
  .btn.active { border-color: var(--accent-teal); color: var(--accent-teal); }
  .btn:disabled { opacity: 0.5; cursor: default; }

  /* 同步详情:独占一整行的展开面板,插在展开卡片所在行之后 */
  .sync-detail {
    grid-column: 1 / -1;
    padding: 0.9rem 1.1rem; display: flex; flex-direction: column; gap: 0.6rem;
  }
  .sync-detail-title { font-size: 0.85rem; font-weight: 600; }
  .sync-loading { display: flex; align-items: center; gap: 0.5rem; font-size: 0.8rem; opacity: 0.7; }
  .sync-section { display: flex; flex-direction: column; gap: 0.35rem; }
  .sync-section-title { font-size: 0.78rem; font-weight: 600; color: var(--accent-teal); }
  .sync-chips { display: flex; flex-wrap: wrap; gap: 0.35rem; }
  .sync-chip {
    display: inline-flex; align-items: center; gap: 0.35rem;
    font-size: 0.72rem; font-family: monospace; padding: 0.15rem 0.55rem;
    border-radius: 999px; border: 1px solid var(--border-subtle);
  }
  .chip-state { font-size: 0.62rem; font-family: inherit; }
  .sync-chip.state-synced { border-color: rgba(74,222,128,0.4); }
  .sync-chip.state-synced .chip-state { color: #4ade80; }
  .sync-chip.state-unsynced { border-color: rgba(251,191,36,0.4); }
  .sync-chip.state-unsynced .chip-state { color: #fbbf24; }
  .sync-chip.state-outdated { border-color: rgba(251,146,60,0.45); }
  .sync-chip.state-outdated .chip-state { color: #fb923c; }
  .sync-chip.state-removing { border-color: rgba(248,113,113,0.35); opacity: 0.75; }
  .sync-chip.state-removing .chip-state { color: #f87171; }
  .sync-path { font-size: 0.68rem; opacity: 0.55; display: block; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .sync-muted { font-size: 0.75rem; opacity: 0.5; margin: 0; }
  .sync-empty { margin: 0; font-size: 0.8rem; opacity: 0.75; }
  .sync-detail-foot { display: flex; justify-content: flex-end; }
  .provider-bind { display: flex; align-items: center; gap: 0.5rem; }
  .bind-label { font-size: 0.75rem; opacity: 0.6; white-space: nowrap; }
  .bind-select {
    /* 用 background-color 而非 background 简写:保留全局 select 的 SVG 下拉箭头 */
    flex: 1; min-width: 0; width: auto;
    padding: 0.3rem 1.9rem 0.3rem 0.6rem; border-radius: 6px;
    border: 1px solid var(--border-strong); background-color: var(--bg-tertiary);
    color: inherit; font-size: 0.75rem; cursor: pointer;
    transition: border-color 0.15s ease;
  }
  .bind-select:hover:not(:disabled) { border-color: var(--neon-cyan); }
  .bind-select.empty { color: var(--text-muted); }
  .bind-select:disabled { opacity: 0.5; cursor: default; }
  .provider-bind.flash .bind-select { border-color: #4ade80; transition: border-color 0.3s; }
  .loading { padding: 2rem; display: flex; justify-content: center; gap: 0.5rem; }
  .spinner { width: 16px; height: 16px; border: 2px solid rgba(94,234,212,0.3); border-top-color: var(--accent-teal); border-radius: 50%; animation: spin 0.8s linear infinite; display: inline-block; }
  .spinner.small { width: 12px; height: 12px; }
  @keyframes spin { to { transform: rotate(360deg); } }
</style>
