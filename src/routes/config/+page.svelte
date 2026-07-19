<script lang="ts">
  import { onMount } from 'svelte';
  import { _ } from 'svelte-i18n';
  import {
    config_mcp_list, config_mcp_upsert, config_mcp_remove,
    sync_mcp_plan, sync_mcp_apply,
    type McpServerSpec, type AgentPlan, type ApplyResult, type ChangeItem,
  } from '$lib/api/mcpSync';
  import {
    parseMcpJson, serializeServers, MCP_JSON_EXAMPLE,
    type NamedServer, type ParseIssue,
  } from '$lib/api/mcpJson';
  import { agents_list } from '$lib/api/agents';

  // ---------- 服务器列表 ----------
  let servers = $state<Record<string, McpServerSpec>>({});
  let isLoading = $state(true);
  let listError = $state('');
  let deleting = $state<string | null>(null); // 两步删除确认:当前展开确认的服务器名

  const serverNames = $derived(Object.keys(servers).sort());

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

  async function refresh() {
    isLoading = true;
    listError = '';
    try {
      servers = await config_mcp_list();
    } catch (e) {
      listError = String(e);
    } finally {
      isLoading = false;
    }
  }

  onMount(async () => {
    await refresh();
    try {
      const all = await agents_list();
      agentLabels = Object.fromEntries(all.map((a) => [a.id, a.label]));
    } catch { /* 回退本地映射即可 */ }
  });

  function summaryOf(spec: McpServerSpec): string {
    if (spec.kind === 'stdio') {
      return [spec.command ?? '', ...spec.args].join(' ').trim();
    }
    return spec.url ?? '';
  }

  async function toggleEnabled(name: string) {
    const spec = servers[name];
    if (!spec) return;
    const next = { ...spec, enabled: !spec.enabled };
    servers = { ...servers, [name]: next }; // 乐观更新
    try {
      await config_mcp_upsert(name, next);
    } catch (e) {
      servers = { ...servers, [name]: spec }; // 回滚
      listError = String(e);
    }
  }

  async function removeServer(name: string) {
    deleting = null;
    try {
      await config_mcp_remove(name);
      await refresh();
    } catch (e) {
      listError = String(e);
    }
  }

  // ---------- 添加 / 编辑内联面板(全局无弹窗:添加=列表顶部展开,编辑=该行下方展开) ----------
  let editorOpen = $state(false);
  let editingName = $state<string | null>(null); // null = 新增
  let formName = $state('');
  let formKind = $state<'stdio' | 'http'>('stdio');
  let formCommand = $state('');
  let formArgs = $state(''); // 逐行输入,存数组
  let formUrl = $state('');
  let formEnabled = $state(true);
  let formEnv = $state<Array<{ key: string; value: string }>>([]);
  let formHeaders = $state<Array<{ key: string; value: string }>>([]);
  let formError = $state('');
  let saving = $state(false);

  // JSON 模式:textarea 内容 + 实时解析结果
  let editorMode = $state<'form' | 'json'>('form');
  let jsonText = $state('');
  const jsonParse = $derived(jsonText.trim() ? parseMcpJson(jsonText) : null);

  function issueText(issue: ParseIssue): string {
    switch (issue.code) {
      case 'invalidJson':
        return $_('mcp.form.jsonErrInvalid', { values: { detail: issue.detail } });
      case 'notObject':
        return $_('mcp.form.jsonErrNotObject');
      case 'bareSpec':
        return $_('mcp.form.jsonErrBareSpec');
      case 'noServers':
        return $_('mcp.form.jsonErrNoServers');
      case 'emptyName':
        return $_('mcp.form.jsonErrEmptyName');
      case 'notServer':
        return $_('mcp.form.jsonErrNotServer', { values: { name: issue.name } });
      case 'missingCommand':
        return $_('mcp.form.jsonErrMissingCommand', { values: { name: issue.name } });
      case 'missingUrl':
        return $_('mcp.form.jsonErrMissingUrl', { values: { name: issue.name } });
    }
  }

  function pairsFrom(rec: Record<string, string>): Array<{ key: string; value: string }> {
    return Object.entries(rec).map(([key, value]) => ({ key, value }));
  }

  function recFrom(pairs: Array<{ key: string; value: string }>): Record<string, string> {
    const out: Record<string, string> = {};
    for (const { key, value } of pairs) {
      if (key.trim()) out[key.trim()] = value;
    }
    return out;
  }

  function closeEditor() {
    editorOpen = false;
    editingName = null;
  }

  function openAdd() {
    // 再次点击「添加服务器」时收起
    if (editorOpen && editingName === null) {
      closeEditor();
      return;
    }
    editingName = null;
    formName = '';
    formKind = 'stdio';
    formCommand = '';
    formArgs = '';
    formUrl = '';
    formEnabled = true;
    formEnv = [];
    formHeaders = [];
    formError = '';
    editorMode = 'form';
    jsonText = '';
    editorOpen = true;
  }

  function openEdit(name: string) {
    // 再次点击同一行的「编辑」时收起
    if (editorOpen && editingName === name) {
      closeEditor();
      return;
    }
    const spec = servers[name];
    if (!spec) return;
    editingName = name;
    formName = name;
    formKind = spec.kind;
    formCommand = spec.command ?? '';
    formArgs = spec.args.join('\n');
    formUrl = spec.url ?? '';
    formEnabled = spec.enabled;
    formEnv = pairsFrom(spec.env);
    formHeaders = pairsFrom(spec.headers);
    formError = '';
    editorMode = 'form';
    jsonText = '';
    editorOpen = true;
  }

  function parseArgs(text: string): string[] {
    // 逐行优先;单行时按空格分隔
    const lines = text.split('\n').map((s) => s.trim()).filter(Boolean);
    if (lines.length > 1) return lines;
    if (lines.length === 1) return lines[0].split(/\s+/).filter(Boolean);
    return [];
  }

  /** 当前表单内容 → spec(不做必填校验,切 JSON 序列化时也会用) */
  function buildSpecFromForm(): McpServerSpec {
    return {
      kind: formKind,
      command: formKind === 'stdio' ? formCommand.trim() : null,
      args: formKind === 'stdio' ? parseArgs(formArgs) : [],
      env: formKind === 'stdio' ? recFrom(formEnv) : {},
      url: formKind === 'http' ? formUrl.trim() : null,
      headers: formKind === 'http' ? recFrom(formHeaders) : {},
      enabled: formEnabled,
    };
  }

  /** JSON 解析出的单个 server 回填表单(编辑模式下名称锁定为 editingName,不回填) */
  function fillFormFrom({ name, spec }: NamedServer) {
    if (editingName === null) formName = name;
    formKind = spec.kind;
    formCommand = spec.command ?? '';
    formArgs = spec.args.join('\n');
    formUrl = spec.url ?? '';
    formEnabled = spec.enabled;
    formEnv = pairsFrom(spec.env);
    formHeaders = pairsFrom(spec.headers);
  }

  /** 表单是否完全空白(新增时切 JSON 不序列化空表单,留 placeholder 示例) */
  function formIsBlank(): boolean {
    return (
      !formName.trim() && !formCommand.trim() && !formArgs.trim() && !formUrl.trim() &&
      formEnv.every((p) => !p.key.trim() && !p.value.trim()) &&
      formHeaders.every((p) => !p.key.trim() && !p.value.trim())
    );
  }

  function switchMode(mode: 'form' | 'json') {
    if (mode === editorMode) return;
    formError = '';
    if (mode === 'json') {
      // 表单 → JSON:把当前表单内容序列化进 textarea;空表单则留空展示 placeholder 示例
      jsonText = formIsBlank()
        ? ''
        : serializeServers([{ name: (editingName ?? formName).trim() || 'my-server', spec: buildSpecFromForm() }]);
      editorMode = 'json';
      return;
    }
    // JSON → 表单:空内容直接切;恰好 1 个 server 回填;否则报错阻止切换
    if (!jsonText.trim()) {
      editorMode = 'form';
      return;
    }
    const res = parseMcpJson(jsonText);
    if (!res.ok) {
      formError = res.issues.map(issueText).join('\n');
      return;
    }
    if (res.servers.length > 1) {
      formError = $_('mcp.form.jsonMultiSwitch');
      return;
    }
    fillFormFrom(res.servers[0]);
    editorMode = 'form';
  }

  /** JSON 模式保存:逐个 upsert(同名覆盖),部分失败时提示失败项并保留面板 */
  async function saveJson() {
    formError = '';
    if (!jsonText.trim()) {
      formError = $_('mcp.form.jsonErrNoServers');
      return;
    }
    const res = parseMcpJson(jsonText);
    if (!res.ok) {
      formError = res.issues.map(issueText).join('\n');
      return;
    }
    saving = true;
    const failed: string[] = [];
    let lastError = '';
    try {
      for (const { name, spec } of res.servers) {
        try {
          await config_mcp_upsert(name, spec);
        } catch (e) {
          failed.push(name);
          lastError = String(e);
        }
      }
      if (failed.length === 0) {
        closeEditor();
      } else {
        formError = `${$_('mcp.form.jsonPartialFail', { values: { names: failed.join(', ') } })}\n${lastError}`;
      }
      await refresh();
    } finally {
      saving = false;
    }
  }

  async function saveServer() {
    formError = '';
    const name = (editingName ?? formName).trim();
    if (!name) {
      formError = $_('mcp.form.nameRequired');
      return;
    }
    if (editingName === null && servers[name]) {
      formError = $_('mcp.form.nameExists');
      return;
    }
    if (formKind === 'stdio' && !formCommand.trim()) {
      formError = $_('mcp.form.commandRequired');
      return;
    }
    if (formKind === 'http' && !formUrl.trim()) {
      formError = $_('mcp.form.urlRequired');
      return;
    }
    const spec = buildSpecFromForm();
    saving = true;
    try {
      await config_mcp_upsert(name, spec);
      closeEditor();
      await refresh();
    } catch (e) {
      formError = String(e);
    } finally {
      saving = false;
    }
  }

  // ---------- 同步流程 ----------
  type SyncStage = 'closed' | 'planning' | 'preview' | 'applying' | 'result';
  let syncStage = $state<SyncStage>('closed');
  let plans = $state<AgentPlan[]>([]);
  let syncError = $state('');
  let checked = $state<Record<string, boolean>>({});
  let results = $state<ApplyResult[]>([]);

  /** 有实际变更(add/update/remove)才可勾选 */
  function realChanges(p: AgentPlan): ChangeItem[] {
    return p.changes.filter((c) => c.action === 'add' || c.action === 'update' || c.action === 'remove');
  }

  function selectable(p: AgentPlan): boolean {
    return p.supported && !p.error && realChanges(p).length > 0;
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
    results = [];
    try {
      plans = await sync_mcp_plan();
      checked = Object.fromEntries(plans.filter(selectable).map((p) => [p.agent_id, true]));
      syncStage = 'preview';
    } catch (e) {
      syncError = String(e);
      syncStage = 'preview';
    }
  }

  async function applySync() {
    const ids = plans.filter((p) => selectable(p) && checked[p.agent_id]).map((p) => p.agent_id);
    if (ids.length === 0) return;
    syncStage = 'applying';
    syncError = '';
    try {
      results = await sync_mcp_apply(ids);
      syncStage = 'result';
    } catch (e) {
      syncError = String(e);
      syncStage = 'result';
    }
  }

  async function closeSync() {
    syncStage = 'closed';
    await refresh();
  }

  function unchangedCount(p: AgentPlan): number {
    return p.changes.filter((c) => c.action === 'unchanged').length;
  }
