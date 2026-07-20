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
  import { MCP_CATALOG, type McpCatalogEntry } from '$lib/data/mcpCatalog';
  import AgentLogo from '$lib/components/AgentLogo.svelte';

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

  // ---------- 添加 / 编辑内联面板(全局无弹窗:添加=列表顶部整行,编辑=所点卡片行尾整行插入) ----------
  let editorOpen = $state(false);
  let editingName = $state<string | null>(null); // null = 新增

  // 网格实际列数(auto-fill 随宽度变),浏览器解析后的真值 + ResizeObserver 跟踪;
  // 编辑面板插入位置(行尾)依赖它。
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
    if (!editorOpen || editingName === null) return -1;
    const idx = serverNames.indexOf(editingName);
    if (idx < 0) return -1;
    return Math.min(Math.floor(idx / gridCols) * gridCols + gridCols - 1, serverNames.length - 1);
  });
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
    catalogHint = [];
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
    catalogHint = [];
    editorOpen = true;
  }

  // ---------- 精选目录:点卡片 = 切表单模式并预填,用户可改后保存 ----------
  let catalogHint = $state<string[]>([]); // 当前预填条目需要用户自填的键名(提示行)

  function pickCatalog(c: McpCatalogEntry) {
    editorMode = 'form';
    formError = '';
    formName = c.id;
    formKind = c.kind;
    formCommand = c.command ?? '';
    formArgs = (c.args ?? []).join('\n');
    formUrl = c.url ?? '';
    formEnabled = true;
    // envHint:stdio 预填 env 键名,http 预填 header 键名;值留空由用户填
    if (c.kind === 'stdio') {
      formEnv = (c.envHint ?? []).map((key) => ({ key, value: '' }));
      formHeaders = [];
    } else {
      formHeaders = (c.envHint ?? []).map((key) => ({ key, value: '' }));
      formEnv = [];
    }
    jsonText = '';
    catalogHint = c.envHint ?? [];
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
        quickSync = syncStage === 'closed'; // 保存成功 → 去同步快捷入口
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
      quickSync = syncStage === 'closed'; // 保存成功 → 去同步快捷入口
    } catch (e) {
      formError = String(e);
    } finally {
      saving = false;
    }
  }

  // ---------- 同步到 Agent(标签式:行状态 + 单行同步 + 批量,结果就地写回) ----------
  type SyncStage = 'closed' | 'planning' | 'preview';
  let syncStage = $state<SyncStage>('closed');
  let quickSync = $state(false); // 「配置有变更待同步」快捷入口条
  let plans = $state<AgentPlan[]>([]);
  let syncError = $state(''); // plan / 批量 invoke 整体失败
  let expanded = $state<Record<string, boolean>>({}); // 行内明细展开/收起
  let checked = $state<Record<string, boolean>>({}); // 批量应用勾选
  let batchApplying = $state(false);
  let rowApplying = $state<Record<string, boolean>>({});
  let rowError = $state<Record<string, string>>({});
  let rowSynced = $state<Record<string, boolean>>({}); // apply 成功后就地翻「已同步」
  let rowBackup = $state<Record<string, string>>({});

  /** 实际变更项(add/update/remove) */
  function realChanges(p: AgentPlan): ChangeItem[] {
    return p.changes.filter((c) => c.action === 'add' || c.action === 'update' || c.action === 'remove');
  }

  function skipItems(p: AgentPlan): ChangeItem[] {
    return p.changes.filter((c) => c.action === 'skip');
  }

  /** 行状态:apply 成功后 rowSynced 就地覆盖(其它行 plan 数据不刷新,面板重开才重新 plan) */
  type RowStatus = 'synced' | 'pending' | 'skipped' | 'unsupported' | 'error';

  function rowStatus(p: AgentPlan): RowStatus {
    if (!p.supported) return 'unsupported';
    if (p.error) return 'error';
    if (rowSynced[p.agent_id]) return 'synced';
    if (realChanges(p).length > 0) return 'pending';
    if (skipItems(p).length > 0) return 'skipped';
    return 'synced'; // 全部 unchanged 或无变更项
  }

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
      plans = await sync_mcp_plan();
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
      rowError = { ...rowError, [r.agent_id]: r.error ?? $_('errors.applyFailed') };
    }
  }

  async function applyOne(id: string) {
    if (rowApplying[id] || batchApplying) return;
    rowApplying = { ...rowApplying, [id]: true };
    rowError = { ...rowError, [id]: '' };
    try {
      const results = await sync_mcp_apply([id]);
      if (results[0]) recordResult(results[0]);
    } catch (e) {
      rowError = { ...rowError, [id]: String(e) };
    } finally {
      rowApplying = { ...rowApplying, [id]: false };
    }
  }

  async function applyChecked() {
    const ids = plans.filter((p) => selectable(p) && checked[p.agent_id]).map((p) => p.agent_id);
    if (ids.length === 0 || batchApplying) return;
    batchApplying = true;
    syncError = '';
    rowApplying = { ...rowApplying, ...Object.fromEntries(ids.map((id) => [id, true])) };
    try {
      const results = await sync_mcp_apply(ids);
      for (const r of results) recordResult(r);
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
  }

  function unchangedCount(p: AgentPlan): number {
    return p.changes.filter((c) => c.action === 'unchanged').length;
  }
</script>

<svelte:window
  onkeydown={(e) => {
    if (e.key !== 'Escape') return;
    if (editorOpen) closeEditor();
    else if (syncStage === 'preview' && !batchApplying) closeSync();
  }}
/>

<!-- 添加 / 编辑内联表单(添加时在列表顶部展开,编辑时在对应行下方展开) -->
{#snippet editorPanel()}
  <div class="editor-panel glass-card">
    <h3>{editingName === null ? $_('mcp.addServer') : $_('mcp.editServer')}</h3>

    <!-- 精选目录:仅添加模式;点卡片一键预填,已配置同名的置灰 -->
    {#if editingName === null}
      <div class="featured-row">
        {#each MCP_CATALOG as c (c.id)}
          {@const added = !!servers[c.id]}
          <button
            type="button"
            class="source-card"
            class:sel={!added && formName === c.id}
            disabled={added}
            title={c.docsUrl}
            onclick={() => pickCatalog(c)}
          >
            <span class="source-name">
              {c.name}
              {#if added}<span class="added-badge">{$_('mcp.form.catalogAdded')}</span>{/if}
            </span>
            <span class="source-desc">{c.description}</span>
          </button>
        {/each}
      </div>
      {#if catalogHint.length > 0}
        <p class="catalog-hint">{$_('mcp.form.catalogEnvHint', { values: { keys: catalogHint.join(', ') } })}</p>
      {/if}
    {/if}

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

  <!-- 去同步快捷入口(新增/编辑保存成功后就地出现;面板打开时不显示) -->
  {#if quickSync && syncStage === 'closed'}
    <div class="quick-sync-bar">
      <span class="qs-hint">{$_('quickSync.hint')}</span>
      <button class="qs-action" onclick={() => { quickSync = false; startSync(); }}>
        {$_('quickSync.action')}
      </button>
      <button class="qs-close" onclick={() => (quickSync = false)} aria-label={$_('quickSync.dismiss')}>✕</button>
    </div>
  {/if}

  <!-- 同步内联面板(按钮下方整行展开;逐行状态 + 单行同步 + 批量,结果就地写回) -->
  {#if syncStage !== 'closed'}
    <div class="sync-panel glass-card">
      {#if syncStage === 'planning'}
        <div class="loading"><span class="spinner"></span> {$_('providers.sync.planning')}</div>
      {:else}
        <h3>{$_('providers.sync.previewTitle')}</h3>
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
                        {:else}
                          <span class="tag red">{$_('providers.sync.error')}</span>
                        {/if}
                      </div>
                      {#if p.supported && p.config_path}
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

  <!-- 新增表单:列表顶部展开 -->
  {#if editorOpen && editingName === null}
    {@render editorPanel()}
  {/if}

  <!-- 服务器卡片网格(对齐技能库/服务商卡片语言) -->
  <div class="server-grid" bind:this={gridEl}>
    {#if isLoading}
      <div class="grid-span loading glass-card"><span class="spinner"></span> {$_('mcp.loading')}</div>
    {:else if serverNames.length === 0}
      <div class="grid-span glass-card empty">
        <p>{$_('mcp.empty')}</p>
        {#if !editorOpen}
          <button class="btn primary" onclick={openAdd}>{$_('mcp.addServer')}</button>
        {/if}
      </div>
    {:else}
      {#each serverNames as name, i (name)}
        {@const spec = servers[name]}
        <div class="glass-card server-card" class:disabled={!spec.enabled}>
          <div class="server-title">
            <span class="server-name">{name}</span>
            <span class="kind-badge kind-{spec.kind}">{spec.kind}</span>
          </div>
          <code class="server-summary" title={summaryOf(spec)}>{summaryOf(spec)}</code>
          <div class="server-actions">
            <button
              class="btn toggle"
              class:on={spec.enabled}
              onclick={() => toggleEnabled(name)}
              title={spec.enabled ? $_('mcp.enabled') : $_('mcp.disabled')}
            >
              {spec.enabled ? $_('mcp.enabled') : $_('mcp.disabled')}
            </button>
            <span class="spacer"></span>
            <button class="btn" class:active={editorOpen && editingName === name} onclick={() => openEdit(name)}>{$_('mcp.edit')}</button>
            {#if deleting === name}
              <button class="btn danger" onclick={() => removeServer(name)}>{$_('mcp.confirmDelete')}</button>
              <button class="btn" onclick={() => (deleting = null)}>{$_('mcp.cancel')}</button>
            {:else}
              <button class="btn remove" onclick={() => (deleting = name)}>{$_('mcp.delete')}</button>
            {/if}
          </div>
        </div>

        <!-- 编辑面板:整行插在所点卡片所在行的行尾之后,同行卡片位置不动 -->
        {#if editorOpen && editingName !== null && i === editRowEnd}
          {@render editorPanel()}
        {/if}
      {/each}
    {/if}
  </div>
</div>

<style>
  .mcp-page { padding: 1.5rem; display: flex; flex-direction: column; gap: 1rem; }

  .page-header { display: flex; justify-content: space-between; align-items: center; gap: 1rem; flex-wrap: wrap; }
  .page-header h1 { margin: 0; font-size: 1.25rem; }
  .subtitle { margin: 0.25rem 0 0; color: var(--text-muted); font-size: 0.85rem; }
  .header-actions { display: flex; gap: 0.5rem; }

  /* 服务器卡片网格(对齐技能库/服务商卡片语言) */
  .server-grid {
    display: grid; gap: 0.75rem;
    grid-template-columns: repeat(auto-fill, minmax(260px, 1fr));
  }
  .grid-span { grid-column: 1 / -1; }
  .server-card {
    padding: 0.9rem 1rem; display: flex; flex-direction: column; gap: 0.5rem;
    transition: opacity 0.2s ease;
  }
  .server-card.disabled { opacity: 0.55; }
  .server-title { display: flex; align-items: center; gap: 0.5rem; flex-wrap: wrap; min-width: 0; }
  .server-name { font-weight: 600; font-size: 0.9rem; }
  .kind-badge { font-size: 0.65rem; padding: 0.1rem 0.5rem; border-radius: 999px; white-space: nowrap; }
  .kind-badge.kind-stdio { background: rgba(94, 234, 212, 0.15); color: #5eead4; }
  .kind-badge.kind-http { background: rgba(123, 97, 255, 0.15); color: #a99bff; }
  .server-summary {
    font-size: 0.72rem; color: var(--text-secondary);
    white-space: nowrap; overflow: hidden; text-overflow: ellipsis; display: block; max-width: 100%;
  }
  .server-actions { display: flex; align-items: center; gap: 0.4rem; margin-top: auto; flex-wrap: wrap; }
  .server-actions .spacer { flex: 1; }

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

  /* 去同步快捷入口条(全站统一样式) */
  .quick-sync-bar {
    display: flex; align-items: center; gap: 0.6rem;
    padding: 0.45rem 0.8rem; border-radius: 0.5rem;
    background: rgba(0,245,255,0.08); border: 1px solid rgba(0,245,255,0.3);
  }
  .qs-hint { flex: 1; font-size: 0.78rem; color: var(--neon-cyan); }
  .qs-action {
    padding: 0.25rem 0.7rem; border-radius: 0.4rem; font-size: 0.75rem; cursor: pointer;
    background: rgba(0,245,255,0.14); border: 1px solid var(--neon-cyan); color: var(--neon-cyan);
  }
  .qs-close {
    background: transparent; border: none; cursor: pointer; color: var(--text-muted);
    font-size: 0.75rem; padding: 0.2rem 0.3rem; line-height: 1;
  }
  .qs-close:hover { color: var(--text-primary); }

  /* 内联面板(编辑器 / 同步预览):页面内推开内容,无遮罩;编辑器在网格内占满整行 */
  .editor-panel, .sync-panel {
    padding: 1.3rem 1.5rem; display: flex; flex-direction: column; gap: 0.9rem;
    background: var(--bg-secondary);
    border-color: rgba(0,245,255,0.35);
  }
  .editor-panel { grid-column: 1 / -1; }
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

  /* 精选目录卡片行(同技能安装区精选源样式) */
  .featured-row { display: flex; gap: 0.5rem; flex-wrap: wrap; }
  .source-card {
    flex: 1; min-width: 170px; max-width: 300px; text-align: left; cursor: pointer;
    display: flex; flex-direction: column; gap: 0.2rem;
    background: var(--bg-tertiary); border: 1px solid rgba(255,255,255,0.1);
    border-radius: 0.5rem; padding: 0.55rem 0.7rem; color: var(--text-primary);
  }
  .source-card:hover { border-color: rgba(0,245,255,0.4); }
  .source-card.sel { border-color: var(--neon-cyan); background: rgba(0,245,255,0.06); }
  .source-card:disabled { opacity: 0.5; cursor: default; }
  .source-card:disabled:hover { border-color: rgba(255,255,255,0.1); }
  .source-name { font-weight: 600; font-size: 0.8rem; display: flex; align-items: center; gap: 0.4rem; }
  .source-desc { font-size: 0.7rem; color: var(--text-muted); }
  .added-badge {
    font-size: 0.62rem; padding: 0.05rem 0.45rem; border-radius: 999px; white-space: nowrap;
    background: rgba(74,222,128,0.15); color: #4ade80; font-weight: 400;
  }
  .catalog-hint { margin: 0; font-size: 0.75rem; color: #fbbf24; }

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

  /* 同步面板(标签式,与服务商/技能/记忆面板同款) */
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
    white-space: nowrap; overflow: hidden; text-overflow: ellipsis; display: block; max-width: 100%;
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
