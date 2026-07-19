<script lang="ts">
  import { onMount } from 'svelte';
  import { _ } from 'svelte-i18n';
  import {
    PROVIDER_CATALOG, PROVIDER_CATEGORIES,
    type ProviderCatalogEntry, type ProviderCategory,
  } from '$lib/data/providers';
  import ProviderLogo from '$lib/components/ProviderLogo.svelte';
  import AgentLogo from '$lib/components/AgentLogo.svelte';
  import { providers, addProvider, updateProvider, deleteProvider, loadProviders } from '$lib/stores/config';
  import { provider_test, type ModelProvider, type ProviderFlavor, type ProviderTestResult } from '$lib/api/config';
  import {
    sync_providers_plan, sync_providers_apply, sync_providers_status,
    config_active_provider_get, config_active_provider_set,
    type AgentPlan, type ApplyResult, type ChangeItem, type ProviderSyncStatus,
  } from '$lib/api/providerSync';
  import { agents_list } from '$lib/api/agents';

  let query = $state('');
  let activeCategory = $state<ProviderCategory | 'all'>('all');
  let pageError = $state('');

  // ---------- 激活(默认)服务商 ----------
  let activeProviderId = $state<string | null>(null);

  async function refreshActive() {
    try {
      activeProviderId = await config_active_provider_get();
    } catch (e) {
      pageError = String(e);
    }
  }

  /** 点击星标:未激活 → 设为默认;已激活 → 取消(传 null) */
  async function toggleActive(p: ModelProvider) {
    pageError = '';
    const prev = activeProviderId;
    const next = activeProviderId === p.id ? null : p.id;
    activeProviderId = next; // 乐观更新
    try {
      await config_active_provider_set(next);
      void refreshSyncStatus(); // 设默认影响 claude-code/codex 的待同步判定
    } catch (e) {
      activeProviderId = prev; // 回滚
      pageError = String(e);
    }
  }

  /** 激活服务商缺失或被禁用:同步时 claude-code / codex 会跳过 */
  const activeMissing = $derived(
    !activeProviderId || !$providers.some((p) => p.id === activeProviderId && p.enabled)
  );

  // ---------- 卡片同步状态标签 ----------
  let cardStatus = $state<Record<string, ProviderSyncStatus>>({}); // provider_id → 状态
  let cardStatusLoaded = $state(false); // 拉取失败时静默降级:不显示标签

  /** 轻量拉取各服务商同步状态;命令内部读各 agent 配置文件,失败只 warn 不打扰页面 */
  async function refreshSyncStatus() {
    try {
      const list = await sync_providers_status();
      cardStatus = Object.fromEntries(list.map((s) => [s.provider_id, s]));
      cardStatusLoaded = true;
    } catch (e) {
      console.warn('sync_providers_status failed:', e);
      cardStatusLoaded = false;
    }
  }

  // 已配置的服务商:任一端点槽的 host 命中目录条目即视为匹配
  const configuredByHost = $derived.by(() => {
    const map = new Map<string, ModelProvider>();
    for (const p of $providers) {
      if (p.anthropicBaseUrl) map.set(hostOf(p.anthropicBaseUrl), p);
      if (p.openaiBaseUrl) map.set(hostOf(p.openaiBaseUrl), p);
    }
    return map;
  });

  function hostOf(url: string): string {
    try { return new URL(url).host; } catch { return url; }
  }

  function configuredEntry(e: ProviderCatalogEntry): ModelProvider | undefined {
    return configuredByHost.get(hostOf(e.apiHost))
      ?? (e.anthropicHost ? configuredByHost.get(hostOf(e.anthropicHost)) : undefined);
  }

  const filtered = $derived(
    PROVIDER_CATALOG.filter((e) => {
      const matchCat = activeCategory === 'all' || e.category === activeCategory;
      const q = query.trim().toLowerCase();
      const matchQ = !q || e.name.toLowerCase().includes(q) || e.apiHost.toLowerCase().includes(q) || (e.description ?? '').toLowerCase().includes(q);
      return matchCat && matchQ;
    })
  );

  function categoryLabel(c: ProviderCategory): string {
    return PROVIDER_CATEGORIES.find((x) => x.id === c)?.label ?? c;
  }

  async function toggleEnabled(p: ModelProvider) {
    pageError = '';
    try {
      await updateProvider(p.id, { enabled: !p.enabled });
      await refreshActive(); // 禁用/启用后重新拉取激活状态
      void refreshSyncStatus();
    } catch (e) {
      pageError = String(e);
    }
  }

  let removingId = $state<string | null>(null); // 两步删除确认:当前展开确认的服务商 id

  async function removeProvider(id: string) {
    removingId = null;
    pageError = '';
    try {
      await deleteProvider(id);
      await refreshActive(); // 删除激活服务商时后端会自动清除 active_provider_id
    } catch (e) {
      pageError = String(e);
    }
  }

  // ---------- 添加 / 编辑内联配置面板(全局无弹窗:面板整行插在所点卡片所在行的行尾之后) ----------
  let editorOpen = $state(false);
  let editingId = $state<string | null>(null); // null = 新增
  let editingEntry = $state<ProviderCatalogEntry | null>(null); // 目录条目,提供官方地址一键填充

  // 网格实际列数(auto-fill 随宽度变),浏览器解析后的真值 + ResizeObserver 跟踪;
  // 配置面板插入位置(行尾)依赖它。
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

  /** 编辑中卡片所在行的行尾索引;面板渲染在该索引卡片之后(整行插入,同行卡片不动) */
  const editRowEnd = $derived.by(() => {
    if (!editorOpen || !editingEntry) return -1;
    const idx = filtered.findIndex((x) => x.id === editingEntry!.id);
    if (idx < 0) return -1;
    return Math.min(Math.floor(idx / gridCols) * gridCols + gridCols - 1, filtered.length - 1);
  });
  let formName = $state('');
  let formAnthropicUrl = $state(''); // Anthropic 兼容端点槽
  let formOpenaiUrl = $state(''); // OpenAI 兼容端点槽
  let formApiKey = $state('');
  let formDefaultModel = $state('');
  let formModels = $state<string[]>([]);
  let formEnabled = $state(true);
  let showKey = $state(false);
  let formError = $state('');
  let saving = $state(false);

  // 每行端点独立的测试状态 / 拉取模型状态
  let testingSlot = $state<ProviderFlavor | null>(null);
  let slotResults = $state<Record<ProviderFlavor, ProviderTestResult | null>>({
    anthropic: null,
    openai: null,
  });
  let fetching = $state(false);
  let fetchedModels = $state<string[] | null>(null);
  let fetchError = $state('');
  let modelInput = $state('');

  /** 目录条目的官方 Anthropic 端点(官方 Anthropic 条目的 apiHost 本身就是 Anthropic 协议) */
  function anthropicHostOf(e: ProviderCatalogEntry | null): string | null {
    if (!e) return null;
    if (e.id === 'anthropic') return e.apiHost;
    return e.anthropicHost ?? null;
  }

  /** 目录条目的官方 OpenAI 端点 */
  function openaiHostOf(e: ProviderCatalogEntry | null): string | null {
    if (!e) return null;
    if (e.id === 'anthropic') return null;
    return e.apiHost;
  }

  function slotUrl(slot: ProviderFlavor): string {
    return (slot === 'anthropic' ? formAnthropicUrl : formOpenaiUrl).trim();
  }

  /** 对单个端点槽测试连接,结果只写回该行,两行互不干扰 */
  async function testSlot(slot: ProviderFlavor) {
    const url = slotUrl(slot);
    if (!url || !formApiKey.trim() || testingSlot !== null) return;
    testingSlot = slot;
    slotResults = { ...slotResults, [slot]: null };
    try {
      const r = await provider_test(url, formApiKey.trim(), slot);
      slotResults = { ...slotResults, [slot]: r };
    } catch (e) {
      slotResults = { ...slotResults, [slot]: { ok: false, latencyMs: 0, models: [], error: String(e) } };
    } finally {
      testingSlot = null;
    }
  }

  /** 拉取模型用的槽:anthropic 槽优先,取第一个非空端点 */
  function fetchSlot(): { url: string; flavor: ProviderFlavor } | null {
    const a = slotUrl('anthropic');
    if (a) return { url: a, flavor: 'anthropic' };
    const o = slotUrl('openai');
    if (o) return { url: o, flavor: 'openai' };
    return null;
  }

  const canFetch = $derived(
    (!!formAnthropicUrl.trim() || !!formOpenaiUrl.trim()) && !!formApiKey.trim()
  );

  async function fetchModels() {
    const slot = fetchSlot();
    if (!slot) return;
    fetching = true;
    fetchError = '';
    fetchedModels = null;
    try {
      const r = await provider_test(slot.url, formApiKey.trim(), slot.flavor);
      if (r.ok) {
        fetchedModels = r.models;
      } else {
        fetchError = r.error ?? 'Request failed';
      }
    } catch (e) {
      fetchError = String(e);
    } finally {
      fetching = false;
    }
  }

  function addModelFromInput() {
    const m = modelInput.trim();
    if (m && !formModels.includes(m)) formModels = [...formModels, m];
    modelInput = '';
  }

  function removeModel(m: string) {
    formModels = formModels.filter((x) => x !== m);
  }

  function toggleFetchedModel(m: string) {
    formModels = formModels.includes(m) ? formModels.filter((x) => x !== m) : [...formModels, m];
  }

  function resetTransientState() {
    showKey = false;
    formError = '';
    testingSlot = null;
    slotResults = { anthropic: null, openai: null };
    fetching = false;
    fetchedModels = null;
    fetchError = '';
    modelInput = '';
  }

  function closeEditor() {
    editorOpen = false;
    editingEntry = null;
    editingId = null;
  }

  function openAdd(e: ProviderCatalogEntry) {
    // 再次点击同一张卡片的按钮时收起面板
    if (editorOpen && editingEntry?.id === e.id) {
      closeEditor();
      return;
    }
    editingId = null;
    editingEntry = e;
    formName = e.name;
    formAnthropicUrl = anthropicHostOf(e) ?? '';
    formOpenaiUrl = openaiHostOf(e) ?? '';
    formApiKey = '';
    formDefaultModel = e.defaultModel ?? '';
    formModels = [];
    formEnabled = true;
    resetTransientState();
    editorOpen = true;
  }

  function openEdit(e: ProviderCatalogEntry, p: ModelProvider) {
    if (editorOpen && editingEntry?.id === e.id) {
      closeEditor();
      return;
    }
    editingId = p.id;
    editingEntry = e;
    formName = p.name;
    // 空槽直接预填目录官方地址,免得用户还要手动找端点;不想要清空即可
    formAnthropicUrl = p.anthropicBaseUrl || (anthropicHostOf(e) ?? '');
    formOpenaiUrl = p.openaiBaseUrl || (openaiHostOf(e) ?? '');
    formApiKey = p.apiKey;
    formDefaultModel = p.defaultModel;
    formModels = [...(p.models ?? [])];
    formEnabled = p.enabled;
    resetTransientState();
    editorOpen = true;
  }

  async function saveProvider() {
    formError = '';
    const name = formName.trim();
    if (!name) {
      formError = $_('providers.form.nameRequired');
      return;
    }
    const anthropicBaseUrl = formAnthropicUrl.trim();
    const openaiBaseUrl = formOpenaiUrl.trim();
    if (!anthropicBaseUrl && !openaiBaseUrl) {
      formError = $_('providers.form.endpointRequired');
      return;
    }
    const data = {
      name,
      anthropicBaseUrl,
      openaiBaseUrl,
      apiKey: formApiKey.trim(),
      defaultModel: formDefaultModel.trim(),
      models: formModels,
      enabled: formEnabled,
    };
    saving = true;
    try {
      if (editingId === null) {
        await addProvider({ id: crypto.randomUUID(), ...data });
      } else {
        await updateProvider(editingId, data);
      }
      closeEditor();
      void refreshSyncStatus();
    } catch (e) {
      formError = String(e);
    } finally {
      saving = false;
    }
  }

  // ---------- 同步到 Agent(逐行状态 + 单行同步 + 批量应用,结果就地写回行内) ----------
  type SyncStage = 'closed' | 'planning' | 'preview';
  let syncStage = $state<SyncStage>('closed');
  let plans = $state<AgentPlan[]>([]);
  let syncError = $state(''); // plan / 批量 invoke 整体失败
  let expanded = $state<Record<string, boolean>>({}); // 行内明细展开/收起
  let checked = $state<Record<string, boolean>>({}); // 批量应用勾选
  let batchApplying = $state(false); // 批量应用进行中
  let rowApplying = $state<Record<string, boolean>>({}); // 行同步中(行内 spinner)
  let rowError = $state<Record<string, string>>({}); // 行 apply 失败红字
  let rowSynced = $state<Record<string, boolean>>({}); // apply 成功后就地翻「已同步」
  let rowBackup = $state<Record<string, string>>({}); // apply 成功后的备份路径(行内小字)

  // agent_id → 显示名;优先用 agents 注册表 label,取不到时回退本地映射
  const FALLBACK_LABELS: Record<string, string> = {
    'claude-code': 'Claude Code',
    codex: 'Codex',
    opencode: 'OpenCode',
    'cursor-agent': 'Cursor Agent',
    codebuddy: 'CodeBuddy',
    openclaw: 'OpenClaw',
    hermes: 'Hermes',
    kimi: 'Kimi CLI',
    qodercli: 'Qoder CLI',
  };
  let agentLabels = $state<Record<string, string>>({});

  function agentLabel(id: string): string {
    return agentLabels[id] ?? FALLBACK_LABELS[id] ?? id;
  }

  // 有 unsupportedReason 文案的 agent(与 i18n 键集对齐;svelte-i18n 没有
  // 「键是否存在」查询,用本地常量表判断最稳)。未列出的 unsupported agent
  // 只显示标签不显示小字。
  const UNSUPPORTED_REASON_IDS = ['cursor-agent', 'qodercli'];

  /** 实际变更项(add/update/remove) */
  function realChanges(p: AgentPlan): ChangeItem[] {
    return p.changes.filter((c) => c.action === 'add' || c.action === 'update' || c.action === 'remove');
  }

  function skipItems(p: AgentPlan): ChangeItem[] {
    return p.changes.filter((c) => c.action === 'skip');
  }

  /** 单行状态:apply 成功后 rowSynced 就地覆盖为「已同步」(其它行 plan 数据不刷新) */
  type RowStatus = 'synced' | 'pending' | 'skipped' | 'unsupported' | 'error';

  function rowStatus(p: AgentPlan): RowStatus {
    if (!p.supported) return 'unsupported';
    if (p.error) return 'error';
    if (rowSynced[p.agent_id]) return 'synced';
    if (realChanges(p).length > 0) return 'pending';
    if (skipItems(p).length > 0) return 'skipped';
    return 'synced'; // 全部 unchanged 或无变更项
  }

  /** 可勾选参与批量应用:supported、无 plan error、有实际变更、尚未在本面板同步成功 */
  function selectable(p: AgentPlan): boolean {
    return p.supported && !p.error && !rowSynced[p.agent_id] && realChanges(p).length > 0;
  }

  const checkedCount = $derived(plans.filter((p) => selectable(p) && checked[p.agent_id]).length);
  const selectablePlans = $derived(plans.filter((p) => selectable(p)));
  const allPlansPicked = $derived(
    selectablePlans.length > 0 && selectablePlans.every((p) => checked[p.agent_id])
  );

  function toggleAllPlans() {
    const next = { ...checked };
    for (const p of selectablePlans) next[p.agent_id] = !allPlansPicked;
    checked = next;
  }

  async function startSync() {
    syncStage = 'planning';
    syncError = '';
    plans = [];
    expanded = {};
    checked = {};
    batchApplying = false;
    rowApplying = {};
    rowError = {};
    rowSynced = {};
    rowBackup = {};
    try {
      plans = await sync_providers_plan();
      // 默认勾选有实际变更的 supported agent
      checked = Object.fromEntries(plans.filter(selectable).map((p) => [p.agent_id, true]));
      syncStage = 'preview';
    } catch (e) {
      syncError = String(e);
      syncStage = 'preview';
    }
  }

  /** apply 结果就地写回行内:成功翻「已同步」+ 记备份路径,失败行内红字 */
  function recordResult(r: ApplyResult) {
    if (r.ok) {
      rowSynced = { ...rowSynced, [r.agent_id]: true };
      rowError = { ...rowError, [r.agent_id]: '' };
      if (r.backup_path) rowBackup = { ...rowBackup, [r.agent_id]: r.backup_path };
    } else {
      rowError = { ...rowError, [r.agent_id]: r.error ?? 'apply failed' };
    }
  }

  /** 只对该 agent 应用;成功后仅翻当前行状态(其它行 plan 数据不刷新,面板重开才重新 plan) */
  async function applyOne(id: string) {
    if (rowApplying[id] || batchApplying) return;
    rowApplying = { ...rowApplying, [id]: true };
    rowError = { ...rowError, [id]: '' };
    try {
      const results = await sync_providers_apply([id]);
      if (results[0]) {
        recordResult(results[0]);
        if (results[0].ok) void refreshSyncStatus();
      }
    } catch (e) {
      rowError = { ...rowError, [id]: String(e) };
    } finally {
      rowApplying = { ...rowApplying, [id]: false };
    }
  }

  /** 批量应用勾选的 agent;结果同样逐行就地写回,不切换视图 */
  async function applyChecked() {
    const ids = plans.filter((p) => selectable(p) && checked[p.agent_id]).map((p) => p.agent_id);
    if (ids.length === 0 || batchApplying) return;
    batchApplying = true;
    syncError = '';
    rowApplying = { ...rowApplying, ...Object.fromEntries(ids.map((id) => [id, true])) };
    try {
      const results = await sync_providers_apply(ids);
      for (const r of results) recordResult(r);
      if (results.some((r) => r.ok)) void refreshSyncStatus();
    } catch (e) {
      syncError = String(e);
    } finally {
      batchApplying = false;
      rowApplying = { ...rowApplying, ...Object.fromEntries(ids.map((id) => [id, false])) };
    }
  }

  function toggleExpand(id: string) {
    expanded = { ...expanded, [id]: !expanded[id] };
  }

  function closeSync() {
    syncStage = 'closed';
    void refreshSyncStatus();
  }

  function unchangedCount(p: AgentPlan): number {
    return p.changes.filter((c) => c.action === 'unchanged').length;
  }

  onMount(async () => {
    try {
      await loadProviders();
    } catch (e) {
      pageError = String(e);
    }
    await refreshActive();
    void refreshSyncStatus();
    try {
      const all = await agents_list();
      agentLabels = Object.fromEntries(all.map((a) => [a.id, a.label]));
    } catch { /* 回退本地映射即可 */ }
  });
