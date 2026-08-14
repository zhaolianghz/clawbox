<script lang="ts">
  import { onMount } from 'svelte';
  import { _ } from 'svelte-i18n';
  import { agents_list, agent_install, type AgentStatus, path_env_status } from '../../lib/api/agents';
  import { checkLatestVersions, extractSemver, type LatestInfo } from '../../lib/api/latest';
  import { agent_sync_overview, type AgentSyncOverview, type SyncedItem } from '../../lib/api/providerSync';
  import { snapshots_list, snapshots_restore, type SnapshotInfo } from '../../lib/api/snapshots';
  import AgentLogo from '../../lib/components/AgentLogo.svelte';
  import { providers, loadProviders } from '../../lib/stores/config';
  import {
    agent_provider_bind,
    agent_providers_get,
    agent_fallbacks_get,
    agent_fallbacks_set,
    agent_provider_resync,
    agent_provider_adopt,
    agent_active_providers_get,
    type ActiveProviderInfo
  } from '../../lib/api/providerSync';
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
  // PATH 解析是否降级(shell 超时→备用目录)。降级时顶部提示:已装 agent 可能误报未安装(GH#3)。
  let pathDegraded = $state(false);

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
    // 漂移检测随页面加载即跑:顶部「全部恢复」条与卡片横幅依赖 overview,
    // 必须在这里启动,否则漂移要等用户点开「同步详情」才可见(违背傻瓜式设计)。
    void loadOverview();
    agent_providers_get()
      .then((b) => (bindings = b))
      .catch((e) => console.warn('agent_providers_get failed', e));
    agent_fallbacks_get()
      .then((f) => (fallbacks = f))
      .catch((e) => console.warn('agent_fallbacks_get failed', e));
    path_env_status()
      .then((s) => (pathDegraded = s === 'shell_failed'))
      .catch((e) => console.warn('path_env_status failed', e));
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

  // ---------- fallback 链(仅原生支持的 agent;目前仅 hermes)----------
  let fallbacks = $state<Record<string, string[]>>({});
  let fbApplying = $state<Record<string, boolean>>({});
  let fbErrors = $state<Record<string, string>>({});
  let fbFlash = $state<Record<string, boolean>>({});
  // 与后端 ProviderAdapter::supports_fallback() 对应;仅这些 agent 渲染 fallback UI。
  const SUPPORTS_FALLBACK = new Set(['hermes']);

  async function loadFallbacks() {
    try {
      fallbacks = await agent_fallbacks_get();
    } catch (e) {
      console.warn('agent_fallbacks_get failed', e);
    }
  }

  /** 把某 provider 追加到该 agent 的 fallback 链尾(去重;不与 primary 重复) */
  async function addFallback(agentId: string, providerId: string) {
    const primary = bindings[agentId];
    const cur = fallbacks[agentId] ?? [];
    if (providerId === primary || cur.includes(providerId)) return;
    await setFallbacks(agentId, [...cur, providerId]);
  }

  /** 从 fallback 链里移除某 provider */
  async function removeFallback(agentId: string, providerId: string) {
    const cur = fallbacks[agentId] ?? [];
    await setFallbacks(agentId, cur.filter((id) => id !== providerId));
  }

  // ---- 拖拽排序(原生 HTML5 DnD;无额外依赖)----
  // {agentId, index} 或 null。跨 agent 的 drop 一律忽略(只同卡内可排)。
  let dragFb = $state<{ agent: string; from: number } | null>(null);
  let dropIdx = $state<{ agent: string; to: number } | null>(null);

  function onFbDragStart(agentId: string, i: number, e: DragEvent) {
    dragFb = { agent: agentId, from: i };
    if (e.dataTransfer) {
      e.dataTransfer.effectAllowed = 'move';
      // Firefox 需设 data 才触发 drag
      e.dataTransfer.setData('text/plain', String(i));
    }
  }
  function onFbDragOver(agentId: string, i: number, e: DragEvent) {
    if (dragFb?.agent !== agentId) return;
    e.preventDefault(); // 允许 drop
    if (e.dataTransfer) e.dataTransfer.dropEffect = 'move';
    if (dropIdx?.agent !== agentId || dropIdx?.to !== i) dropIdx = { agent: agentId, to: i };
  }
  async function onFbDrop(agentId: string, i: number, e: DragEvent) {
    e.preventDefault();
    const from = dragFb;
    dragFb = null;
    dropIdx = null;
    if (!from || from.agent !== agentId || from.from === i) return;
    const cur = [...(fallbacks[agentId] ?? [])];
    if (from.from < 0 || from.from >= cur.length || i < 0 || i >= cur.length) return;
    const [moved] = cur.splice(from.from, 1);
    cur.splice(i, 0, moved);
    await setFallbacks(agentId, cur);
  }
  function onFbDragEnd() {
    dragFb = null;
    dropIdx = null;
  }

  async function setFallbacks(agentId: string, ids: string[]) {
    fbApplying = { ...fbApplying, [agentId]: true };
    fbErrors = { ...fbErrors, [agentId]: '' };
    try {
      await agent_fallbacks_set(agentId, ids);
      fallbacks = { ...fallbacks, [agentId]: ids };
      fbFlash = { ...fbFlash, [agentId]: true };
      setTimeout(() => (fbFlash = { ...fbFlash, [agentId]: false }), 2000);
      if (overview !== null) void loadOverview(true);
    } catch (e) {
      fbErrors = { ...fbErrors, [agentId]: String(e) };
    } finally {
      fbApplying = { ...fbApplying, [agentId]: false };
    }
  }

  // ---------- 手动重推(愈合「已过期」漂移;reconcile 默认不自动覆盖)----------
  let resyncing = $state<Record<string, boolean>>({});
  async function resyncProvider(agentId: string) {
    resyncing = { ...resyncing, [agentId]: true };
    try {
      const r = await agent_provider_resync(agentId);
      if (!r.ok && r.error) bindErrors = { ...bindErrors, [agentId]: r.error };
      if (overview !== null) void loadOverview(true);
    } catch (e) {
      bindErrors = { ...bindErrors, [agentId]: String(e) };
    } finally {
      resyncing = { ...resyncing, [agentId]: false };
    }
  }

  // ---------- adopt:agent → ClawBox 领养 ----------
  let adopting = $state<Record<string, boolean>>({});
  async function adoptFromAgent(agentId: string) {
    adopting = { ...adopting, [agentId]: true };
    bindErrors = { ...bindErrors, [agentId]: '' };
    try {
      const r = await agent_provider_adopt(agentId);
      // ClawBox 服务商列表变了(可能新建了一条)→ 重新拉取 store + 绑定
      await loadProviders();
      bindings = await agent_providers_get().catch(() => bindings);
      fallbacks = await agent_fallbacks_get().catch(() => fallbacks);
      bindFlash = { ...bindFlash, [agentId]: true };
      setTimeout(() => (bindFlash = { ...bindFlash, [agentId]: false }), 2000);
      if (overview !== null) void loadOverview(true);
      // 领养结果在 sync detail 的 provider_error 里不显示;用 bindErrors 反向提示成功
      // (空串=无错误;flash 高亮已表达成功)。失败走 catch。
      void r;
    } catch (e) {
      bindErrors = { ...bindErrors, [agentId]: String(e) };
    } finally {
      adopting = { ...adopting, [agentId]: false };
    }
  }

  // ---------- 漂移横幅(三态 resolve 的傻瓜式皮)----------
  // 每个 agent 当前在用的服务商(只名字+模型;由 loadOverview 随漂移列表一起拉)。
  let activeProviders = $state<Record<string, ActiveProviderInfo | null>>({});
  let batchRestoring = $state(false);

  /** 某 agent 是否漂移(provider 维度 outdated/removing)。overview 未加载时返回 false。 */
  function isDrifted(agentId: string): boolean {
    if (overview === null) return false;
    const o = overview[agentId];
    return !!o && o.provider_supported && o.providers.some((p) => p.state === 'outdated' || p.state === 'removing');
  }

  /** 所有漂移 agent 的 id(顶部汇总条用)。 */
  function driftedAgentIds(): string[] {
    if (overview === null) return [];
    return Object.values(overview)
      .filter((o) => o.provider_supported && o.providers.some((p) => p.state === 'outdated' || p.state === 'removing'))
      .map((o) => o.agent_id);
  }

  function agentLabel(id: string): string {
    return agents.find((a) => a.id === id)?.label ?? id;
  }

  /** 顶部「全部恢复」:对所有漂移 agent 逐个 resync(ClawBox 赢),批量只做恢复。 */
  async function restoreAll() {
    const ids = driftedAgentIds();
    if (ids.length === 0) return;
    batchRestoring = true;
    try {
      for (const id of ids) {
        await agent_provider_resync(id);
      }
      if (overview !== null) await loadOverview(true);
    } catch (e) {
      overviewError = String(e);
    } finally {
      batchRestoring = false;
    }
  }

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
      // primary 变了:后端会自动把新 primary 从 fallback 链里剔出,刷新本地状态
      void loadFallbacks();
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
      // 为漂移横幅拉取各 agent 现在在用的服务商名(只对漂移的 agent 取,省请求)
      const drifted = list
        .filter((o) => o.provider_supported && o.providers.some((p) => p.state === 'outdated' || p.state === 'removing'))
        .map((o) => o.agent_id);
      activeProviders =
        drifted.length > 0
          ? await agent_active_providers_get(drifted).catch(() => ({} as Record<string, ActiveProviderInfo | null>))
          : {};
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
    snapDetailId = null; // 与快照面板互斥(同一行尾插槽)
    syncDetailId = id;
    void loadOverview();
  }

  // ---------- 快照历史内联展开(与同步详情同款:面板插在所点卡片所在行的行尾) ----------
  let snapDetailId = $state<string | null>(null);
  let snaps = $state<Record<string, SnapshotInfo[]>>({}); // agent_id -> 列表(倒序)
  let snapsLoading = $state(false);
  let snapsError = $state('');
  let restoreConfirmId = $state<string | null>(null); // 待确认恢复的快照 id
  let restoringId = $state<string | null>(null);
  let restoreMsg = $state(''); // 最近一次恢复结果(成功摘要/失败原因)

  const snapRowEnd = $derived.by(() => {
    if (snapDetailId === null) return -1;
    const idx = agents.findIndex((a) => a.id === snapDetailId);
    if (idx < 0) return -1;
    return Math.min(Math.floor(idx / gridCols) * gridCols + gridCols - 1, agents.length - 1);
  });

  async function loadSnaps(force = false) {
    if (snapsLoading) return;
    if (snapDetailId !== null && !force && snaps[snapDetailId]) return;
    snapsLoading = true;
    snapsError = '';
    try {
      const list = await snapshots_list();
      snaps = {};
      for (const s of list) {
        (snaps[s.agent_id] ??= []).push(s);
      }
    } catch (e) {
      snapsError = String(e);
    } finally {
      snapsLoading = false;
    }
  }

  function toggleSnapDetail(id: string) {
    restoreMsg = '';
    restoreConfirmId = null;
    if (snapDetailId === id) {
      snapDetailId = null;
      return;
    }
    syncDetailId = null; // 与同步详情互斥
    snapDetailId = id;
    void loadSnaps();
  }

  async function doRestore(agentId: string, snapId: string) {
    if (restoringId) return;
    restoringId = snapId;
    restoreMsg = '';
    try {
      const r = await snapshots_restore(agentId, snapId);
      restoreMsg = $_('agents.snapshots.restored', { values: { n: r.restored.length } })
        + (r.cleared.length > 0 ? ' · ' + $_('agents.snapshots.clearedNote') : '');
      restoreConfirmId = null;
      await loadSnaps(true); // 恢复会产生 pre-restore 安全快照,刷新列表
    } catch (e) {
      restoreMsg = String(e);
    } finally {
      restoringId = null;
    }
  }

  /** scope → 徽章文案(复用 syncDetail 的维度名) */
  const SNAP_SCOPE_KEYS: Record<string, string> = {
    provider: 'agents.syncDetail.providers',
    fallback: 'agents.fallback.label',
    mcp: 'agents.syncDetail.mcp',
    skills: 'agents.syncDetail.skills',
    memory: 'agents.syncDetail.memory',
  };

  function fmtTime(iso: string): string {
    const d = new Date(iso);
    return isNaN(d.getTime()) ? iso : d.toLocaleString();
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
    {#if pathDegraded}
      <div class="path-warn glass-card">
        <span class="path-warn-icon" aria-hidden="true">⚠</span>
        <span class="path-warn-text">
          <strong>{$_('agents.pathWarning.title')}</strong>
          {$_('agents.pathWarning.text')}
        </span>
      </div>
    {/if}
    {#if driftedAgentIds().length > 0}
      <div class="drift-bar glass-card">
        <span class="drift-bar-icon" aria-hidden="true">⚠</span>
        <span class="drift-bar-text">
          {$_('agents.drift.summary', {
            values: {
              count: driftedAgentIds().length,
              names: driftedAgentIds().map(agentLabel).join(' · ')
            }
          })}
        </span>
        <button class="btn primary" onclick={restoreAll} disabled={batchRestoring}>
          {#if batchRestoring}<span class="spinner small"></span>{/if}
          {$_('agents.drift.restoreAll')}
        </button>
      </div>
    {/if}
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
          {#if isDrifted(a.id)}
            {@const boundSpec = bindings[a.id] ? $providers.find((p) => p.id === bindings[a.id]) : null}
            {@const active = activeProviders[a.id] ?? null}
            {@const clawboxName = boundSpec?.name ?? ''}
            <div class="drift-banner">
              <span class="drift-icon" aria-hidden="true">⚠</span>
              <span class="drift-text">
                {#if active}
                  {$_('agents.drift.card', { values: { agentNow: active.name, clawbox: clawboxName } })}
                {:else}
                  {$_('agents.drift.cardUnknown', { values: { clawbox: clawboxName } })}
                {/if}
              </span>
              <div class="drift-actions">
                <button
                  class="btn mini primary"
                  onclick={() => resyncProvider(a.id)}
                  disabled={resyncing[a.id]}
                >
                  {#if resyncing[a.id]}<span class="spinner small"></span>{/if}
                  {$_('agents.drift.restore', { values: { name: clawboxName } })}
                </button>
                {#if active}
                  <button
                    class="btn mini"
                    onclick={() => adoptFromAgent(a.id)}
                    disabled={adopting[a.id]}
                  >
                    {$_('agents.drift.keep', { values: { name: active.name } })}
                  </button>
                {/if}
              </div>
            </div>
          {/if}
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
            {#if SUPPORTS_FALLBACK.has(a.id)}
              <div class="provider-bind fallback" class:flash={fbFlash[a.id]}>
                <span class="bind-label">{$_('agents.fallback.label')}</span>
                <div class="fallback-chain">
                  {#each fallbacks[a.id] ?? [] as fid, i (fid)}
                    {@const p = enabledProviders.find((x) => x.id === fid) ?? $providers.find((x) => x.id === fid)}
                    <span
                      class="chip chip-on"
                      class:dragging={dragFb?.agent === a.id && dragFb?.from === i}
                      class:drop-target={dropIdx?.agent === a.id && dropIdx?.to === i && dragFb?.from !== i}
                      draggable={!fbApplying[a.id]}
                      title={$_('agents.fallback.dragHint')}
                      ondragstart={(e) => onFbDragStart(a.id, i, e)}
                      ondragover={(e) => onFbDragOver(a.id, i, e)}
                      ondrop={(e) => onFbDrop(a.id, i, e)}
                      ondragend={onFbDragEnd}
                      role="button"
                      tabindex="0"
                    >
                      <span class="chip-grip" aria-hidden="true">⋮⋮</span>
                      <span class="chip-idx">{i + 1}</span>
                      {p?.name ?? fid}
                      <button
                        class="chip-x"
                        title={$_('agents.fallback.remove')}
                        aria-label={$_('agents.fallback.remove')}
                        disabled={fbApplying[a.id]}
                        onclick={() => removeFallback(a.id, fid)}>×</button>
                    </span>
                  {/each}
                  {#each enabledProviders.filter((p) => p.id !== bindings[a.id] && !(fallbacks[a.id] ?? []).includes(p.id) && compatible(a.id, p) && p.defaultModel) as p (p.id)}
                    <button
                      class="chip chip-add"
                      title={$_('agents.fallback.add')}
                      disabled={fbApplying[a.id]}
                      onclick={() => addFallback(a.id, p.id)}
                    >+ {p.name}</button>
                  {/each}
                </div>
                {#if !(fallbacks[a.id]?.length) && !enabledProviders.some((p) => p.id !== bindings[a.id] && compatible(a.id, p) && p.defaultModel)}
                  <span class="fallback-empty">{$_('agents.fallback.empty')}</span>
                {/if}
                {#if fbApplying[a.id]}<span class="spinner small"></span>{/if}
              </div>
              {#if fbErrors[a.id]}
                <pre class="install-error">{fbErrors[a.id]}</pre>
              {/if}
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
            <button
              class="btn"
              class:active={snapDetailId === a.id}
              onclick={() => toggleSnapDetail(a.id)}
            >{$_('agents.snapshots.button')}</button>
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
                {@render syncSection($_('agents.syncDetail.providers'), o.provider_supported, o.providers, o.provider_config_path, o.provider_error, false, $_('agents.syncDetail.resync'), () => resyncProvider(o.agent_id), resyncing[o.agent_id], $_('agents.syncDetail.adopt'), () => adoptFromAgent(o.agent_id), adopting[o.agent_id])}}
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

        <!-- 快照历史:与同步详情同款插槽;两者互斥,不会同时展开 -->
        {#if i === snapRowEnd}
          <div class="glass-card sync-detail">
            <div class="sync-detail-title">
              {agents.find((x) => x.id === snapDetailId)?.label} · {$_('agents.snapshots.button')}
            </div>
            {#if snapsLoading}
              <div class="sync-loading"><span class="spinner small"></span> {$_('agents.snapshots.loading')}</div>
            {:else if snapsError}
              <pre class="install-error">{snapsError}</pre>
              <div class="sync-detail-foot">
                <button class="btn" onclick={() => loadSnaps(true)}>{$_('agents.snapshots.refresh')}</button>
              </div>
            {:else}
              {@const list = snapDetailId !== null ? (snaps[snapDetailId] ?? []) : []}
              {#if list.length === 0}
                <p class="sync-empty">{$_('agents.snapshots.empty')}</p>
                <p class="sync-muted">{$_('agents.snapshots.emptyHint')}</p>
                <div class="sync-detail-foot">
                  <button class="btn" onclick={() => loadSnaps(true)}>{$_('agents.snapshots.refresh')}</button>
                </div>
              {:else}
                {#if restoreMsg}
                  <p class="snap-msg">{restoreMsg}</p>
                {/if}
                <ul class="snap-list">
                  {#each list as s (s.id)}
                    <li class="snap-item">
                      <span class="chip snap-scope" data-scope={s.scope}>
                        {$_(SNAP_SCOPE_KEYS[s.scope] ?? 'agents.snapshots.button')}
                      </span>
                      <code class="snap-time" title={s.id}>{fmtTime(s.created_at)}</code>
                      <span class="snap-files">{$_('agents.snapshots.files', { values: { n: s.files } })}</span>
                      {#if !s.restorable}
                        <span class="snap-unrestorable" title={$_('agents.snapshots.unrestorableHint')}>
                          {$_('agents.snapshots.unrestorable')}
                        </span>
                      {/if}
                      <span class="snap-actions">
                        {#if restoreConfirmId === s.id}
                          <span class="snap-confirm-text">{$_('agents.snapshots.restoreConfirm')}</span>
                          <button class="btn danger" disabled={restoringId !== null} onclick={() => snapDetailId && doRestore(snapDetailId, s.id)}>
                            {#if restoringId === s.id}<span class="spinner small"></span>{:else}{$_('agents.snapshots.restoreConfirmBtn')}{/if}
                          </button>
                          <button class="btn" disabled={restoringId !== null} onclick={() => (restoreConfirmId = null)}>
                            {$_('agents.cancel')}
                          </button>
                        {:else if s.restorable}
                          <button class="btn" disabled={restoringId !== null} onclick={() => (restoreConfirmId = s.id)}>
                            {$_('agents.snapshots.restore')}
                          </button>
                        {/if}
                      </span>
                    </li>
                  {/each}
                </ul>
                <p class="snap-note">{$_('agents.snapshots.note')}</p>
                <div class="sync-detail-foot">
                  <button class="btn" onclick={() => loadSnaps(true)}>{$_('agents.snapshots.refresh')}</button>
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
  cliWhenNoPath: boolean,
  resyncLabel = '',
  onResync: (() => void) | undefined = undefined,
  resyncing = false,
  adoptLabel = '',
  onAdopt: (() => void) | undefined = undefined,
  adopting = false
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
      {#if onResync && items.some((i) => i.state === 'outdated' || i.state === 'removing')}
        <button class="btn mini" onclick={onResync} disabled={resyncing}>
          {#if resyncing}<span class="spinner small"></span>{/if}{resyncLabel}
        </button>
      {/if}
      {#if onAdopt}
        <button class="btn mini" onclick={onAdopt} disabled={adopting} title={$_('agents.syncDetail.adoptHint')}>
          {#if adopting}<span class="spinner small"></span>{/if}{adoptLabel}
        </button>
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
  .btn.mini { padding: 0.2rem 0.6rem; font-size: 0.7rem; }
  /* 漂移横幅(三态 resolve 的傻瓜式皮)*/
  .drift-bar {
    display: flex; align-items: center; gap: 0.7rem; padding: 0.6rem 0.9rem;
    border: 1px solid rgba(251,146,60,0.4); border-radius: 8px;
    background: color-mix(in srgb, #fb923c 8%, var(--bg-secondary));
  }
  .drift-bar-icon { font-size: 1rem; }
  .drift-bar-text { flex: 1; font-size: 0.8rem; }
  /* PATH 解析降级提示(GH#3):安装了但检测不到时给出原因与解法 */
  .path-warn {
    display: flex; align-items: flex-start; gap: 0.6rem; padding: 0.6rem 0.9rem;
    border: 1px solid rgba(250,204,21,0.4); border-radius: 8px;
    background: color-mix(in srgb, #facc15 8%, var(--bg-secondary));
  }
  .path-warn-icon { font-size: 1rem; line-height: 1.4; }
  .path-warn-text { flex: 1; font-size: 0.8rem; line-height: 1.45; }
  .path-warn-text strong { display: block; margin-bottom: 0.15rem; }
  .drift-banner {
    display: flex; align-items: center; gap: 0.5rem; flex-wrap: wrap;
    padding: 0.5rem 0.7rem; border-radius: 7px; margin-bottom: 0.2rem;
    border: 1px solid rgba(251,146,60,0.4);
    background: color-mix(in srgb, #fb923c 8%, transparent);
  }
  .drift-banner .drift-icon { opacity: 0.9; }
  .drift-banner .drift-text { flex: 1; min-width: 0; font-size: 0.75rem; }
  .drift-actions { display: flex; gap: 0.4rem; flex-shrink: 0; }
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
  .snap-list { list-style: none; margin: 0.5rem 0; padding: 0; display: flex; flex-direction: column; gap: 0.35rem; }
  .snap-item { display: flex; align-items: center; gap: 0.6rem; font-size: 0.78rem; flex-wrap: wrap; }
  .snap-scope { min-width: 3.5em; justify-content: center; }
  .snap-scope[data-scope='provider'], .snap-scope[data-scope='fallback'] { border-color: var(--neon-cyan); }
  .snap-time { font-size: 0.75rem; opacity: 0.85; }
  .snap-files { font-size: 0.72rem; opacity: 0.6; font-variant-numeric: tabular-nums; }
  .snap-unrestorable { font-size: 0.7rem; color: var(--danger, #f87171); border: 1px dashed currentColor;
    padding: 0.1rem 0.4rem; border-radius: 999px; cursor: help; }
  .snap-actions { margin-left: auto; display: inline-flex; gap: 0.4rem; align-items: center; }
  .snap-confirm-text { font-size: 0.72rem; color: var(--danger, #f87171); }
  .snap-msg { font-size: 0.78rem; color: var(--accent-teal); margin: 0.25rem 0; }
  .snap-note { font-size: 0.7rem; opacity: 0.5; margin: 0.35rem 0 0; }
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
  /* fallback 链 */
  .provider-bind.fallback { align-items: flex-start; flex-wrap: wrap; }
  .provider-bind.fallback.flash { outline: 1px solid #4ade80; outline-offset: 2px; border-radius: 6px; transition: outline-color 0.3s; }
  .fallback-chain { display: flex; flex-wrap: wrap; gap: 0.35rem; align-items: center; flex: 1; min-width: 0; }
  .chip { display: inline-flex; align-items: center; gap: 0.25rem; font-size: 0.72rem;
    padding: 0.2rem 0.5rem; border-radius: 999px; border: 1px solid var(--border-strong);
    background-color: var(--bg-tertiary); line-height: 1.2; }
  .chip-on { border-color: var(--neon-cyan); cursor: grab; }
  .chip-on:active { cursor: grabbing; }
  .chip-on.dragging { opacity: 0.4; }
  .chip-on.drop-target { border-color: var(--accent, #f59e0b); box-shadow: 0 0 0 2px color-mix(in srgb, var(--neon-cyan, #5eead4) 25%, transparent); }
  .chip-grip { font-size: 0.7rem; opacity: 0.45; letter-spacing: -2px; cursor: inherit; user-select: none; }
  .chip-idx { font-size: 0.65rem; opacity: 0.7; font-variant-numeric: tabular-nums; }
  .chip-x { background: none; border: none; color: var(--text-muted); cursor: pointer;
    padding: 0 0.1rem; font-size: 0.85rem; line-height: 1; }
  .chip-x:hover:not(:disabled) { color: var(--danger, #f87171); }
  .chip-x:disabled { opacity: 0.4; cursor: default; }
  .chip-add { cursor: pointer; border-style: dashed; color: var(--text-muted); }
  .chip-add:hover:not(:disabled) { border-color: var(--neon-cyan); color: inherit; }
  .chip-add:disabled { opacity: 0.4; cursor: default; }
  .fallback-empty { font-size: 0.72rem; opacity: 0.5; }
  .loading { padding: 2rem; display: flex; justify-content: center; gap: 0.5rem; }
  .spinner { width: 16px; height: 16px; border: 2px solid rgba(94,234,212,0.3); border-top-color: var(--accent-teal); border-radius: 50%; animation: spin 0.8s linear infinite; display: inline-block; }
  .spinner.small { width: 12px; height: 12px; }
  @keyframes spin { to { transform: rotate(360deg); } }
</style>
