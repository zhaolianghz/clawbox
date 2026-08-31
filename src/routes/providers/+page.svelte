<script lang="ts">
  import { onMount } from 'svelte';
  import { _, locale } from 'svelte-i18n';
  import {
    PROVIDER_CATALOG, PROVIDER_CATEGORIES,
    type ProviderCatalogEntry, type ProviderCategory, type FreeTier,
  } from '$lib/data/providers';
  import { localize } from '$lib/data/localized';
  import ProviderLogo from '$lib/components/ProviderLogo.svelte';
  import { providers, addProvider, updateProvider, deleteProvider, loadProviders } from '$lib/stores/config';
  import { provider_test, type ModelProvider, type ProviderFlavor, type ProviderTestResult, DEFAULT_PROVIDER_ID } from '$lib/api/config';
  import { usageProviderSummary, type ProviderUsage } from '$lib/api/usage';
  import { agent_providers_get, type ApplyResult } from '$lib/api/providerSync';
  import { agents_list } from '$lib/api/agents';
  import { open } from '@tauri-apps/plugin-dialog';
  import { save } from '@tauri-apps/plugin-dialog';
  import { cc_switch_import_preview, type ImportCandidate } from '$lib/api/ccSwitch';
  import { transfer_export, transfer_import_preview, transfer_import_apply, type TransferPreview, type TransferItem, type TransferOutcome } from '$lib/api/transfer';
  import { config_mcp_list } from '$lib/api/mcpSync';
  import { skills_library_list } from '$lib/api/skillsSync';

  let query = $state('');
  let activeCategory = $state<ProviderCategory | 'all'>('all');
  /** 「免费额度」筛选。与分类正交(免费额度横跨 intl/cn/aggregator),所以是独立开关而非一个分类。 */
  let freeOnly = $state(false);
  let pageError = $state('');

  // 编辑保存后的自动重推结果提示(成功 N 家 / 失败列出)
  let repushNote = $state('');
  let repushFailed = $state<ApplyResult[]>([]);

  function reportRepush(results: ApplyResult[]) {
    repushFailed = results.filter((r) => !r.ok);
    const okCount = results.length - repushFailed.length;
    repushNote = okCount > 0 ? $_('providers.repushOk', { values: { count: okCount } }) : '';
    if (repushNote) setTimeout(() => (repushNote = ''), 5000);
  }

  // agent_id → provider_id 绑定表 → 反查每家服务商被哪些 agent 使用
  let agentBindings = $state<Record<string, string>>({});
  let providerUsages = $state<ProviderUsage[] | null>(null);
  const usageByProvider = $derived.by(() => {
    const m: Record<string, string[]> = {};
    for (const [agentId, pid] of Object.entries(agentBindings)) (m[pid] ??= []).push(agentId);
    return m;
  });

  // 已配置的服务商:任一端点槽的 host 命中目录条目即视为匹配
  const configuredByHost = $derived.by(() => {
    const map = new Map<string, ModelProvider[]>();
    for (const p of $providers) {
      if (p.anthropicBaseUrl) map.set(hostOf(p.anthropicBaseUrl), [...(map.get(hostOf(p.anthropicBaseUrl)) ?? []), p]);
      if (p.openaiBaseUrl) map.set(hostOf(p.openaiBaseUrl), [...(map.get(hostOf(p.openaiBaseUrl)) ?? []), p]);
    }
    return map;
  });

  function hostOf(url: string): string {
    try { return new URL(url).host; } catch { return url; }
  }

  function configuredEntries(e: ProviderCatalogEntry): ModelProvider[] {
    const byApi = configuredByHost.get(hostOf(e.apiHost)) ?? [];
    const byAnthropic = e.anthropicHost ? (configuredByHost.get(hostOf(e.anthropicHost)) ?? []) : [];
    return [...new Map([...byApi, ...byAnthropic].map((p) => [p.id, p])).values()];
  }

  function configuredEntry(e: ProviderCatalogEntry): ModelProvider | undefined {
    return configuredEntries(e)[0];
  }

  // ---------- 自定义服务商(目录里没有的存储条目)----------
  // 特殊 id:网格末尾那张「+ 自定义服务商」新增卡;也作为新增时内联面板的锚点行。
  const CUSTOM_NEW_ID = '__custom_new__';

  /** 目录里所有端点 host 的集合(apiHost + anthropicHost) */
  const catalogHostSet = $derived.by(() => {
    const s = new Set<string>();
    for (const e of PROVIDER_CATALOG) {
      if (e.apiHost) s.add(hostOf(e.apiHost));
      if (e.anthropicHost) s.add(hostOf(e.anthropicHost));
    }
    return s;
  });

  /** 把一条存储 provider 合成为目录条目形状,好让它进同一个网格渲染 */
  function syntheticEntry(p: ModelProvider): ProviderCatalogEntry {
    return {
      id: p.id,
      name: p.name || '(未命名)',
      apiHost: p.openaiBaseUrl || p.anthropicBaseUrl || '',
      category: 'custom',
      color: '#7c5cff',
      defaultModel: p.defaultModel || undefined,
      anthropicHost: p.anthropicBaseUrl || undefined,
    };
  }

  /** 每个目录 host 的首条配置才有资格合并进目录卡;同一端点的后续条目
   * (如 "zhipu glm" 与 "智普" 都指 open.bigmodel.cn)保留为自定义卡,
   * 否则会被目录卡整条吞掉:不渲染、搜不到、自定义分类里也消失 */
  const catalogHostFirstOwner = $derived.by(() => {
    const first = new Map<string, string>();
    for (const p of $providers) {
      for (const url of [p.anthropicBaseUrl, p.openaiBaseUrl]) {
        if (!url) continue;
        const h = hostOf(url);
        if (catalogHostSet.has(h) && !first.has(h)) first.set(h, p.id);
      }
    }
    return first;
  });

  /** 存储里没被目录卡吸收的 provider → 自定义条目(端点不在目录,或同端点第 2+ 条) */
  const customEntries = $derived(
    $providers
      .filter((p) => {
        const hosts = [p.anthropicBaseUrl, p.openaiBaseUrl].filter(Boolean).map(hostOf);
        if (hosts.length === 0) return false;
        return !hosts.some((h) => catalogHostFirstOwner.get(h) === p.id);
      })
      .map(syntheticEntry)
  );

  // 自定义卡片已经通过 provider id 唯一对应配置，不能再次按 Host 反查。
  const configuredForEntry = (e: ProviderCatalogEntry): ModelProvider | undefined => {
    const direct = $providers.find((p) => p.id === e.id);
    return direct ?? configuredEntry(e);
  };

  const filtered = $derived.by(() => {
    const q = query.trim().toLowerCase();
    const list = [...PROVIDER_CATALOG, ...customEntries].filter((e) => {
      // 搜索时无视分类/免费筛选,跨全目录检索——否则停在「自定义」tab 搜索会把
      // 目录条目全部藏掉,看起来像"服务商只剩几个"(2026-08-28 智谱搜索事故)
      const matchCat = !!q || activeCategory === 'all' || e.category === activeCategory;
      const matchFree = !!q || !freeOnly || !!e.freeTier;
      const name = localize(e.name, $locale).toLowerCase();
      const desc = e.description ? localize(e.description, $locale).toLowerCase() : '';
      // id 也参与匹配:中文名搜不到时,zhipu/glm 这类英文标识仍可命中
      // 分词 AND:每个空格分隔的词都命中才算匹配("zhipu GLM" → zhipu 命中 id、
      // GLM 命中描述,而不是要求单个字段包含整串)。已合并进目录卡的配置名
      // (如用户自定义叫 "zhipu glm")也参与检索。
      const configuredNames = ([anthropicHostOf(e), openaiHostOf(e)] as (string | null)[])
        .filter((h): h is string => !!h)
        .flatMap((h) => configuredByHost.get(h) ?? [])
        .map((cp) => cp.name.toLowerCase());
      const haystack = [name, e.id, e.apiHost, desc, ...(e.keywords ?? []), ...configuredNames].join(' ').toLowerCase();
      const matchQ = !q || q.split(/\s+/).every((t) => haystack.includes(t));
      return matchCat && matchFree && matchQ;
    });
    // 「+ 自定义服务商」新增卡:仅在浏览 全部/自定义 且未搜索、未筛免费时出现在末尾
    if ((activeCategory === 'all' || activeCategory === 'custom') && !q && !freeOnly) {
      list.push({ id: CUSTOM_NEW_ID, name: '', apiHost: '', category: 'custom', color: '#7c5cff' });
    }
    return list;
  });

  const freeCount = $derived(PROVIDER_CATALOG.filter((e) => e.freeTier).length);

  /**
   * 免费额度信息超过 6 个月没人工核对 → 视为可能过期。
   * 各家的限额/活动变得很快,宁可提示用户去官网确认,也不要让 ClawBox 背书一条陈旧数据。
   */
  const STALE_MONTHS = 6;
  function isStale(verifiedAt: string): boolean {
    const [y, m] = verifiedAt.split('-').map(Number);
    if (!y || !m) return false;
    const now = new Date();
    return (now.getFullYear() - y) * 12 + (now.getMonth() + 1 - m) > STALE_MONTHS;
  }

  /** 免费徽章的悬停文案:额度说明 + 免绑卡 + 核对时间(过期则改为提醒) */
  function freeTitle(f: FreeTier): string {
    const parts = [localize(f.note, $locale)];
    if (f.noCard) parts.push($_('providers.free.noCard'));
    parts.push(
      isStale(f.verifiedAt)
        ? $_('providers.free.stale', { values: { date: f.verifiedAt } })
        : $_('providers.free.verified', { values: { date: f.verifiedAt } })
    );
    return parts.join(' · ');
  }

  function categoryLabel(c: ProviderCategory): string {
    const label = PROVIDER_CATEGORIES.find((x) => x.id === c)?.label;
    return label ? localize(label, $locale) : c;
  }

  async function toggleEnabled(p: ModelProvider) {
    pageError = '';
    try {
      // 禁用已绑定的服务商会重推失败 → 正好在失败列表里提示
      reportRepush(await updateProvider(p.id, { enabled: !p.enabled }));
    } catch (e) {
      pageError = String(e);
    }
  }

  let removingId = $state<string | null>(null); // 两步删除确认:当前展开确认的服务商 id

  async function removeProvider(id: string) {
    removingId = null;
    pageError = '';
    try {
      await deleteProvider(id); // 返回值忽略:删除不重推,后端自动解绑
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
        fetchError = r.error ?? $_('errors.requestFailed');
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
    formName = localize(e.name, $locale);
    formAnthropicUrl = anthropicHostOf(e) ?? '';
    formOpenaiUrl = openaiHostOf(e) ?? '';
    formApiKey = '';
    formDefaultModel = e.defaultModel ?? '';
    formModels = [];
    formEnabled = true;
    resetTransientState();
    editorOpen = true;
  }

  /** 新增一个目录里没有的自定义服务商:空白表单,内联面板锚在网格末尾的新增卡后 */
  function openAddCustom() {
    if (editorOpen && editingEntry?.id === CUSTOM_NEW_ID) {
      closeEditor();
      return;
    }
    activeCategory = 'custom'; // 确保新增卡在 filtered 里,面板有锚点
    query = '';
    editingId = null;
    editingEntry = { id: CUSTOM_NEW_ID, name: { en: 'Custom Provider', zh: '自定义服务商' }, apiHost: '', category: 'custom', color: '#7c5cff' };
    formName = '';
    formAnthropicUrl = '';
    formOpenaiUrl = '';
    formApiKey = '';
    formDefaultModel = '';
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
    // 名字一律尊重用户输入:不再因端点改动自动加「自定义-」前缀
    const finalName = name;
    const data = {
      name: finalName,
      anthropicBaseUrl,
      openaiBaseUrl,
      apiKey: formApiKey.trim(),
      defaultModel: formDefaultModel.trim(),
      models: formModels,
      enabled: formEnabled,
    };
    saving = true;
    try {
      let repushed: ApplyResult[] = [];
      if (editingId === null) {
        repushed = await addProvider({ id: crypto.randomUUID(), ...data });
      } else {
        repushed = await updateProvider(editingId, data);
      }
      closeEditor();
      reportRepush(repushed);
    } catch (e) {
      formError = String(e);
    } finally {
      saving = false;
    }
  }

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
    gemini: 'Gemini CLI',
    cline: 'Cline',
    pi: 'Pi',
    'qwen-code': 'Qwen Code',
    'trae-agent': 'Trae Agent',
  };
  let agentLabels = $state<Record<string, string>>({});

  function agentLabel(id: string): string {
    return agentLabels[id] ?? FALLBACK_LABELS[id] ?? id;
  }

  // ---------- 配置导入/导出(issue #2:.clawbox.json 熟人分享) ----------
  type ExportStage = 'closed' | 'open';
  let exportStage = $state<ExportStage>('closed');
  let expChecked = $state<Record<string, boolean>>({}); // provider id → 勾选
  let expIncludeKeys = $state(true);
  let expIncludeMcp = $state(true);
  let expIncludeSkills = $state(true);
  let expMcpCount = $state(0);
  let expSkillNames = $state<string[]>([]);
  let exporting = $state(false);
  let expError = $state('');
  let expDone = $state(''); // 成功导出的文件路径

  const expPickedIds = $derived($providers.filter((p) => expChecked[p.id]).map((p) => p.id));

  async function openExport() {
    if (exportStage === 'open') {
      exportStage = 'closed';
      return;
    }
    expError = '';
    expDone = '';
    expChecked = Object.fromEntries($providers.map((p) => [p.id, true]));
    expIncludeKeys = true;
    expIncludeMcp = true;
    expIncludeSkills = true;
    exportStage = 'open';
    // 计数仅供展示;取不到就按 0(该 section 复选框自然置灰)
    try {
      expMcpCount = Object.keys(await config_mcp_list()).length;
    } catch {
      expMcpCount = 0;
    }
    try {
      expSkillNames = (await skills_library_list()).filter((s) => s.source).map((s) => s.name);
    } catch {
      expSkillNames = [];
    }
  }

  async function doExport() {
    if (exporting || expPickedIds.length === 0) return;
    exporting = true;
    expError = '';
    expDone = '';
    try {
      const path = await save({
        defaultPath: 'my-config.clawbox.json',
        filters: [{ name: 'ClawBox', extensions: ['json'] }],
      });
      if (!path) return; // 用户取消
      await transfer_export(
        path,
        expPickedIds,
        expIncludeKeys,
        expIncludeMcp && expMcpCount > 0,
        expIncludeSkills ? expSkillNames : []
      );
      expDone = path;
    } catch (e) {
      expError = String(e);
    } finally {
      exporting = false;
    }
  }

  // 导入:选文件 → 预览(add/merge/overwrite/skip 逐条勾选) → 应用
  type TransferStage = 'closed' | 'loading' | 'preview';
  let tStage = $state<TransferStage>('closed');
  let tPath = $state('');
  let tPreview = $state<TransferPreview | null>(null);
  let tChecked = $state<Record<string, boolean>>({}); // "p:名" / "m:名" / "s:名"
  let tApplying = $state(false);
  let tError = $state('');
  let tOutcome = $state<TransferOutcome | null>(null);

  async function startTransferImport() {
    if (tStage !== 'closed') {
      tStage = 'closed';
      return;
    }
    tError = '';
    tOutcome = null;
    tPreview = null;
    let picked: string | string[] | null;
    try {
      picked = await open({ multiple: false, filters: [{ name: 'ClawBox', extensions: ['json'] }] });
    } catch (e) {
      tError = String(e);
      tStage = 'preview';
      return;
    }
    if (typeof picked !== 'string' || !picked) return; // 用户取消
    tPath = picked;
    tStage = 'loading';
    try {
      tPreview = await transfer_import_preview(picked);
      const init: Record<string, boolean> = {};
      for (const it of tPreview.providers) init[`p:${it.name}`] = it.action !== 'skip';
      for (const it of tPreview.mcp) init[`m:${it.name}`] = it.action !== 'skip';
      for (const it of tPreview.skills) init[`s:${it.name}`] = it.action !== 'skip';
      tChecked = init;
      tStage = 'preview';
    } catch (e) {
      tError = String(e);
      tStage = 'preview';
    }
  }

  const tPickCount = $derived.by(() => {
    if (!tPreview) return 0;
    const live = (sec: 'p' | 'm' | 's', items: TransferItem[]) =>
      items.filter((i) => i.action !== 'skip' && tChecked[`${sec}:${i.name}`]).length;
    return live('p', tPreview.providers) + live('m', tPreview.mcp) + live('s', tPreview.skills);
  });

  async function applyTransferImport() {
    if (!tPreview || tApplying || tPickCount === 0) return;
    tApplying = true;
    tError = '';
    try {
      const pick = (sec: 'p' | 'm' | 's', items: TransferItem[]) =>
        items.filter((i) => i.action !== 'skip' && tChecked[`${sec}:${i.name}`]).map((i) => i.name);
      tOutcome = await transfer_import_apply(tPath, {
        providers: pick('p', tPreview.providers),
        mcp: pick('m', tPreview.mcp),
        skills: pick('s', tPreview.skills),
      });
      await loadProviders();
    } catch (e) {
      tError = String(e);
    } finally {
      tApplying = false;
    }
  }

  // ---------- 从 cc-switch 导入(内联预览面板,复用 sync-panel 视觉) ----------
  type ImportStage = 'closed' | 'loading' | 'preview';
  let importStage = $state<ImportStage>('closed');
  let importCandidates = $state<ImportCandidate[]>([]);
  let importChecked = $state<Record<number, boolean>>({}); // 候选索引 → 勾选
  let importError = $state('');
  let importing = $state(false);

  const importCheckedCount = $derived(importCandidates.filter((_, i) => importChecked[i]).length);
  const importAllPicked = $derived(
    importCandidates.length > 0 && importCandidates.every((_, i) => importChecked[i])
  );

  /** 候选按 host 命中的现有 provider(有 = 合并到它,无 = 新增) */
  function importTarget(c: ImportCandidate): ModelProvider | undefined {
    const hosts = [c.anthropicBaseUrl, c.openaiBaseUrl].filter(Boolean).map(hostOf);
    return $providers.find((p) =>
      [p.anthropicBaseUrl, p.openaiBaseUrl].filter(Boolean).map(hostOf).some((h) => hosts.includes(h))
    );
  }

  function maskKey(k: string): string {
    if (!k) return '';
    return k.length <= 8 ? '••••' : `${k.slice(0, 4)}••••${k.slice(-4)}`;
  }

  /** 打开导入:先探测 config.json,未找到则弹文件选择器选导出的 JSON */
  async function startImport() {
    if (importStage !== 'closed') return;
    importError = '';
    importStage = 'loading';
    try {
      let preview = await cc_switch_import_preview();
      if (preview.kind === 'needFile') {
        let picked: string | string[] | null;
        try {
          picked = await open({ multiple: false, filters: [{ name: 'cc-switch', extensions: ['db', 'json'] }] });
        } catch (e) {
          importError = String(e);
          importStage = 'closed';
          return;
        }
        if (typeof picked !== 'string' || !picked) {
          importStage = 'closed'; // 用户取消
          return;
        }
        preview = await cc_switch_import_preview(picked);
      }
      if (preview.kind === 'found') {
        importCandidates = preview.candidates;
        importChecked = Object.fromEntries(importCandidates.map((_, i) => [i, true]));
      } else {
        importCandidates = [];
      }
      importStage = 'preview';
    } catch (e) {
      importError = String(e);
      importCandidates = [];
      importStage = 'preview';
    }
  }

  function toggleAllImport() {
    const v = !importAllPicked;
    importChecked = Object.fromEntries(importCandidates.map((_, i) => [i, v]));
  }

  /** 应用勾选:命中已有则只填空槽/补空 key(不覆盖),否则新增。逐条走现有 store 动作。 */
  async function applyImport() {
    const picked = importCandidates.filter((_, i) => importChecked[i]);
    if (picked.length === 0 || importing) return;
    importing = true;
    importError = '';
    try {
      const repushed: ApplyResult[] = [];
      for (const c of picked) {
        const existing = importTarget(c);
        if (existing) {
          const data: Partial<ModelProvider> = {};
          if (c.anthropicBaseUrl && !existing.anthropicBaseUrl) data.anthropicBaseUrl = c.anthropicBaseUrl;
          if (c.openaiBaseUrl && !existing.openaiBaseUrl) data.openaiBaseUrl = c.openaiBaseUrl;
          if (c.apiKey && !existing.apiKey) data.apiKey = c.apiKey;
          if (c.defaultModel && !existing.defaultModel) data.defaultModel = c.defaultModel;
          if (Object.keys(data).length > 0) repushed.push(...(await updateProvider(existing.id, data)));
        } else {
          repushed.push(...(await addProvider({
            id: crypto.randomUUID(),
            name: c.name || 'Imported',
            anthropicBaseUrl: c.anthropicBaseUrl,
            openaiBaseUrl: c.openaiBaseUrl,
            apiKey: c.apiKey,
            defaultModel: c.defaultModel,
            models: [],
            enabled: true,
          })));
        }
      }
      importStage = 'closed';
      importCandidates = [];
      reportRepush(repushed);
    } catch (e) {
      importError = String(e);
    } finally {
      importing = false;
    }
  }

  function closeImport() {
    importStage = 'closed';
    importCandidates = [];
    importError = '';
  }

  onMount(async () => {
    try {
      await loadProviders();
    } catch (e) {
      pageError = String(e);
    }
    try {
      const all = await agents_list();
      agentLabels = Object.fromEntries(all.map((a) => [a.id, a.label]));
    } catch { /* 回退本地映射即可 */ }
    // 绑定表反查「使用中」徽章;绑定在 Agents 页变更后本页重新挂载时自然刷新
    agentBindings = await agent_providers_get().catch(() => ({}));
    // 用量:按 provider 名聚合的近 N 天消耗,失败静默(用户没扫过本地日志 = 没数据)
    usageProviderSummary()
      .then((r) => (providerUsages = r))
      .catch(() => {
        /* 没数据正常 */
      });
  });

  // 按 provider 名查近 30 天用量,无数据返回 null(卡片里不渲染该行)
  function providerUsageByName(name: string): ProviderUsage | null {
    if (!providerUsages) return null;
    return providerUsages.find((u) => u.provider_name === name) ?? null;
  }
  function fmtTokens(n: number): string {
    if (n >= 1_000_000) return (n / 1_000_000).toFixed(1) + 'M';
    if (n >= 1_000) return (n / 1_000).toFixed(1) + 'k';
    return String(n);
  }
