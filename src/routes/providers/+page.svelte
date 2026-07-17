<script lang="ts">
  import { onMount } from 'svelte';
  import { _ } from 'svelte-i18n';
  import {
    PROVIDER_CATALOG, PROVIDER_CATEGORIES,
    type ProviderCatalogEntry, type ProviderCategory,
  } from '$lib/data/providers';
  import ProviderLogo from '$lib/components/ProviderLogo.svelte';
  import { providers, addProvider, updateProvider, deleteProvider, loadProviders } from '$lib/stores/config';
  import { provider_test, type ModelProvider, type ProviderFlavor, type ProviderTestResult } from '$lib/api/config';

  let query = $state('');
  let activeCategory = $state<ProviderCategory | 'all'>('all');
  let pageError = $state('');

  // 已配置的服务商:按 host 匹配目录条目(OpenAI 端点与 Anthropic 兼容端点均可匹配)
  const configuredByHost = $derived.by(() => {
    const map = new Map<string, ModelProvider>();
    for (const p of $providers) {
      map.set(hostOf(p.baseUrl), p);
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
    } catch (e) {
      pageError = String(e);
    }
  }

  async function removeProvider(id: string) {
    pageError = '';
    try {
      await deleteProvider(id);
    } catch (e) {
      pageError = String(e);
    }
  }

  // ---------- 添加 / 编辑内联配置面板(全局无弹窗:面板在卡片正下方整行展开) ----------
  let editorOpen = $state(false);
  let editingId = $state<string | null>(null); // null = 新增
  let editingEntry = $state<ProviderCatalogEntry | null>(null); // 目录条目,提供 anthropicHost 切换
  let formName = $state('');
  let formBaseUrl = $state('');
  let formApiKey = $state('');
  let formDefaultModel = $state('');
  let formModels = $state<string[]>([]);
  let formEnabled = $state(true);
  let showKey = $state(false);
  let formError = $state('');
  let saving = $state(false);

  // 测试连接 / 拉取模型状态
  let testing = $state(false);
  let testResult = $state<ProviderTestResult | null>(null);
  let fetching = $state(false);
  let fetchedModels = $state<string[] | null>(null);
  let fetchError = $state('');
  let modelInput = $state('');

  /** 当前端点风格:切到 Anthropic 兼容端点或官方 Anthropic 条目时用 anthropic 协议 */
  function currentFlavor(): ProviderFlavor {
    const url = formBaseUrl.trim();
    if (editingEntry?.anthropicHost && url === editingEntry.anthropicHost) return 'anthropic';
    if (editingEntry?.id === 'anthropic') return 'anthropic';
    return 'openai';
  }

  const canTest = $derived(!!formBaseUrl.trim() && !!formApiKey.trim());

  async function testConnection() {
    testing = true;
    testResult = null;
    try {
      testResult = await provider_test(formBaseUrl.trim(), formApiKey.trim(), currentFlavor());
    } catch (e) {
      testResult = { ok: false, latencyMs: 0, models: [], error: String(e) };
    } finally {
      testing = false;
    }
  }

  async function fetchModels() {
    fetching = true;
    fetchError = '';
    fetchedModels = null;
    try {
      const r = await provider_test(formBaseUrl.trim(), formApiKey.trim(), currentFlavor());
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
    testing = false;
    testResult = null;
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
    formBaseUrl = e.apiHost;
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
    formBaseUrl = p.baseUrl;
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
    const baseUrl = formBaseUrl.trim();
    if (!baseUrl) {
      formError = $_('providers.form.baseUrlRequired');
      return;
    }
    const data = {
      name,
      baseUrl,
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
    } catch (e) {
      formError = String(e);
    } finally {
      saving = false;
    }
  }

  onMount(async () => {
    try {
      await loadProviders();
    } catch (e) {
      pageError = String(e);
    }
  });
</script>

<svelte:window onkeydown={(e) => { if (e.key === 'Escape' && editorOpen) closeEditor(); }} />

<div class="providers-page">
  <header class="page-header">
    <div>
      <h1>{$_('nav.providers')}</h1>
    </div>
    <div class="search-box">
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
        <circle cx="11" cy="11" r="8"/><line x1="21" y1="21" x2="16.65" y2="16.65"/>
      </svg>
      <input type="text" bind:value={query} placeholder={$_('providers.search')} />
    </div>
  </header>

  {#if pageError}
    <pre class="error-text">{pageError}</pre>
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

  <div class="provider-grid">
    {#each filtered as e (e.id)}
      {@const configured = configuredEntry(e)}
      <div class="provider-card glass-card" class:added={!!configured}>
        <div class="card-top">
          <ProviderLogo entry={e} />
          <div class="card-head-info">
            <div class="card-title">
              <span class="name">{e.name}</span>
              <span class="cat-badge cat-{e.category}">{categoryLabel(e.category)}</span>
            </div>
            {#if e.description}<div class="desc">{e.description}</div>{/if}
          </div>
        </div>

        <div class="card-meta">
          <code class="host" title={e.apiHost}>{e.apiHost.replace(/^https?:\/\//, '')}</code>
          {#if e.anthropicHost}
            <span class="compat" title={e.anthropicHost}>Anthropic 兼容</span>
          {/if}
          {#if configured && (configured.models?.length ?? 0) > 0}
            <span class="model-count">{$_('providers.modelCount', { values: { count: configured.models.length } })}</span>
          {/if}
        </div>

        <div class="card-actions">
          {#if e.website}
            <a class="link" href={e.website} target="_blank" rel="noreferrer">{$_('providers.website')}</a>
          {/if}
          <span class="spacer"></span>
          {#if configured}
            <button class="btn toggle" class:on={configured.enabled} onclick={() => toggleEnabled(configured)}>
              {configured.enabled ? $_('providers.enabled') : $_('providers.disabled')}
            </button>
            <button class="btn" class:active={editorOpen && editingEntry?.id === e.id} onclick={() => openEdit(e, configured)}>{$_('providers.configure')}</button>
            <button class="btn remove" onclick={() => removeProvider(configured.id)} title={$_('providers.remove')}>✕</button>
          {:else}
            <button class="btn primary" onclick={() => openAdd(e)}>{$_('providers.add')}</button>
          {/if}
        </div>
      </div>

      <!-- 内联配置面板:在被点击的卡片正下方整行展开(grid-column: 1 / -1) -->
      {#if editorOpen && editingEntry?.id === e.id}
        <div class="config-panel glass-card">
          <h3>{editingId === null ? $_('providers.addTitle') : $_('providers.editTitle')}</h3>

          <div class="form-row">
            <label for="pv-name">{$_('providers.form.name')} *</label>
            <input id="pv-name" type="text" bind:value={formName} />
          </div>

          <div class="form-row">
            <label for="pv-base-url">{$_('providers.form.baseUrl')} *</label>
            {#if editingEntry?.anthropicHost}
              <div class="endpoint-switch">
                <button
                  class="chip"
                  class:active={formBaseUrl.trim() === editingEntry.apiHost}
                  onclick={() => (formBaseUrl = editingEntry!.apiHost)}
                >{$_('providers.form.endpointOpenai')}</button>
                <button
                  class="chip"
                  class:active={formBaseUrl.trim() === editingEntry.anthropicHost}
                  onclick={() => (formBaseUrl = editingEntry!.anthropicHost!)}
                >{$_('providers.form.endpointAnthropic')}</button>
              </div>
            {/if}
            <input id="pv-base-url" type="text" bind:value={formBaseUrl} placeholder="https://api.example.com/v1" />
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
              <button type="button" class="btn" onclick={fetchModels} disabled={fetching || !canTest}>
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
            <button class="btn" onclick={testConnection} disabled={testing || !canTest}>
              {#if testing}<span class="spinner small"></span>{/if}
              {testing ? $_('providers.form.testing') : $_('providers.form.testConnection')}
            </button>
            {#if testResult}
              {#if testResult.ok}
                <span class="test-ok">✓ {$_('providers.form.testOk', { values: { ms: testResult.latencyMs, count: testResult.models.length } })}</span>
              {:else}
                <span class="test-fail">✗ {testResult.error}</span>
              {/if}
            {/if}
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
  .compat { font-size: 0.65rem; color: var(--neon-cyan); background: rgba(0,245,255,0.1); padding: 0.1rem 0.45rem; border-radius: 999px; }
  .model-count { font-size: 0.65rem; color: var(--neon-green); background: rgba(94,234,212,0.1); padding: 0.1rem 0.45rem; border-radius: 999px; white-space: nowrap; }

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

  .endpoint-switch { display: flex; gap: 0.5rem; margin-bottom: 0.15rem; }
  .endpoint-switch .chip { background: var(--bg-tertiary); }

  .key-input { position: relative; display: flex; align-items: center; }
  .key-input input { padding-right: 2.4rem; }
  .eye {
    position: absolute; right: 0.4rem; display: flex; align-items: center; justify-content: center;
    background: transparent; border: none; cursor: pointer; color: var(--text-muted); padding: 0.25rem;
  }
  .eye:hover { color: var(--text-primary); }
  .eye svg { width: 16px; height: 16px; }

  .check-label { display: flex; align-items: center; gap: 0.5rem; font-size: 0.82rem; cursor: pointer; }
</style>