</script>

<svelte:window
  onkeydown={(e) => {
    if (e.key !== 'Escape') return;
    if (editorOpen) closeEditor();
    else if (syncStage === 'preview' && !batchApplying) closeSync();
  }}
/>

<div class="providers-page">
  <header class="page-header">
    <div>
      <h1>{$_('nav.providers')}</h1>
    </div>
    <div class="header-actions">
      <div class="search-box">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <circle cx="11" cy="11" r="8"/><line x1="21" y1="21" x2="16.65" y2="16.65"/>
        </svg>
        <input type="text" bind:value={query} placeholder={$_('providers.search')} />
      </div>
      <button
        class="btn primary"
        onclick={startSync}
        disabled={syncStage !== 'closed'}
      >
        {$_('providers.syncToAgents')}
      </button>
    </div>
  </header>

  {#if pageError}
    <pre class="error-text">{pageError}</pre>
  {/if}

  <!-- 同步内联面板(按钮下方整行展开,全局无弹窗;逐行状态 + 单行同步) -->
  {#if syncStage !== 'closed'}
    <div class="sync-panel glass-card">
      {#if syncStage === 'planning'}
        <div class="loading"><span class="spinner"></span> {$_('providers.sync.planning')}</div>
      {:else}
        <h3>{$_('providers.sync.previewTitle')}</h3>
        {#if activeMissing}
          <p class="sync-hint">{$_('providers.sync.noActiveHint')}</p>
        {/if}
        {#if syncError}
          <pre class="error-text">{syncError}</pre>
        {/if}
        {#if plans.length > 0}
          <div class="plan-list">
            <label class="select-all-plans">
              <input
                type="checkbox"
                class="row-check"
                disabled={selectablePlans.length === 0 || batchApplying}
                checked={allPlansPicked}
                onchange={toggleAllPlans}
              />
              <span>{$_('providers.sync.selectAll')}</span>
              <span class="selectable-count">{checkedCount}/{selectablePlans.length}</span>
            </label>
            {#each plans as p (p.agent_id)}
              {@const status = rowStatus(p)}
              {@const changes = realChanges(p)}
              {@const skips = skipItems(p)}
              {@const expandable = p.changes.length > 0}
              {@const canPick = selectable(p)}
              <div class="plan-item" class:muted={status === 'unsupported'}>
                <div class="plan-row">
                  <input
                    type="checkbox"
                    class="row-check"
                    disabled={!canPick || batchApplying}
                    checked={canPick && !!checked[p.agent_id]}
                    onchange={(e) => (checked = { ...checked, [p.agent_id]: e.currentTarget.checked })}
                    aria-label={agentLabel(p.agent_id)}
                  />
                  <!-- 行本身可点击:内联展开/收起变更明细 -->
                  <button
                    type="button"
                    class="plan-head"
                    class:expandable
                    disabled={!expandable}
                    onclick={() => toggleExpand(p.agent_id)}
                  >
                    <AgentLogo id={p.agent_id} label={agentLabel(p.agent_id)} />
                    <div class="plan-info">
                      <div class="plan-title-line">
                        <span class="agent-name">{agentLabel(p.agent_id)}</span>
                        {#if status === 'synced'}
                          <span class="tag green">{$_('providers.sync.statusSynced')}</span>
                        {:else if status === 'pending'}
                          <span class="tag yellow">{$_('providers.sync.statusPending')}</span>
                          <span class="change-summary">
                            {$_('providers.sync.changeCount', { values: { count: changes.length } })}
                          </span>
                        {:else if status === 'skipped'}
                          <span class="tag amber">{$_('providers.sync.statusSkipped')}</span>
                          {#if skips[0]?.detail}
                            <span class="skip-reason">{skips[0].detail}</span>
                          {/if}
                        {:else if status === 'unsupported'}
                          <span class="tag gray">{$_('providers.sync.unsupported')}</span>
                          {#if UNSUPPORTED_REASON_IDS.includes(p.agent_id)}
                            <span class="skip-reason">{$_(`providers.sync.unsupportedReason.${p.agent_id}`)}</span>
                          {/if}
                        {:else}
                          <span class="tag red">{$_('providers.sync.error')}</span>
                        {/if}
                      </div>
                      {#if p.supported}
                        <code class="config-path" title={p.config_path}>{p.config_path}</code>
                      {/if}
                    </div>
                    {#if expandable}
                      <span class="chevron" class:open={!!expanded[p.agent_id]}>▾</span>
                    {/if}
                  </button>
                  {#if status === 'pending'}
                    <button
                      class="btn primary sync-one"
                      onclick={() => applyOne(p.agent_id)}
                      disabled={!!rowApplying[p.agent_id] || batchApplying}
                    >
                      {#if rowApplying[p.agent_id]}<span class="spinner small"></span>{/if}
                      {$_('providers.sync.syncOne')}
                    </button>
                  {/if}
                </div>
                {#if rowSynced[p.agent_id] && rowBackup[p.agent_id]}
                  <code class="config-path" title={rowBackup[p.agent_id]}>
                    {$_('providers.sync.backup')}: {rowBackup[p.agent_id]}
                  </code>
                {/if}
                {#if p.error}
                  <pre class="error-text">{p.error}</pre>
                {/if}
                {#if rowError[p.agent_id]}
                  <pre class="error-text">{rowError[p.agent_id]}</pre>
                {/if}
                {#if expandable && expanded[p.agent_id]}
                  <ul class="change-list">
                    {#each changes as c (c.name + c.action)}
                      <li class="change action-{c.action}">
                        <span class="change-action">{$_(`providers.sync.action.${c.action}`)}</span>
                        <span class="change-name">{c.name}</span>
                        {#if c.detail}<span class="change-detail">{c.detail}</span>{/if}
                      </li>
                    {/each}
                    {#each skips as c (c.name)}
                      <li class="change action-skip">
                        <span class="change-action">{$_('providers.sync.action.skip')}</span>
                        <span class="change-name">{c.name}</span>
                        {#if c.detail}<span class="change-detail">{c.detail}</span>{/if}
                      </li>
                    {/each}
                  </ul>
                  {#if unchangedCount(p) > 0}
                    <span class="unchanged-note">
                      {$_('providers.sync.unchangedCount', { values: { count: unchangedCount(p) } })}
                    </span>
                  {/if}
                {/if}
              </div>
            {/each}
          </div>
        {/if}
        <div class="panel-actions">
          <button class="btn" onclick={closeSync} disabled={batchApplying}>{$_('providers.close')}</button>
          <button class="btn primary" onclick={applyChecked} disabled={checkedCount === 0 || batchApplying}>
            {#if batchApplying}<span class="spinner small"></span>{/if}
            {$_('providers.sync.confirm', { values: { count: checkedCount } })}
          </button>
        </div>
      {/if}
    </div>
  {/if}

  <div class="category-bar">
    <button class="chip" class:active={activeCategory === 'all'} onclick={() => (activeCategory = 'all')}>
      {$_('providers.all')}
    </button>
    {#each PROVIDER_CATEGORIES as cat (cat.id)}
      <button class="chip" class:active={activeCategory === cat.id} onclick={() => (activeCategory = cat.id)}>
        {cat.label}
      </button>
    {/each}
  </div>

  <div class="provider-grid" bind:this={gridEl}>
    {#each filtered as e, i (e.id)}
      {@const configured = configuredEntry(e)}
      <div
        class="provider-card glass-card"
        class:added={!!configured}
        class:default-active={!!configured && activeProviderId === configured.id}
      >
        <div class="card-top">
          <ProviderLogo entry={e} />
          <div class="card-head-info">
            <div class="card-title">
              <span class="name">{e.name}</span>
              <span class="cat-badge cat-{e.category}">{categoryLabel(e.category)}</span>
              {#if configured && activeProviderId === configured.id}
                <span class="default-badge">{$_('providers.defaultBadge')}</span>
              {/if}
            </div>
            {#if e.description}<div class="desc">{e.description}</div>{/if}
          </div>
        </div>

        <div class="card-meta">
          {#if configured}
            <!-- 已配置:显示配了哪些端点,地址悬停可见 -->
            {#if configured.anthropicBaseUrl}
              <span class="endpoint-chip anthropic" title={configured.anthropicBaseUrl}>Anthropic</span>
            {/if}
            {#if configured.openaiBaseUrl}
              <span class="endpoint-chip openai" title={configured.openaiBaseUrl}>OpenAI</span>
            {/if}
          {:else}
            <code class="host" title={e.apiHost}>{e.apiHost.replace(/^https?:\/\//, '')}</code>
          {/if}
          {#if configured && (configured.models?.length ?? 0) > 0}
            <span class="model-count">{$_('providers.modelCount', { values: { count: configured.models.length } })}</span>
          {/if}
          <!-- 同步状态:仅已配置且 enabled 的卡片;status 拉取失败时整体不显示。
               语义:分母 = 当前适用的 agent 数(synced+pending);全到位才算已同步。 -->
          {#if configured && configured.enabled && cardStatusLoaded}
            {@const st = cardStatus[configured.id]}
            {@const synced = st?.synced_agents.length ?? 0}
            {@const total = synced + (st?.pending_agents.length ?? 0)}
            {#if st && total > 0 && synced === total}
              <span
                class="sync-badge synced"
                title={$_('providers.syncedTo', { values: { agents: st.synced_agents.map(agentLabel).join(', ') } })}
              >{$_('providers.sync.statusSynced')} {synced}/{total}</span>
            {:else if st && synced > 0}
              <span
                class="sync-badge pending"
                title={$_('providers.pendingTo', { values: { agents: st.pending_agents.map(agentLabel).join(', ') } })}
              >{$_('providers.sync.statusPending')} {synced}/{total}</span>
            {:else}
              <span
                class="sync-badge pending"
                title={st && total > 0 ? $_('providers.pendingTo', { values: { agents: st.pending_agents.map(agentLabel).join(', ') } }) : undefined}
              >{$_('providers.sync.statusPending')}</span>
            {/if}
          {/if}
        </div>

        <div class="card-actions">
          {#if e.website}
            <a class="link" href={e.website} target="_blank" rel="noreferrer">{$_('providers.website')}</a>
          {/if}
          <span class="spacer"></span>
          {#if configured}
            {#if configured.enabled || activeProviderId === configured.id}
              <button
                class="btn star"
                class:on={activeProviderId === configured.id}
                onclick={() => toggleActive(configured)}
                title={activeProviderId === configured.id ? $_('providers.unsetDefault') : $_('providers.setDefault')}
                aria-label={activeProviderId === configured.id ? $_('providers.unsetDefault') : $_('providers.setDefault')}
              >{activeProviderId === configured.id ? '★' : '☆'}</button>
            {/if}
            <button class="btn toggle" class:on={configured.enabled} onclick={() => toggleEnabled(configured)}>
              {configured.enabled ? $_('providers.enabled') : $_('providers.disabled')}
            </button>
            <button class="btn" class:active={editorOpen && editingEntry?.id === e.id} onclick={() => openEdit(e, configured)}>{$_('providers.configure')}</button>
            {#if removingId === configured.id}
              <button class="btn danger" onclick={() => removeProvider(configured.id)}>{$_('providers.confirmRemove')}</button>
              <button class="btn" onclick={() => (removingId = null)}>{$_('providers.cancel')}</button>
            {:else}
              <button class="btn remove" onclick={() => (removingId = configured.id)} title={$_('providers.remove')}>✕</button>
            {/if}
          {:else}
            <button class="btn primary" onclick={() => openAdd(e)}>{$_('providers.configure')}</button>
          {/if}
        </div>
      </div>

      <!-- 内联配置面板:整行插在所点卡片所在行的行尾之后,同行卡片位置不动 -->
      {#if editorOpen && i === editRowEnd}
        <div class="config-panel glass-card">
          <h3>{editingId === null ? $_('providers.addTitle') : $_('providers.editTitle')} · {editingEntry?.name}</h3>

          <div class="form-row">
            <label for="pv-name">{$_('providers.form.name')} *</label>
            <input id="pv-name" type="text" bind:value={formName} />
          </div>

          <!-- 双端点槽:至少填一个;每行独立测试 -->
          <div class="form-row">
            <label for="pv-anthropic-url">{$_('providers.form.anthropicEndpoint')}</label>
            <div class="endpoint-row">
              <input
                id="pv-anthropic-url"
                type="text"
                bind:value={formAnthropicUrl}
                placeholder={anthropicHostOf(editingEntry) ?? 'https://api.anthropic.com'}
              />
              <button
                type="button"
                class="btn"
                onclick={() => testSlot('anthropic')}
                disabled={testingSlot !== null || !formAnthropicUrl.trim() || !formApiKey.trim()}
              >
                {#if testingSlot === 'anthropic'}<span class="spinner small"></span>{/if}
                {$_('providers.form.test')}
              </button>
            </div>
            {#if slotResults.anthropic}
              {#if slotResults.anthropic.ok}
                <span class="test-ok">✓ {$_('providers.form.testOk', { values: { ms: slotResults.anthropic.latencyMs, count: slotResults.anthropic.models.length } })}</span>
              {:else}
                <span class="test-fail">✗ {slotResults.anthropic.error}</span>
              {/if}
            {/if}
          </div>

          <div class="form-row">
            <label for="pv-openai-url">{$_('providers.form.openaiEndpoint')}</label>
            <div class="endpoint-row">
              <input
                id="pv-openai-url"
                type="text"
                bind:value={formOpenaiUrl}
                placeholder={openaiHostOf(editingEntry) ?? 'https://api.openai.com/v1'}
              />
              <button
                type="button"
                class="btn"
                onclick={() => testSlot('openai')}
                disabled={testingSlot !== null || !formOpenaiUrl.trim() || !formApiKey.trim()}
              >
                {#if testingSlot === 'openai'}<span class="spinner small"></span>{/if}
                {$_('providers.form.test')}
              </button>
            </div>
            {#if slotResults.openai}
              {#if slotResults.openai.ok}
                <span class="test-ok">✓ {$_('providers.form.testOk', { values: { ms: slotResults.openai.latencyMs, count: slotResults.openai.models.length } })}</span>
              {:else}
                <span class="test-fail">✗ {slotResults.openai.error}</span>
              {/if}
            {/if}
          </div>

          <div class="form-row">
            <label for="pv-key">{$_('providers.form.apiKey')}</label>
            <div class="key-input">
              {#if showKey}
                <input id="pv-key" type="text" bind:value={formApiKey} placeholder="sk-..." autocomplete="off" spellcheck="false" />
              {:else}
                <input id="pv-key" type="password" bind:value={formApiKey} placeholder="sk-..." autocomplete="off" />
              {/if}
              <button
                type="button"
                class="eye"
                onclick={() => (showKey = !showKey)}
                title={showKey ? $_('providers.form.hideKey') : $_('providers.form.showKey')}
                aria-label={showKey ? $_('providers.form.hideKey') : $_('providers.form.showKey')}
              >
                {#if showKey}
                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                    <path d="M17.94 17.94A10.07 10.07 0 0 1 12 20c-7 0-11-8-11-8a18.45 18.45 0 0 1 5.06-5.94M9.9 4.24A9.12 9.12 0 0 1 12 4c7 0 11 8 11 8a18.5 18.5 0 0 1-2.16 3.19m-6.72-1.07a3 3 0 1 1-4.24-4.24"/>
                    <line x1="1" y1="1" x2="23" y2="23"/>
                  </svg>
                {:else}
                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                    <path d="M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z"/>
                    <circle cx="12" cy="12" r="3"/>
                  </svg>
                {/if}
              </button>
            </div>
          </div>

          <div class="form-row">
            <label for="pv-model-input">{$_('providers.form.models')}</label>
            {#if formModels.length > 0}
              <div class="model-chips">
                {#each formModels as m (m)}
                  <span class="model-chip">
                    {m}
                    <button
                      type="button"
                      class="chip-remove"
                      onclick={() => removeModel(m)}
                      title={$_('providers.form.removeModel')}
                      aria-label={$_('providers.form.removeModel')}
                    >✕</button>
                  </span>
                {/each}
              </div>
            {/if}
            <div class="model-add">
              <input
                id="pv-model-input"
                type="text"
                bind:value={modelInput}
                placeholder={$_('providers.form.modelPlaceholder')}
                onkeydown={(e) => { if (e.key === 'Enter') { e.preventDefault(); addModelFromInput(); } }}
              />
              <button type="button" class="btn" onclick={addModelFromInput} disabled={!modelInput.trim()}>
                {$_('providers.form.addModel')}
              </button>
              <button type="button" class="btn" onclick={fetchModels} disabled={fetching || !canFetch}>
                {#if fetching}<span class="spinner small"></span>{/if}
                {$_('providers.form.fetchModels')}
              </button>
            </div>
            {#if fetchError}
              <span class="test-fail">✗ {fetchError}</span>
            {/if}
            {#if fetchedModels !== null}
              {#if fetchedModels.length === 0}
                <span class="hint">{$_('providers.form.noModelsFetched')}</span>
              {:else}
                <div class="fetched-panel">
                  {#each fetchedModels as m (m)}
                    <button
                      type="button"
                      class="fetched-item"
                      class:selected={formModels.includes(m)}
                      onclick={() => toggleFetchedModel(m)}
                    >
                      <span class="tick">{formModels.includes(m) ? '✓' : '+'}</span>{m}
                    </button>
                  {/each}
                </div>
              {/if}
            {/if}
          </div>

          <div class="form-row">
            <label for="pv-model">{$_('providers.form.defaultModel')}</label>
            {#if formModels.length > 0}
              <select id="pv-model" bind:value={formDefaultModel}>
                <option value="">—</option>
                {#if formDefaultModel && !formModels.includes(formDefaultModel)}
                  <option value={formDefaultModel}>{formDefaultModel}</option>
                {/if}
                {#each formModels as m (m)}
                  <option value={m}>{m}</option>
                {/each}
              </select>
            {:else}
              <input id="pv-model" type="text" bind:value={formDefaultModel} placeholder="gpt-4o" />
            {/if}
          </div>

          <div class="form-row">
            <label class="check-label">
              <input type="checkbox" bind:checked={formEnabled} />
              {$_('providers.form.enabled')}
            </label>
          </div>

          {#if formError}
            <pre class="error-text">{formError}</pre>
          {/if}

          <div class="panel-actions">
            <span class="spacer"></span>
            <button class="btn" onclick={closeEditor}>{$_('providers.cancel')}</button>
            <button class="btn primary" onclick={saveProvider} disabled={saving}>
              {#if saving}<span class="spinner small"></span>{/if}
              {$_('providers.save')}
            </button>
          </div>
        </div>
      {/if}
    {/each}
  </div>

  {#if filtered.length === 0}
    <div class="empty">{$_('providers.noResults')}</div>
  {/if}
</div>

<style>
  .providers-page { padding: 1.5rem; display: flex; flex-direction: column; gap: 1rem; }

  .page-header { display: flex; justify-content: space-between; align-items: center; gap: 1rem; flex-wrap: wrap; }
  .page-header h1 { margin: 0; font-size: 1.25rem; }
  .header-actions { display: flex; align-items: center; gap: 0.6rem; flex-wrap: wrap; }

  .search-box {
    display: flex; align-items: center; gap: 0.5rem;
    background: var(--bg-secondary); border: 1px solid rgba(255,255,255,0.1);
    border-radius: 0.5rem; padding: 0.4rem 0.7rem; min-width: 220px;
  }
  .search-box svg { width: 16px; height: 16px; opacity: 0.6; }
  .search-box input {
    border: none; background: transparent; color: var(--text-primary);
    font-size: 0.85rem; outline: none; width: 100%;
  }

  .category-bar { display: flex; gap: 0.5rem; flex-wrap: wrap; }
  .chip {
    padding: 0.3rem 0.85rem; border-radius: 999px; font-size: 0.8rem;
    background: var(--bg-secondary); border: 1px solid rgba(255,255,255,0.1);
    color: var(--text-secondary); cursor: pointer; transition: all 0.2s ease;
  }
  .chip:hover { color: var(--text-primary); }
  .chip.active { background: rgba(0,245,255,0.12); border-color: var(--neon-cyan); color: var(--neon-cyan); }

  .provider-grid {
    display: grid; gap: 0.9rem;
    grid-template-columns: repeat(auto-fill, minmax(290px, 1fr));
  }

  .provider-card { padding: 1rem 1.1rem; display: flex; flex-direction: column; gap: 0.75rem; transition: border-color 0.2s ease; }
  .provider-card.added { border-color: rgba(94,234,212,0.35); }
  .provider-card.default-active { border-color: var(--neon-cyan); box-shadow: 0 0 0 1px rgba(0,245,255,0.25); }
  .default-badge {
    font-size: 0.65rem; padding: 0.1rem 0.5rem; border-radius: 999px; white-space: nowrap;
    background: rgba(0,245,255,0.14); color: var(--neon-cyan); border: 1px solid rgba(0,245,255,0.35);
  }

  .card-top { display: flex; align-items: center; gap: 0.85rem; }
  .card-head-info { min-width: 0; }
  .card-title { display: flex; align-items: center; gap: 0.5rem; flex-wrap: wrap; }
  .name { font-weight: 600; color: var(--text-primary); }
  .cat-badge { font-size: 0.65rem; padding: 0.1rem 0.5rem; border-radius: 999px; white-space: nowrap; }
  .cat-badge.cat-intl { background: rgba(66,133,244,0.15); color: #7aa7ff; }
  .cat-badge.cat-cn { background: rgba(255,107,107,0.15); color: #ff8b8b; }
  .cat-badge.cat-aggregator { background: rgba(123,97,255,0.15); color: #a99bff; }
  .cat-badge.cat-local { background: rgba(148,163,184,0.15); color: #cbd5e1; }
  .desc { color: var(--text-muted); font-size: 0.75rem; margin-top: 0.2rem; }

  .card-meta { display: flex; align-items: center; gap: 0.5rem; flex-wrap: wrap; }
  .host {
    font-size: 0.7rem; color: var(--text-secondary); background: var(--bg-tertiary);
    padding: 0.15rem 0.45rem; border-radius: 0.3rem; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; max-width: 100%;
  }
  .endpoint-chip { font-size: 0.65rem; padding: 0.1rem 0.45rem; border-radius: 999px; white-space: nowrap; cursor: help; }
  .endpoint-chip.anthropic { color: #d97757; background: rgba(217,119,87,0.14); }
  .endpoint-chip.openai { color: #74aa9c; background: rgba(116,170,156,0.14); }
  .model-count { font-size: 0.65rem; color: var(--neon-green); background: rgba(94,234,212,0.1); padding: 0.1rem 0.45rem; border-radius: 999px; white-space: nowrap; }
  .sync-badge { font-size: 0.65rem; padding: 0.1rem 0.45rem; border-radius: 999px; white-space: nowrap; }
  .sync-badge.synced { background: rgba(74,222,128,0.15); color: #4ade80; cursor: help; }
  .sync-badge.pending { background: rgba(251,191,36,0.15); color: #fbbf24; }

  .card-actions { display: flex; align-items: center; gap: 0.5rem; margin-top: auto; }
  .spacer { flex: 1; }
  .link { font-size: 0.75rem; color: var(--neon-cyan); }
  .link:hover { text-decoration: underline; }

  .btn {
    padding: 0.3rem 0.8rem; border-radius: 0.4rem; font-size: 0.75rem; cursor: pointer;
    border: 1px solid rgba(255,255,255,0.15); background: transparent; color: var(--text-primary);
  }
  .btn:hover { background: var(--bg-tertiary); }
  .btn.primary { background: rgba(0,245,255,0.14); border-color: var(--neon-cyan); color: var(--neon-cyan); }
  .btn.active { border-color: var(--neon-cyan); color: var(--neon-cyan); }
  .btn.toggle { color: var(--text-muted); }
  .btn.toggle.on { color: var(--neon-green); border-color: rgba(94,234,212,0.4); background: rgba(94,234,212,0.1); }
  .btn.remove { color: var(--neon-pink); border-color: rgba(255,0,110,0.3); padding: 0.3rem 0.55rem; }
  .btn.danger { background: rgba(248,113,113,0.15); border-color: #f87171; color: #f87171; }
  .btn.star { color: var(--text-muted); padding: 0.3rem 0.55rem; font-size: 0.85rem; line-height: 1; }
  .btn.star.on { color: #fbbf24; border-color: rgba(251,191,36,0.45); background: rgba(251,191,36,0.12); }
  .btn:disabled { opacity: 0.5; cursor: default; }

  .empty { text-align: center; color: var(--text-muted); padding: 3rem; }

  .error-text { font-size: 0.75rem; color: #f87171; white-space: pre-wrap; margin: 0; }

  .spinner { width: 16px; height: 16px; border: 2px solid rgba(94,234,212,0.3); border-top-color: #5eead4; border-radius: 50%; animation: spin 0.8s linear infinite; display: inline-block; }
  .spinner.small { width: 12px; height: 12px; }
  @keyframes spin { to { transform: rotate(360deg); } }

  /* 内联配置面板:占满 grid 整行,插在被点击卡片之后 */
  .config-panel {
    grid-column: 1 / -1;
    padding: 1.3rem 1.5rem; display: flex; flex-direction: column; gap: 0.9rem;
    background: var(--bg-secondary);
    border-color: rgba(0,245,255,0.35);
  }
  .config-panel h3 { margin: 0; }
  .panel-actions { display: flex; align-items: center; justify-content: flex-end; gap: 0.6rem; margin-top: 0.3rem; flex-wrap: wrap; }

  .test-ok { font-size: 0.75rem; color: var(--neon-green, #5eead4); }
  .test-fail { font-size: 0.75rem; color: #f87171; word-break: break-word; }
  .hint { font-size: 0.75rem; color: var(--text-muted); }

  .model-chips { display: flex; flex-wrap: wrap; gap: 0.4rem; }
  .model-chip {
    display: inline-flex; align-items: center; gap: 0.3rem;
    font-size: 0.75rem; color: var(--text-primary); background: var(--bg-tertiary);
    border: 1px solid rgba(255,255,255,0.12); border-radius: 999px; padding: 0.15rem 0.35rem 0.15rem 0.6rem;
  }
  .chip-remove {
    background: transparent; border: none; cursor: pointer; color: var(--text-muted);
    font-size: 0.7rem; padding: 0 0.2rem; line-height: 1;
  }
  .chip-remove:hover { color: var(--neon-pink); }

  .model-add { display: flex; gap: 0.5rem; align-items: center; }
  .model-add input {
    flex: 1; background: var(--bg-tertiary); border: 1px solid rgba(255,255,255,0.1); border-radius: 0.4rem;
    padding: 0.45rem 0.6rem; color: var(--text-primary); font-size: 0.82rem; outline: none; box-sizing: border-box;
  }
  .model-add input:focus { border-color: var(--neon-cyan); }
  .model-add .btn { white-space: nowrap; }

  .fetched-panel {
    display: flex; flex-wrap: wrap; gap: 0.35rem; max-height: 180px; overflow-y: auto;
    border: 1px solid rgba(255,255,255,0.1); border-radius: 0.4rem; padding: 0.5rem;
    background: var(--bg-tertiary);
  }
  .fetched-item {
    display: inline-flex; align-items: center; gap: 0.3rem;
    font-size: 0.72rem; padding: 0.2rem 0.55rem; border-radius: 999px; cursor: pointer;
    background: transparent; border: 1px solid rgba(255,255,255,0.15); color: var(--text-secondary);
  }
  .fetched-item:hover { color: var(--text-primary); border-color: rgba(255,255,255,0.3); }
  .fetched-item.selected { color: var(--neon-green, #5eead4); border-color: rgba(94,234,212,0.4); background: rgba(94,234,212,0.08); }
  .fetched-item .tick { font-size: 0.7rem; }

  .form-row select {
    background: var(--bg-tertiary); border: 1px solid rgba(255,255,255,0.1); border-radius: 0.4rem;
    padding: 0.45rem 0.6rem; color: var(--text-primary); font-size: 0.82rem; outline: none; width: 100%;
    box-sizing: border-box;
  }
  .form-row select:focus { border-color: var(--neon-cyan); }

  .form-row { display: flex; flex-direction: column; gap: 0.35rem; }
  .form-row label { font-size: 0.78rem; color: var(--text-secondary); }
  .form-row input[type="text"], .form-row input[type="password"] {
    background: var(--bg-tertiary); border: 1px solid rgba(255,255,255,0.1); border-radius: 0.4rem;
    padding: 0.45rem 0.6rem; color: var(--text-primary); font-size: 0.82rem; outline: none; width: 100%;
    box-sizing: border-box;
  }
  .form-row input:focus { border-color: var(--neon-cyan); }

  .endpoint-row { display: flex; gap: 0.5rem; align-items: center; }
  .endpoint-row input { flex: 1; min-width: 0; }
  .endpoint-row .btn { white-space: nowrap; flex-shrink: 0; display: inline-flex; align-items: center; gap: 0.3rem; }

  .key-input { position: relative; display: flex; align-items: center; }
  .key-input input { padding-right: 2.4rem; }
  .eye {
    position: absolute; right: 0.4rem; display: flex; align-items: center; justify-content: center;
    background: transparent; border: none; cursor: pointer; color: var(--text-muted); padding: 0.25rem;
  }
  .eye:hover { color: var(--text-primary); }
  .eye svg { width: 16px; height: 16px; }

  .check-label { display: flex; align-items: center; gap: 0.5rem; font-size: 0.82rem; cursor: pointer; }

  /* 同步面板(样式与 MCP 同步面板一致) */
  .sync-panel {
    padding: 1.3rem 1.5rem; display: flex; flex-direction: column; gap: 0.9rem;
    background: var(--bg-secondary);
    border-color: rgba(0,245,255,0.35);
  }
  .sync-panel h3 { margin: 0; }
  .sync-hint { margin: 0; font-size: 0.78rem; color: #fbbf24; }

  .loading { padding: 2rem; display: flex; justify-content: center; align-items: center; gap: 0.5rem; }

  .plan-list { display: flex; flex-direction: column; gap: 0.7rem; overflow-y: auto; }
  .select-all-plans {
    display: flex; align-items: center; gap: 0.6rem; cursor: pointer;
    padding-bottom: 0.45rem; border-bottom: 1px solid rgba(255,255,255,0.08);
    color: var(--neon-cyan); font-weight: 600; font-size: 0.82rem;
  }
  .selectable-count { color: var(--text-muted); font-weight: 400; font-size: 0.75rem; }
  .plan-item {
    border: 1px solid rgba(255,255,255,0.08); border-radius: 0.5rem;
    padding: 0.6rem 0.9rem; display: flex; flex-direction: column; gap: 0.4rem;
  }
  .plan-item.muted { opacity: 0.55; }
  .plan-row { display: flex; align-items: center; gap: 0.6rem; }
  .row-check { flex-shrink: 0; cursor: pointer; }
  .row-check:disabled { cursor: default; opacity: 0.4; }
  /* 行头是按钮(点击展开明细),重置按钮默认样式 */
  .plan-head {
    flex: 1; min-width: 0; display: flex; align-items: center; gap: 0.6rem;
    background: transparent; border: none; padding: 0; margin: 0;
    color: inherit; font: inherit; text-align: left; cursor: default;
  }
  .plan-head.expandable { cursor: pointer; }
  .plan-head:disabled { cursor: default; }
  .plan-info { min-width: 0; flex: 1; display: flex; flex-direction: column; gap: 0.2rem; }
  .plan-title-line { display: flex; align-items: center; gap: 0.5rem; flex-wrap: wrap; }
  .agent-name { font-weight: 600; font-size: 0.9rem; }
  .change-summary { font-size: 0.72rem; color: var(--text-secondary); }
  .skip-reason { font-size: 0.72rem; color: var(--text-muted); }
  .chevron {
    flex-shrink: 0; font-size: 0.8rem; color: var(--text-muted);
    transition: transform 0.15s ease;
  }
  .chevron.open { transform: rotate(180deg); }
  .sync-one { flex-shrink: 0; display: inline-flex; align-items: center; gap: 0.35rem; }
  .config-path {
    font-size: 0.68rem; color: var(--text-muted);
    white-space: nowrap; overflow: hidden; text-overflow: ellipsis; display: block;
    max-width: 100%;
  }
  .tag { font-size: 0.65rem; padding: 0.1rem 0.5rem; border-radius: 999px; white-space: nowrap; }
  .tag.gray { background: rgba(148,163,184,0.15); color: #cbd5e1; }
  .tag.red { background: rgba(248,113,113,0.15); color: #f87171; }
  .tag.green { background: rgba(74,222,128,0.15); color: #4ade80; }
  .tag.yellow { background: rgba(251,191,36,0.15); color: #fbbf24; }
  .tag.amber { background: rgba(202,164,60,0.12); color: #d0b978; }

  .change-list { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 0.25rem; }
  .change { display: flex; align-items: baseline; gap: 0.5rem; font-size: 0.78rem; min-width: 0; }
  .change-action { font-size: 0.65rem; padding: 0.05rem 0.45rem; border-radius: 999px; white-space: nowrap; flex-shrink: 0; }
  .action-add .change-action { background: rgba(74,222,128,0.15); color: #4ade80; }
  .action-update .change-action { background: rgba(251,191,36,0.15); color: #fbbf24; }
  .action-remove .change-action { background: rgba(248,113,113,0.15); color: #f87171; }
  .action-skip .change-action { background: rgba(148,163,184,0.15); color: #cbd5e1; }
  .action-skip { opacity: 0.7; }
  .change-name { font-family: monospace; }
  .change-detail { color: var(--text-muted); font-size: 0.72rem; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .unchanged-note { font-size: 0.72rem; color: var(--text-muted); }
</style>