</script>

<svelte:window
  onkeydown={(e) => {
    if (e.key !== 'Escape') return;
    if (editorOpen) closeEditor();
    else if (importStage === 'preview' && !importing) closeImport();
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
        class="btn"
        onclick={openAddCustom}
        class:active={editorOpen && editingEntry?.id === CUSTOM_NEW_ID}
      >
        {$_('providers.addCustom')}
      </button>
      <button
        class="btn"
        onclick={startImport}
        disabled={importStage !== 'closed'}
      >
        {$_('providers.import.button')}
      </button>
      <button class="btn" onclick={startTransferImport} class:active={tStage !== 'closed'}>
        {$_('providers.transfer.importBtn')}
      </button>
      <button class="btn" onclick={openExport} class:active={exportStage === 'open'}>
        {$_('providers.transfer.exportBtn')}
      </button>
    </div>
  </header>

  {#if pageError}
    <pre class="error-text">{pageError}</pre>
  {/if}

  <!-- 导出面板:勾选服务商 + 三开关,保存为 .clawbox.json -->
  {#if exportStage === 'open'}
    <div class="sync-panel glass-card">
      <h3>{$_('providers.transfer.exportTitle')}</h3>
      <div class="transfer-picks">
        {#each $providers.filter((p) => p.id !== DEFAULT_PROVIDER_ID) as p (p.id)}
          <label class="check-label">
            <input type="checkbox" bind:checked={expChecked[p.id]} />
            <span>{p.name}</span>
          </label>
        {/each}
      </div>
      <div class="transfer-opts">
        <label class="check-label">
          <input type="checkbox" bind:checked={expIncludeKeys} />
          <span>{$_('providers.transfer.includeKeys')}</span>
        </label>
        <label class="check-label">
          <input type="checkbox" bind:checked={expIncludeMcp} disabled={expMcpCount === 0} />
          <span>{$_('providers.transfer.includeMcp', { values: { count: expMcpCount } })}</span>
        </label>
        <label class="check-label">
          <input type="checkbox" bind:checked={expIncludeSkills} disabled={expSkillNames.length === 0} />
          <span>{$_('providers.transfer.includeSkills', { values: { count: expSkillNames.length } })}</span>
        </label>
      </div>
      {#if expIncludeKeys}
        <p class="keys-warning">{$_('providers.transfer.keysWarning')}</p>
      {/if}
      {#if expError}<pre class="error-text">{expError}</pre>{/if}
      {#if expDone}<p class="export-done">{$_('providers.transfer.exported', { values: { path: expDone } })}</p>{/if}
      <div class="panel-actions">
        <button class="btn" onclick={() => (exportStage = 'closed')}>{$_('providers.close')}</button>
        <button class="btn primary" onclick={doExport} disabled={exporting || expPickedIds.length === 0}>
          {#if exporting}<span class="spinner small"></span>{/if}
          {$_('providers.transfer.confirmExport', { values: { count: expPickedIds.length } })}
        </button>
      </div>
    </div>
  {/if}

  <!-- 导入预览面板:三组逐条勾选(skip 项置灰) -->
  {#if tStage !== 'closed'}
    <div class="sync-panel glass-card">
      {#if tStage === 'loading'}
        <div class="loading"><span class="spinner"></span></div>
      {:else}
        <h3>{$_('providers.transfer.importTitle')}</h3>
        {#if tError}<pre class="error-text">{tError}</pre>{/if}
        {#if tOutcome}
          <p class="export-done">
            {$_('providers.transfer.importDone', {
              values: {
                pa: tOutcome.providersAdded,
                pm: tOutcome.providersMerged,
                mc: tOutcome.mcpApplied,
                sk: tOutcome.skillsInstalled,
              },
            })}
          </p>
          {#each tOutcome.errors as e (e)}<pre class="error-text">{e}</pre>{/each}
        {:else if tPreview}
          {#each [
            { sec: 'p', title: $_('nav.providers'), items: tPreview.providers },
            { sec: 'm', title: 'MCP', items: tPreview.mcp },
            { sec: 's', title: $_('nav.skills'), items: tPreview.skills },
          ] as group (group.sec)}
            {#if group.items.length > 0}
              <div class="transfer-group">
                <span class="transfer-group-title">{group.title}</span>
                {#each group.items as it (it.name)}
                  <label class="check-label" class:muted={it.action === 'skip'}>
                    <input
                      type="checkbox"
                      disabled={it.action === 'skip' || tApplying}
                      bind:checked={tChecked[`${group.sec}:${it.name}`]}
                    />
                    <span>{it.name}</span>
                    <span class="tag" class:green={it.action === 'add'} class:yellow={it.action === 'merge' || it.action === 'overwrite'} class:gray={it.action === 'skip'}>
                      {$_(`providers.transfer.action.${it.action}`)}
                    </span>
                    {#if it.detail}<span class="transfer-detail">{it.detail}</span>{/if}
                  </label>
                {/each}
              </div>
            {/if}
          {/each}
          {#if tPreview.providers.length + tPreview.mcp.length + tPreview.skills.length === 0}
            <p class="empty-note">{$_('providers.transfer.empty')}</p>
          {/if}
        {/if}
        <div class="panel-actions">
          <button class="btn" onclick={() => (tStage = 'closed')} disabled={tApplying}>{$_('providers.close')}</button>
          {#if !tOutcome && tPreview}
            <button class="btn primary" onclick={applyTransferImport} disabled={tApplying || tPickCount === 0}>
              {#if tApplying}<span class="spinner small"></span>{/if}
              {$_('providers.transfer.applyImport', { values: { count: tPickCount } })}
            </button>
          {/if}
        </div>
      {/if}
    </div>
  {/if}

  <!-- 编辑保存后的自动重推结果提示(成功条 5s 自隐;失败条手动关) -->
  {#if repushNote}
    <div class="quick-sync-bar"><span class="qs-hint">{repushNote}</span></div>
  {/if}
  {#if repushFailed.length > 0}
    <div class="quick-sync-bar">
      <span class="qs-hint">{$_('providers.repushFail', { values: { agents: repushFailed.map((r) => agentLabel(r.agent_id)).join(', ') } })}</span>
      <button class="qs-close" onclick={() => (repushFailed = [])} aria-label={$_('providers.cancel')}>✕</button>
    </div>
  {/if}

  <!-- cc-switch 导入预览面板(整行展开,复用 sync-panel 视觉;无弹窗) -->
  {#if importStage !== 'closed'}
    <div class="sync-panel glass-card">
      {#if importStage === 'loading'}
        <div class="loading"><span class="spinner"></span> {$_('providers.import.loading')}</div>
      {:else}
        <h3>{$_('providers.import.previewTitle')}</h3>
        {#if importError}
          <pre class="error-text">{importError}</pre>
        {/if}
        {#if importCandidates.length === 0}
          <p class="sync-hint">{$_('providers.import.empty')}</p>
        {:else}
          <div class="plan-list">
            <label class="select-all-plans">
              <input
                type="checkbox"
                class="row-check"
                disabled={importing}
                checked={importAllPicked}
                onchange={toggleAllImport}
              />
              <span>{$_('providers.sync.selectAll')}</span>
              <span class="selectable-count">{importCheckedCount}/{importCandidates.length}</span>
            </label>
            {#each importCandidates as c, i (i)}
              {@const target = importTarget(c)}
              <div class="plan-item">
                <div class="plan-row">
                  <input
                    type="checkbox"
                    class="row-check"
                    disabled={importing}
                    checked={!!importChecked[i]}
                    onchange={(e) => (importChecked = { ...importChecked, [i]: e.currentTarget.checked })}
                    aria-label={c.name}
                  />
                  <div class="plan-head">
                    <ProviderLogo entry={{ id: c.name, name: c.name, apiHost: c.openaiBaseUrl || c.anthropicBaseUrl, category: 'aggregator', color: '#7c5cff' }} />
                    <div class="plan-info">
                      <div class="plan-title-line">
                        <span class="agent-name">{c.name}</span>
                        {#if target}
                          <span class="tag amber">{$_('providers.import.badgeMerge', { values: { name: target.name } })}</span>
                        {:else}
                          <span class="tag green">{$_('providers.import.badgeAdd')}</span>
                        {/if}
                        {#if c.anthropicBaseUrl}
                          <span class="endpoint-chip anthropic" title={c.anthropicBaseUrl}>Anthropic</span>
                        {/if}
                        {#if c.openaiBaseUrl}
                          <span class="endpoint-chip openai" title={c.openaiBaseUrl}>OpenAI</span>
                        {/if}
                      </div>
                      <div class="import-meta">
                        {#if c.apiKey}<code class="key-mask">{maskKey(c.apiKey)}</code>{/if}
                        {#if c.defaultModel}<span class="src-apps">{c.defaultModel}</span>{/if}
                        <span class="src-apps">{$_('providers.import.sourceFrom', { values: { apps: c.sourceApps.join('+') } })}</span>
                      </div>
                    </div>
                  </div>
                </div>
              </div>
            {/each}
          </div>
        {/if}
        <div class="panel-actions">
          <button class="btn" onclick={closeImport} disabled={importing}>{$_('providers.close')}</button>
          <button class="btn primary" onclick={applyImport} disabled={importCheckedCount === 0 || importing}>
            {#if importing}<span class="spinner small"></span>{/if}
            {$_('providers.import.confirm', { values: { count: importCheckedCount } })}
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
        {localize(cat.label, $locale)}
      </button>
    {/each}
    <span class="bar-sep"></span>
    <!-- 免费额度:与分类正交的独立开关,可叠加在任意分类上 -->
    <button
      class="chip free-toggle"
      class:active={freeOnly}
      aria-pressed={freeOnly}
      onclick={() => (freeOnly = !freeOnly)}
    >
      {$_('providers.free.filter', { values: { count: freeCount } })}
    </button>
  </div>

  <div class="provider-grid" bind:this={gridEl}>
    {#each filtered as e, i (e.id)}
      {#if e.id === CUSTOM_NEW_ID}
        <!-- 网格末尾:新增自定义服务商卡(点开空白配置面板) -->
        <button
          type="button"
          class="provider-card add-custom-card glass-card"
          class:active={editorOpen && editingEntry?.id === CUSTOM_NEW_ID}
          onclick={openAddCustom}
        >
          <span class="add-plus">+</span>
          <span class="add-label">{$_('providers.addCustom')}</span>
        </button>
      {:else}
      {@const configured = configuredForEntry(e)}
      {@const pUsage = providerUsageByName(localize(e.name, $locale))}
      <div
        class="provider-card glass-card"
        class:added={!!configured}
      >
        <div class="card-top">
          <ProviderLogo entry={e} />
          <div class="card-head-info">
            <div class="card-title">
              <span class="name">{localize(e.name, $locale)}</span>
              <span class="cat-badge cat-{e.category}">{categoryLabel(e.category)}</span>
            </div>
            {#if e.description}<div class="desc">{localize(e.description, $locale)}</div>{/if}
          </div>
        </div>

        <div class="card-meta">
          {#if pUsage && (pUsage.totals.input + pUsage.totals.cache_read + pUsage.totals.cache_creation + pUsage.totals.output) > 0}
            <div class="usage-line" title={$_('providers.usageLineHint')}>
              {$_('providers.usageMonth', { values: { tokens: fmtTokens(pUsage.totals.input + pUsage.totals.cache_read + pUsage.totals.cache_creation + pUsage.totals.output) } })}
            </div>
          {/if}
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
          {#if e.freeTier}
            <span
              class="free-chip free-{e.freeTier.kind}"
              class:stale={isStale(e.freeTier.verifiedAt)}
              title={freeTitle(e.freeTier)}
            >
              {e.freeTier.kind === 'recurring' ? $_('providers.free.recurring') : $_('providers.free.trial')}
            </span>
            {#if e.freeTier.noCard}
              <span class="free-chip no-card" title={$_('providers.free.noCard')}>{$_('providers.free.noCardShort')}</span>
            {/if}
          {/if}
          {#if configured && (configured.models?.length ?? 0) > 0}
            <span class="model-count">{$_('providers.modelCount', { values: { count: configured.models.length } })}</span>
          {/if}
          <!-- 使用中徽章:绑定表反查该服务商被哪些 agent 使用,悬停列出名单 -->
          {#if configured && (usageByProvider[configured.id]?.length ?? 0) > 0}
            <span
              class="sync-badge synced"
              title={usageByProvider[configured.id].map(agentLabel).join(', ')}
            >{$_('providers.usedBy', { values: { count: usageByProvider[configured.id].length } })}</span>
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
      {/if}

      <!-- 内联配置面板:整行插在所点卡片所在行的行尾之后,同行卡片位置不动 -->
      {#if editorOpen && i === editRowEnd}
        <div class="config-panel glass-card">
          <h3>{editingId === null ? $_('providers.addTitle') : $_('providers.editTitle')} · {editingEntry ? localize(editingEntry.name, $locale) : ''}</h3>

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

  .category-bar { display: flex; gap: 0.5rem; flex-wrap: wrap; align-items: center; }
  .bar-sep { width: 1px; align-self: stretch; margin: 0.15rem 0.25rem; background: rgba(255,255,255,0.12); }
  /* 免费开关用绿色语义,与青色的分类 chip 区分开 —— 它是正交筛选,不是第六个分类 */
  .chip.free-toggle.active { background: rgba(74,222,128,0.14); border-color: #4ade80; color: #4ade80; }
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
  .provider-card .usage-line {
    font-size: 0.72rem;
    color: var(--neon-cyan);
    font-variant-numeric: tabular-nums;
    padding: 0.2rem 0;
    border-top: 1px solid var(--border-subtle);
  }
  .provider-card.added { border-color: rgba(94,234,212,0.35); }

  /* 新增自定义服务商卡:虚线占位,居中加号 */
  .add-custom-card {
    align-items: center; justify-content: center; gap: 0.5rem; min-height: 120px;
    border-style: dashed; border-color: rgba(124,92,255,0.5); color: var(--text-secondary);
    cursor: pointer; background: transparent;
  }
  .add-custom-card:hover { border-color: #7c5cff; color: var(--text-primary); }
  .add-custom-card.active { border-color: #7c5cff; color: #a99bff; }
  .add-custom-card .add-plus { font-size: 1.6rem; line-height: 1; }
  .add-custom-card .add-label { font-size: 0.85rem; }

  .card-top { display: flex; align-items: center; gap: 0.85rem; }
  .card-head-info { min-width: 0; }
  .card-title { display: flex; align-items: center; gap: 0.5rem; flex-wrap: wrap; }
  .name { font-weight: 600; color: var(--text-primary); }
  .cat-badge { font-size: 0.65rem; padding: 0.1rem 0.5rem; border-radius: 999px; white-space: nowrap; }
  .cat-badge.cat-intl { background: rgba(66,133,244,0.15); color: #7aa7ff; }
  .cat-badge.cat-cn { background: rgba(255,107,107,0.15); color: #ff8b8b; }
  .cat-badge.cat-aggregator { background: rgba(123,97,255,0.15); color: #a99bff; }
  .cat-badge.cat-local { background: rgba(148,163,184,0.15); color: #cbd5e1; }
  .cat-badge.cat-custom { background: rgba(124,92,255,0.15); color: #a99bff; }
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
  .free-chip {
    font-size: 0.65rem; padding: 0.1rem 0.45rem; border-radius: 999px; white-space: nowrap;
    background: rgba(74,222,128,0.15); color: #4ade80; cursor: help;
  }
  /* 长期免费 = 绿色实心;一次性试用金 = 描边弱化,区分「用完就没」 */
  .free-chip.free-trial {
    background: transparent; color: #a3b18a; border: 1px solid rgba(163,177,138,0.45);
  }
  .free-chip.no-card { background: rgba(0,245,255,0.12); color: var(--neon-cyan); }
  /* 超过核对期:淡化,悬停文案会提示去官网确认 */
  .free-chip.stale { opacity: 0.45; }
  .sync-badge { font-size: 0.65rem; padding: 0.1rem 0.45rem; border-radius: 999px; white-space: nowrap; }
  .sync-badge.synced { background: rgba(74,222,128,0.15); color: #4ade80; cursor: help; }

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
  .btn:disabled { opacity: 0.5; cursor: default; }

  .empty { text-align: center; color: var(--text-muted); padding: 3rem; }

  .error-text { font-size: 0.75rem; color: #f87171; white-space: pre-wrap; margin: 0; }

  /* 去同步快捷入口条(全站统一样式) */
  .quick-sync-bar {
    display: flex; align-items: center; gap: 0.6rem;
    padding: 0.45rem 0.8rem; border-radius: 0.5rem;
    background: rgba(0,245,255,0.08); border: 1px solid rgba(0,245,255,0.3);
  }
  .qs-hint { flex: 1; font-size: 0.78rem; color: var(--neon-cyan); }
  .qs-close {
    background: transparent; border: none; cursor: pointer; color: var(--text-muted);
    font-size: 0.75rem; padding: 0.2rem 0.3rem; line-height: 1;
  }
  .qs-close:hover { color: var(--text-primary); }

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

  /* cc-switch 导入面板(复用原同步面板视觉,与 MCP 同步面板一致) */
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
  .plan-row { display: flex; align-items: center; gap: 0.6rem; }
  .row-check { flex-shrink: 0; cursor: pointer; }
  .row-check:disabled { cursor: default; opacity: 0.4; }
  .plan-head {
    flex: 1; min-width: 0; display: flex; align-items: center; gap: 0.6rem;
    background: transparent; border: none; padding: 0; margin: 0;
    color: inherit; font: inherit; text-align: left; cursor: default;
  }
  .plan-info { min-width: 0; flex: 1; display: flex; flex-direction: column; gap: 0.2rem; }
  .plan-title-line { display: flex; align-items: center; gap: 0.5rem; flex-wrap: wrap; }
  .agent-name { font-weight: 600; font-size: 0.9rem; }
  .tag { font-size: 0.65rem; padding: 0.1rem 0.5rem; border-radius: 999px; white-space: nowrap; }
  .tag.green { background: rgba(74,222,128,0.15); color: #4ade80; }
  .tag.amber { background: rgba(202,164,60,0.12); color: #d0b978; }
  .tag.yellow { background: rgba(251,191,36,0.15); color: #fbbf24; }
  .tag.gray { background: rgba(148,163,184,0.15); color: #cbd5e1; }

  /* 配置导入/导出面板 */
  .transfer-picks { display: flex; gap: 0.4rem 1rem; flex-wrap: wrap; }
  .transfer-opts { display: flex; gap: 0.4rem 1.2rem; flex-wrap: wrap; padding-top: 0.4rem; border-top: 1px solid rgba(255,255,255,0.08); }
  .keys-warning { margin: 0; font-size: 0.75rem; color: #f87171; }
  .export-done { margin: 0; font-size: 0.78rem; color: #4ade80; word-break: break-all; }
  .transfer-group { display: flex; flex-direction: column; gap: 0.35rem; }
  .transfer-group-title { font-size: 0.72rem; color: var(--text-muted); text-transform: uppercase; letter-spacing: 0.05em; }
  .transfer-detail { font-size: 0.72rem; color: var(--text-muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .check-label.muted { opacity: 0.5; cursor: default; }
  .empty-note { margin: 0; color: var(--text-muted); font-size: 0.82rem; }

  /* cc-switch 导入预览:候选行的 meta(key 掩码 / 模型 / 来源) */
  .import-meta { display: flex; align-items: center; gap: 0.5rem; flex-wrap: wrap; margin-top: 0.15rem; }
  .key-mask {
    font-size: 0.68rem; color: var(--text-secondary); background: var(--bg-tertiary);
    padding: 0.1rem 0.4rem; border-radius: 0.3rem; font-family: monospace;
  }
  .src-apps { font-size: 0.7rem; color: var(--text-muted); }
</style>