</script>

<svelte:window
  onkeydown={(e) => {
    if (e.key !== 'Escape') return;
    if (editorOpen) closeEditor();
    else if (syncStage === 'preview' || syncStage === 'result') closeSync();
  }}
/>

<!-- 添加 / 编辑内联表单(添加时在列表顶部展开,编辑时在对应行下方展开) -->
{#snippet editorPanel()}
  <div class="editor-panel glass-card">
    <h3>{editingName === null ? $_('mcp.addServer') : $_('mcp.editServer')}</h3>

    <div class="form-row">
      <div class="kind-switch" role="tablist">
        <button class="chip" class:active={editorMode === 'form'} onclick={() => switchMode('form')}>
          {$_('mcp.form.modeForm')}
        </button>
        <button class="chip" class:active={editorMode === 'json'} onclick={() => switchMode('json')}>
          {$_('mcp.form.modeJson')}
        </button>
      </div>
    </div>

    {#if editorMode === 'form'}
    <div class="form-row">
      <label for="mcp-name">{$_('mcp.form.name')}</label>
      <input
        id="mcp-name"
        type="text"
        bind:value={formName}
        disabled={editingName !== null}
        placeholder="my-server"
      />
    </div>

    <div class="form-row">
      <span class="form-label">{$_('mcp.form.kind')}</span>
      <div class="kind-switch">
        <button class="chip" class:active={formKind === 'stdio'} onclick={() => (formKind = 'stdio')}>stdio</button>
        <button class="chip" class:active={formKind === 'http'} onclick={() => (formKind = 'http')}>http</button>
      </div>
    </div>

    {#if formKind === 'stdio'}
      <div class="form-row">
        <label for="mcp-command">{$_('mcp.form.command')} *</label>
        <input id="mcp-command" type="text" bind:value={formCommand} placeholder="npx" />
      </div>
      <div class="form-row">
        <label for="mcp-args">{$_('mcp.form.args')}</label>
        <textarea id="mcp-args" rows="3" bind:value={formArgs} placeholder={$_('mcp.form.argsHint')}></textarea>
      </div>
      <div class="form-row">
        <span class="form-label">{$_('mcp.form.env')}</span>
        <div class="kv-editor">
          {#each formEnv as pair, i (i)}
            <div class="kv-row">
              <input type="text" placeholder="KEY" bind:value={pair.key} />
              <input type="text" placeholder="value" bind:value={pair.value} />
              <button class="btn remove" onclick={() => (formEnv = formEnv.filter((_p, j) => j !== i))}>✕</button>
            </div>
          {/each}
          <button class="btn small" onclick={() => (formEnv = [...formEnv, { key: '', value: '' }])}>
            + {$_('mcp.form.addRow')}
          </button>
        </div>
      </div>
    {:else}
      <div class="form-row">
        <label for="mcp-url">{$_('mcp.form.url')} *</label>
        <input id="mcp-url" type="text" bind:value={formUrl} placeholder="https://example.com/mcp" />
      </div>
      <div class="form-row">
        <span class="form-label">{$_('mcp.form.headers')}</span>
        <div class="kv-editor">
          {#each formHeaders as pair, i (i)}
            <div class="kv-row">
              <input type="text" placeholder="Header" bind:value={pair.key} />
              <input type="text" placeholder="value" bind:value={pair.value} />
              <button class="btn remove" onclick={() => (formHeaders = formHeaders.filter((_p, j) => j !== i))}>✕</button>
            </div>
          {/each}
          <button class="btn small" onclick={() => (formHeaders = [...formHeaders, { key: '', value: '' }])}>
            + {$_('mcp.form.addRow')}
          </button>
        </div>
      </div>
    {/if}

    <div class="form-row">
      <label class="check-label">
        <input type="checkbox" bind:checked={formEnabled} />
        {$_('mcp.form.enabled')}
      </label>
    </div>
    {:else}
      <div class="form-row">
        <label for="mcp-json">{$_('mcp.form.jsonLabel')}</label>
        <textarea
          id="mcp-json"
          class="json-input"
          rows="12"
          bind:value={jsonText}
          placeholder={MCP_JSON_EXAMPLE}
          spellcheck="false"
        ></textarea>
      </div>
      {#if jsonParse}
        {#if jsonParse.ok}
          <p class="json-summary">
            {$_('mcp.form.jsonParsedCount', {
              values: {
                count: jsonParse.servers.length,
                names: jsonParse.servers.map((s) => s.name).join(', '),
              },
            })}
          </p>
        {:else}
          <pre class="error-text">{jsonParse.issues.map(issueText).join('\n')}</pre>
        {/if}
      {/if}
    {/if}

    {#if formError}
      <pre class="error-text">{formError}</pre>
    {/if}

    <div class="panel-actions">
      <button class="btn" onclick={closeEditor}>{$_('mcp.cancel')}</button>
      <button
        class="btn primary"
        onclick={() => (editorMode === 'json' ? saveJson() : saveServer())}
        disabled={saving}
      >
        {#if saving}<span class="spinner small"></span>{/if}
        {$_('mcp.save')}
      </button>
    </div>
  </div>
{/snippet}

<div class="mcp-page">
  <header class="page-header">
    <div>
      <h1>{$_('mcp.title')}</h1>
      <p class="subtitle">{$_('mcp.description')}</p>
    </div>
    <div class="header-actions">
      <button class="btn" class:active={editorOpen && editingName === null} onclick={openAdd}>{$_('mcp.addServer')}</button>
      <button
        class="btn primary"
        onclick={startSync}
        disabled={isLoading || serverNames.length === 0 || syncStage !== 'closed'}
      >
        {$_('mcp.syncToAgents')}
      </button>
    </div>
  </header>

  {#if listError}
    <pre class="error-text">{listError}</pre>
  {/if}

  <!-- 同步预览 / 结果内联区(按钮下方、列表上方展开) -->
  {#if syncStage !== 'closed'}
    <div class="sync-panel glass-card">
      {#if syncStage === 'planning'}
        <div class="loading"><span class="spinner"></span> {$_('mcp.sync.planning')}</div>
      {:else if syncStage === 'preview'}
        <h3>{$_('mcp.sync.previewTitle')}</h3>
        {#if syncError}
          <pre class="error-text">{syncError}</pre>
        {:else}
          <div class="plan-list">
            <label class="check-label select-all-plans">
              <input
                type="checkbox"
                disabled={selectablePlans.length === 0}
                checked={allPlansPicked}
                onchange={toggleAllPlans}
              />
              <span>{$_('mcp.sync.selectAll')}</span>
              <span class="selectable-count">{checkedCount}/{selectablePlans.length}</span>
            </label>
            {#each plans as p (p.agent_id)}
              {@const changes = realChanges(p)}
              {@const canPick = selectable(p)}
              <div class="plan-item" class:muted={!canPick}>
                <div class="plan-head">
                  <label class="check-label">
                    <input
                      type="checkbox"
                      disabled={!canPick}
                      checked={canPick && !!checked[p.agent_id]}
                      onchange={(e) => (checked = { ...checked, [p.agent_id]: e.currentTarget.checked })}
                    />
                    <span class="agent-name">{agentLabel(p.agent_id)}</span>
                  </label>
                  {#if !p.supported}
                    <span class="tag gray">{$_('mcp.sync.unsupported')}</span>
                  {:else if p.error}
                    <span class="tag red">{$_('mcp.sync.error')}</span>
                  {:else if changes.length === 0}
                    <span class="tag gray">{$_('mcp.sync.noChanges')}</span>
                  {/if}
                </div>
                {#if p.supported}
                  <code class="config-path" title={p.config_path}>{p.config_path}</code>
                {/if}
                {#if p.error}
                  <pre class="error-text">{p.error}</pre>
                {:else if p.supported}
                  {#if changes.length > 0}
                    <ul class="change-list">
                      {#each changes as c (c.name + c.action)}
                        <li class="change action-{c.action}">
                          <span class="change-action">{$_(`mcp.sync.action.${c.action}`)}</span>
                          <span class="change-name">{c.name}</span>
                          {#if c.detail}<span class="change-detail">{c.detail}</span>{/if}
                        </li>
                      {/each}
                      {#each p.changes.filter((c) => c.action === 'skip') as c (c.name)}
                        <li class="change action-skip">
                          <span class="change-action">{$_('mcp.sync.action.skip')}</span>
                          <span class="change-name">{c.name}</span>
                          {#if c.detail}<span class="change-detail">{c.detail}</span>{/if}
                        </li>
                      {/each}
                    </ul>
                  {/if}
                  {#if unchangedCount(p) > 0}
                    <span class="unchanged-note">
                      {$_('mcp.sync.unchangedCount', { values: { count: unchangedCount(p) } })}
                    </span>
                  {/if}
                {/if}
              </div>
            {/each}
          </div>
        {/if}
        <div class="panel-actions">
          <button class="btn" onclick={closeSync}>{$_('mcp.cancel')}</button>
          <button class="btn primary" onclick={applySync} disabled={checkedCount === 0}>
            {$_('mcp.sync.confirm', { values: { count: checkedCount } })}
          </button>
        </div>
      {:else if syncStage === 'applying'}
        <div class="loading"><span class="spinner"></span> {$_('mcp.sync.applying')}</div>
      {:else if syncStage === 'result'}
        <h3>{$_('mcp.sync.resultTitle')}</h3>
        {#if syncError}
          <pre class="error-text">{syncError}</pre>
        {/if}
        <div class="plan-list">
          {#each results as r (r.agent_id)}
            <div class="plan-item">
              <div class="plan-head">
                {#if r.ok}
                  <span class="result-icon ok">✓</span>
                  <span class="agent-name">{agentLabel(r.agent_id)}</span>
                  <span class="tag green">{$_('mcp.sync.appliedCount', { values: { count: r.applied } })}</span>
                {:else}
                  <span class="result-icon fail">✗</span>
                  <span class="agent-name">{agentLabel(r.agent_id)}</span>
                  <span class="tag red">{$_('mcp.sync.failed')}</span>
                {/if}
              </div>
              {#if r.ok && r.backup_path}
                <code class="config-path" title={r.backup_path}>{$_('mcp.sync.backup')}: {r.backup_path}</code>
              {/if}
              {#if !r.ok && r.error}
                <pre class="error-text">{r.error}</pre>
              {/if}
            </div>
          {/each}
        </div>
        <div class="panel-actions">
          <button class="btn primary" onclick={closeSync}>{$_('mcp.close')}</button>
        </div>
      {/if}
    </div>
  {/if}

  <!-- 新增表单:列表顶部展开 -->
  {#if editorOpen && editingName === null}
    {@render editorPanel()}
  {/if}

  {#if isLoading}
    <div class="loading glass-card"><span class="spinner"></span> {$_('mcp.loading')}</div>
  {:else if serverNames.length === 0}
    <div class="glass-card empty">
      <p>{$_('mcp.empty')}</p>
      {#if !editorOpen}
        <button class="btn primary" onclick={openAdd}>{$_('mcp.addServer')}</button>
      {/if}
    </div>
  {:else}
    <div class="server-list">
      {#each serverNames as name (name)}
        {@const spec = servers[name]}
        <div class="glass-card server-card" class:disabled={!spec.enabled}>
          <div class="server-main">
            <div class="server-title">
              <span class="server-name">{name}</span>
              <span class="kind-badge kind-{spec.kind}">{spec.kind}</span>
            </div>
            <code class="server-summary" title={summaryOf(spec)}>{summaryOf(spec)}</code>
          </div>
          <div class="server-actions">
            <button
              class="btn toggle"
              class:on={spec.enabled}
              onclick={() => toggleEnabled(name)}
              title={spec.enabled ? $_('mcp.enabled') : $_('mcp.disabled')}
            >
              {spec.enabled ? $_('mcp.enabled') : $_('mcp.disabled')}
            </button>
            <button class="btn" class:active={editorOpen && editingName === name} onclick={() => openEdit(name)}>{$_('mcp.edit')}</button>
            {#if deleting === name}
              <button class="btn danger" onclick={() => removeServer(name)}>{$_('mcp.confirmDelete')}</button>
              <button class="btn" onclick={() => (deleting = null)}>{$_('mcp.cancel')}</button>
            {:else}
              <button class="btn remove" onclick={() => (deleting = name)}>{$_('mcp.delete')}</button>
            {/if}
          </div>
        </div>

        <!-- 编辑表单:该行下方展开 -->
        {#if editorOpen && editingName === name}
          {@render editorPanel()}
        {/if}
      {/each}
    </div>
  {/if}
</div>

<style>
  .mcp-page { padding: 1.5rem; display: flex; flex-direction: column; gap: 1rem; }

  .page-header { display: flex; justify-content: space-between; align-items: center; gap: 1rem; flex-wrap: wrap; }
  .page-header h1 { margin: 0; font-size: 1.25rem; }
  .subtitle { margin: 0.25rem 0 0; color: var(--text-muted); font-size: 0.85rem; }
  .header-actions { display: flex; gap: 0.5rem; }

  .server-list { display: flex; flex-direction: column; gap: 0.7rem; }
  .server-card { padding: 0.9rem 1.1rem; display: flex; align-items: center; gap: 1rem; transition: opacity 0.2s ease; }
  .server-card.disabled { opacity: 0.55; }
  .server-main { min-width: 0; flex: 1; display: flex; flex-direction: column; gap: 0.3rem; }
  .server-title { display: flex; align-items: center; gap: 0.5rem; }
  .server-name { font-weight: 600; }
  .kind-badge { font-size: 0.65rem; padding: 0.1rem 0.5rem; border-radius: 999px; white-space: nowrap; }
  .kind-badge.kind-stdio { background: rgba(94, 234, 212, 0.15); color: #5eead4; }
  .kind-badge.kind-http { background: rgba(123, 97, 255, 0.15); color: #a99bff; }
  .server-summary {
    font-size: 0.72rem; color: var(--text-secondary);
    white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
  }
  .server-actions { display: flex; align-items: center; gap: 0.5rem; flex-shrink: 0; }

  .empty { padding: 3rem; display: flex; flex-direction: column; align-items: center; gap: 1rem; color: var(--text-muted); }
  .empty p { margin: 0; }

  .btn {
    padding: 0.3rem 0.8rem; border-radius: 0.4rem; font-size: 0.75rem; cursor: pointer;
    border: 1px solid rgba(255,255,255,0.15); background: transparent; color: var(--text-primary);
  }
  .btn:hover { background: var(--bg-tertiary); }
  .btn.primary { background: rgba(0,245,255,0.14); border-color: var(--neon-cyan); color: var(--neon-cyan); }
  .btn.active { border-color: var(--neon-cyan); color: var(--neon-cyan); }
  .btn.danger { background: rgba(248, 113, 113, 0.15); border-color: #f87171; color: #f87171; }
  .btn.remove { color: var(--neon-pink); border-color: rgba(255,0,110,0.3); }
  .btn.toggle { color: var(--text-muted); }
  .btn.toggle.on { color: var(--neon-green); border-color: rgba(94,234,212,0.4); background: rgba(94,234,212,0.1); }
  .btn.small { font-size: 0.7rem; padding: 0.2rem 0.6rem; align-self: flex-start; }
  .btn:disabled { opacity: 0.5; cursor: default; }

  .loading { padding: 2rem; display: flex; justify-content: center; align-items: center; gap: 0.5rem; }
  .spinner { width: 16px; height: 16px; border: 2px solid rgba(94,234,212,0.3); border-top-color: #5eead4; border-radius: 50%; animation: spin 0.8s linear infinite; display: inline-block; }
  .spinner.small { width: 12px; height: 12px; }
  @keyframes spin { to { transform: rotate(360deg); } }

  .error-text { font-size: 0.75rem; color: #f87171; white-space: pre-wrap; margin: 0; }

  /* 内联面板(编辑器 / 同步预览):页面内推开内容,无遮罩 */
  .editor-panel, .sync-panel {
    padding: 1.3rem 1.5rem; display: flex; flex-direction: column; gap: 0.9rem;
    background: var(--bg-secondary);
    border-color: rgba(0,245,255,0.35);
  }
  .editor-panel h3, .sync-panel h3 { margin: 0; }
  .panel-actions { display: flex; justify-content: flex-end; gap: 0.6rem; margin-top: 0.3rem; }

  .form-row { display: flex; flex-direction: column; gap: 0.35rem; }
  .form-row label, .form-label { font-size: 0.78rem; color: var(--text-secondary); }
  .form-row input[type="text"], .form-row textarea {
    background: var(--bg-tertiary); border: 1px solid rgba(255,255,255,0.1); border-radius: 0.4rem;
    padding: 0.45rem 0.6rem; color: var(--text-primary); font-size: 0.82rem; outline: none; width: 100%;
    box-sizing: border-box;
  }
  .form-row textarea { font-family: monospace; resize: vertical; }
  .form-row textarea.json-input { min-height: 200px; line-height: 1.5; }
  .json-summary { margin: 0; font-size: 0.75rem; color: #4ade80; }
  .form-row input:focus, .form-row textarea:focus { border-color: var(--neon-cyan); }
  .form-row input:disabled { opacity: 0.5; }

  .kind-switch { display: flex; gap: 0.5rem; }
  .chip {
    padding: 0.3rem 0.85rem; border-radius: 999px; font-size: 0.8rem;
    background: var(--bg-tertiary); border: 1px solid rgba(255,255,255,0.1);
    color: var(--text-secondary); cursor: pointer;
  }
  .chip.active { background: rgba(0,245,255,0.12); border-color: var(--neon-cyan); color: var(--neon-cyan); }

  .kv-editor { display: flex; flex-direction: column; gap: 0.4rem; }
  .kv-row { display: flex; gap: 0.4rem; }
  .kv-row input {
    background: var(--bg-tertiary); border: 1px solid rgba(255,255,255,0.1); border-radius: 0.4rem;
    padding: 0.35rem 0.55rem; color: var(--text-primary); font-size: 0.78rem; outline: none;
    font-family: monospace; min-width: 0; flex: 1;
  }
  .kv-row input:focus { border-color: var(--neon-cyan); }

  .check-label { display: flex; align-items: center; gap: 0.5rem; font-size: 0.82rem; cursor: pointer; }
  .check-label input:disabled { cursor: default; }

  /* 同步面板 */
  .plan-list { display: flex; flex-direction: column; gap: 0.7rem; overflow-y: auto; }
  .select-all-plans {
    padding-bottom: 0.45rem; border-bottom: 1px solid rgba(255,255,255,0.08);
    color: var(--neon-cyan); font-weight: 600;
  }
  .selectable-count { color: var(--text-muted); font-weight: 400; font-size: 0.75rem; }
  .plan-item {
    border: 1px solid rgba(255,255,255,0.08); border-radius: 0.5rem;
    padding: 0.7rem 0.9rem; display: flex; flex-direction: column; gap: 0.4rem;
  }
  .plan-item.muted { opacity: 0.55; }
  .plan-head { display: flex; align-items: center; gap: 0.6rem; flex-wrap: wrap; }
  .agent-name { font-weight: 600; font-size: 0.9rem; }
  .config-path {
    font-size: 0.68rem; color: var(--text-muted);
    white-space: nowrap; overflow: hidden; text-overflow: ellipsis; display: block;
  }
  .tag { font-size: 0.65rem; padding: 0.1rem 0.5rem; border-radius: 999px; white-space: nowrap; }
  .tag.gray { background: rgba(148,163,184,0.15); color: #cbd5e1; }
  .tag.red { background: rgba(248,113,113,0.15); color: #f87171; }
  .tag.green { background: rgba(74,222,128,0.15); color: #4ade80; }

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

  .result-icon { font-weight: 700; }
  .result-icon.ok { color: #4ade80; }
  .result-icon.fail { color: #f87171; }
</style>
