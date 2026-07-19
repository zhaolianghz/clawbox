<script lang="ts">
  import { _ } from 'svelte-i18n';
  import { onMount } from 'svelte';
  import { open } from '@tauri-apps/plugin-dialog';
  import { list_backends, type BackendInfo, type BackendId } from '$lib/api/backends';
  import {
    skills_library_list, skills_import, skills_library_remove,
    skills_scan, skills_adopt, sync_skills_plan, sync_skills_apply,
    skills_repo_discover, skills_repo_install, skills_check_updates, skills_update,
    type SkillEntry, type SkillSource, type AdoptCandidate, type AdoptOutcome,
    type RepoDiscovery, type InstallOutcome, type SkillUpdateInfo,
    type AgentPlan, type ApplyResult, type ChangeItem,
  } from '$lib/api/skillsSync';
  import { SKILL_SOURCES } from '$lib/data/skillSources';
  import {
    memory_read, memory_write, memory_targets, memory_target_content,
    sync_memory_plan, sync_memory_apply,
    type MemoryTarget,
  } from '$lib/api/memorySync';
  import { agents_list } from '$lib/api/agents';
  import AgentLogo from '$lib/components/AgentLogo.svelte';
  import {
    list_tools_all, set_tool_enabled,
    type Tool,
  } from '$lib/api/capabilities/tools';
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

  type TabId = 'skills' | 'memory' | 'plugins' | 'tools' | 'hooks';

  const tabs: { id: TabId; key: string }[] = [
    { id: 'skills', key: 'capabilities.tab.skills' },
    { id: 'memory', key: 'capabilities.tab.memory' },
    { id: 'plugins', key: 'capabilities.tab.plugins' },
    { id: 'tools', key: 'capabilities.tab.tools' },
    { id: 'hooks', key: 'capabilities.tab.hooks' },
  ];

  let activeTab = $state<TabId>('skills');
  let backends = $state<BackendInfo[]>([]);
  let toolsByBackend = $state<Record<BackendId, Tool[]>>({ openclaw: [], hermes: [] });
  let memoryByBackend = $state<Record<BackendId, MemoryStatus | null>>({ openclaw: null, hermes: null });
  let pluginsByBackend = $state<Record<BackendId, Plugin[]>>({ openclaw: [], hermes: [] });
  let hooksByBackend = $state<Record<BackendId, Hook[]>>({ openclaw: [], hermes: [] });

  let toolErrors = $state<BackendError[]>([]);
  let memoryErrors = $state<BackendError[]>([]);
  let pluginsErrors = $state<BackendError[]>([]);
  let hooksErrors = $state<BackendError[]>([]);

  let isLoading = $state(true);
  let busyKey = $state<string | null>(null);

  let newPluginSource = $state('');

  async function load() {
    isLoading = true;
    const [bl, tools, mem, plugins, hooks] = await Promise.all([
      list_backends(),
      list_tools_all(),
      list_memory_all(),
      list_plugins_all(),
      list_hooks_all(),
    ]);
    backends = bl;
    toolErrors = tools.errors;
    memoryErrors = mem.errors;
    pluginsErrors = plugins.errors;
    hooksErrors = hooks.errors;

    const tm: Record<BackendId, Tool[]> = { openclaw: [], hermes: [] };
    for (const t of tools.items) tm[t.backend].push(t.item);
    toolsByBackend = tm;

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

  // ---------- 技能库(真源 ~/.agents/skills/,软链下发) ----------
  let library = $state<SkillEntry[]>([]);
  let libraryLoading = $state(true);
  let libraryError = $state('');
  let deletingSkill = $state<string | null>(null); // 两步删除确认:当前展开确认的技能名
  let importing = $state(false);
  let importError = $state('');

  async function loadLibrary() {
    libraryLoading = true;
    libraryError = '';
    try {
      library = await skills_library_list();
    } catch (e) {
      libraryError = String(e);
    } finally {
      libraryLoading = false;
    }
  }

  /** 系统目录选择器(OS 对话框,非应用内弹窗)→ skills_import */
  async function importSkill() {
    importError = '';
    let dir: string | string[] | null = null;
    try {
      dir = await open({ directory: true, multiple: false });
    } catch (e) {
      importError = String(e);
      return;
    }
    if (typeof dir !== 'string' || !dir) return; // 用户取消
    importing = true;
    try {
      await skills_import(dir);
      await loadLibrary();
    } catch (e) {
      importError = String(e);
    } finally {
      importing = false;
    }
  }

  async function removeSkill(name: string) {
    deletingSkill = null;
    libraryError = '';
    try {
      await skills_library_remove(name);
      await loadLibrary();
    } catch (e) {
      libraryError = String(e);
    }
  }

  // ---------- 从 Git 仓库安装技能(内联展开:精选源 + repo 输入 + 发现列表) ----------
  let installOpen = $state(false);
  let repoInput = $state('');
  let discovering = $state(false);
  let discoverError = $state('');
  let discovery = $state<RepoDiscovery | null>(null);
  let installChecked = $state<Record<string, boolean>>({}); // key = subdir
  let installingSkills = $state(false);
  let installOutcomes = $state<Record<string, InstallOutcome>>({}); // key = 技能名,就地标注

  function toggleInstallPanel() {
    installOpen = !installOpen;
  }

  /** 点精选源卡片 = 填入其 repo 并自动发现 */
  function pickSource(repo: string) {
    repoInput = repo;
    void discoverRepo();
  }

  /** 克隆仓库并解析技能列表(需数秒,spinner 提示) */
  async function discoverRepo() {
    const repo = repoInput.trim();
    if (!repo || discovering) return;
    discovering = true;
    discoverError = '';
    discovery = null;
    installChecked = {}; // 默认全不勾
    installOutcomes = {};
    try {
      discovery = await skills_repo_discover(repo);
    } catch (e) {
      discoverError = String(e);
    } finally {
      discovering = false;
    }
  }

  const installCheckedCount = $derived(
    discovery ? discovery.skills.filter((s) => !s.in_library && installChecked[s.subdir]).length : 0
  );

  async function installSelected() {
    if (!discovery || installingSkills) return;
    const picked = discovery.skills.filter((s) => !s.in_library && installChecked[s.subdir]);
    if (picked.length === 0) return;
    installingSkills = true;
    discoverError = '';
    try {
      const outcomes = await skills_repo_install(discovery.repo, picked.map((s) => s.subdir));
      installOutcomes = Object.fromEntries(outcomes.map((o) => [o.name, o]));
      await loadLibrary(); // 安装成功的技能进库
    } catch (e) {
      discoverError = String(e);
    } finally {
      installingSkills = false;
    }
  }

  // ---------- 检查更新(仅来源追踪的技能;单个更新,不做批量) ----------
  let checkingUpdates = $state(false);
  let updatesError = $state('');
  let updates = $state<Record<string, SkillUpdateInfo> | null>(null); // null = 未检查过
  let updatingSkill = $state<Record<string, boolean>>({});
  let updateErrors = $state<Record<string, string>>({}); // 单技能更新失败红字

  const allUpToDate = $derived(
    updates !== null && Object.values(updates).every((u) => !u.has_update && !u.missing)
  );

  async function checkUpdates() {
    if (checkingUpdates) return;
    checkingUpdates = true;
    updatesError = '';
    updateErrors = {};
    try {
      const list = await skills_check_updates();
      updates = Object.fromEntries(list.map((u) => [u.name, u]));
    } catch (e) {
      updatesError = String(e);
    } finally {
      checkingUpdates = false;
    }
  }

  /** 单技能更新;成功后本地翻掉「有更新」徽章并刷新库(不重跑全量网络检查) */
  async function updateOne(name: string) {
    if (updatingSkill[name]) return;
    updatingSkill = { ...updatingSkill, [name]: true };
    updateErrors = { ...updateErrors, [name]: '' };
    try {
      const results = await skills_update([name]);
      const r = results[0];
      if (r?.ok) {
        if (updates?.[name]) {
          updates = {
            ...updates,
            [name]: { ...updates[name], has_update: false, current_commit: updates[name].latest_commit },
          };
        }
        await loadLibrary();
      } else if (r) {
        updateErrors = { ...updateErrors, [name]: r.detail };
      }
    } catch (e) {
      updateErrors = { ...updateErrors, [name]: String(e) };
    } finally {
      updatingSkill = { ...updatingSkill, [name]: false };
    }
  }

  /** 来源徽章悬停:commit 短哈希 · subdir · 安装时间 */
  function sourceTitle(src: SkillSource): string {
    const parts = [src.commit.slice(0, 7)];
    if (src.subdir) parts.push(src.subdir);
    if (src.installed_at) parts.push(src.installed_at);
    return parts.join(' · ');
  }

  // ---------- 扫描收编(存量技能 → 复制进库 + 原位换软链) ----------
  let scanOpen = $state(false);
  let scanning = $state(false);
  let scanError = $state('');
  let candidates = $state<AdoptCandidate[]>([]);
  let adoptChecked = $state<Record<string, boolean>>({}); // key = agent_id/name
  let adopting = $state(false);
  let adoptOutcomes = $state<Record<string, AdoptOutcome>>({}); // 逐条结果就地显示

  function candKey(c: { agent_id: string; name: string }): string {
    return `${c.agent_id}/${c.name}`;
  }

  const candidatesByAgent = $derived.by(() => {
    const map = new Map<string, AdoptCandidate[]>();
    for (const c of candidates) {
      const list = map.get(c.agent_id) ?? [];
      list.push(c);
      map.set(c.agent_id, list);
    }
    return [...map.entries()];
  });

  const adoptCheckedCount = $derived(candidates.filter((c) => adoptChecked[candKey(c)]).length);

  async function toggleScan() {
    if (scanOpen) {
      scanOpen = false;
      return;
    }
    scanOpen = true;
    await runScan();
  }

  async function runScan() {
    if (scanning) return;
    scanning = true;
    scanError = '';
    candidates = [];
    adoptChecked = {}; // 默认全不勾
    adoptOutcomes = {};
    try {
      candidates = await skills_scan();
    } catch (e) {
      scanError = String(e);
    } finally {
      scanning = false;
    }
  }

  /** 收编勾选项;结果逐条就地标注,库列表立即刷新。扫描列表保留结果展示,
   *  「重新扫描」手动刷新(立刻重扫会冲掉刚显示的逐条结果)。 */
  async function adoptSelected() {
    const picked = candidates.filter((c) => adoptChecked[candKey(c)]);
    if (picked.length === 0 || adopting) return;
    adopting = true;
    scanError = '';
    try {
      const outcomes = await skills_adopt(picked.map((c) => ({ agent_id: c.agent_id, name: c.name })));
      adoptOutcomes = Object.fromEntries(outcomes.map((o) => [candKey(o), o]));
      await loadLibrary(); // 收编成功的技能进库
    } catch (e) {
      scanError = String(e);
    } finally {
      adopting = false;
    }
  }

  // ---------- 同步到 Agent(与服务商页同款:行状态 + 单行同步 + 批量,结果就地写回) ----------
  type SyncStage = 'closed' | 'planning' | 'preview';
  let syncStage = $state<SyncStage>('closed');
  let plans = $state<AgentPlan[]>([]);
  let syncError = $state('');
  let expanded = $state<Record<string, boolean>>({});
  let checked = $state<Record<string, boolean>>({});
  let batchApplying = $state(false);
  let rowApplying = $state<Record<string, boolean>>({});
  let rowError = $state<Record<string, string>>({});
  let rowSynced = $state<Record<string, boolean>>({});
  let rowBackup = $state<Record<string, string>>({});

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

  function realChanges(p: AgentPlan): ChangeItem[] {
    return p.changes.filter((c) => c.action === 'add' || c.action === 'update' || c.action === 'remove');
  }

  function skipItems(p: AgentPlan): ChangeItem[] {
    return p.changes.filter((c) => c.action === 'skip');
  }

  /** 行状态口径:全 unchanged=已同步;有 add/update/remove=未同步;skip 项只进展开明细 */
  type RowStatus = 'synced' | 'pending' | 'unsupported' | 'error';

  function rowStatus(p: AgentPlan): RowStatus {
    if (!p.supported) return 'unsupported';
    if (p.error) return 'error';
    if (rowSynced[p.agent_id]) return 'synced';
    if (realChanges(p).length > 0) return 'pending';
    return 'synced';
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
      plans = await sync_skills_plan();
      checked = Object.fromEntries(plans.filter(selectable).map((p) => [p.agent_id, true]));
      syncStage = 'preview';
    } catch (e) {
      syncError = String(e);
      syncStage = 'preview';
    }
  }

  function recordResult(r: ApplyResult) {
    if (r.ok) {
      rowSynced = { ...rowSynced, [r.agent_id]: true };
      rowError = { ...rowError, [r.agent_id]: '' };
      if (r.backup_path) rowBackup = { ...rowBackup, [r.agent_id]: r.backup_path };
    } else {
      rowError = { ...rowError, [r.agent_id]: r.error ?? 'apply failed' };
    }
  }

  async function applyOne(id: string) {
    if (rowApplying[id] || batchApplying) return;
    rowApplying = { ...rowApplying, [id]: true };
    rowError = { ...rowError, [id]: '' };
    try {
      const results = await sync_skills_apply([id]);
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
      const results = await sync_skills_apply(ids);
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

  async function toggleTool(t: Tool, backend: BackendId) {
    const key = `tool:${backend}:${t.id}`;
    busyKey = key;
    try {
      await set_tool_enabled(backend, t.id, !t.enabled);
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

  // ---------- 统一指令记忆(真源 ~/.agents/memory/MEMORY.md,托管区块注入) ----------
  let memoryContent = $state('');
  let memoryLoaded = $state(''); // 已加载基线;dirty = 内容 ≠ 基线
  let memoryLoading = $state(true);
  let memReadError = $state('');
  let memorySaving = $state(false);
  let memorySavedFlag = $state(false); // 保存成功提示(下次加载/再改动前有效)

  const memoryDirty = $derived(memoryContent !== memoryLoaded);

  async function loadMemory() {
    memoryLoading = true;
    memReadError = '';
    memorySavedFlag = false;
    try {
      const v = await memory_read();
      memoryContent = v;
      memoryLoaded = v;
    } catch (e) {
      memReadError = String(e);
    } finally {
      memoryLoading = false;
    }
  }

  async function saveMemory() {
    if (memorySaving || !memoryDirty) return;
    memorySaving = true;
    memReadError = '';
    try {
      await memory_write(memoryContent);
      memoryLoaded = memoryContent;
      memorySavedFlag = true;
    } catch (e) {
      memReadError = String(e);
    } finally {
      memorySaving = false;
    }
  }

  // 各 agent 指令文件面板
  let targets = $state<MemoryTarget[]>([]);
  let targetsLoading = $state(false);
  let targetsError = $state('');
  let viewingTarget = $state<string | null>(null); // 展开查看全文的 agent_id
  let targetContent = $state<Record<string, string>>({});
  let targetContentLoading = $state<Record<string, boolean>>({});
  let targetContentError = $state<Record<string, string>>({});

  async function loadTargets() {
    if (targetsLoading) return;
    targetsLoading = true;
    targetsError = '';
    targetContent = {};
    targetContentError = {};
    try {
      targets = await memory_targets();
    } catch (e) {
      targetsError = String(e);
    } finally {
      targetsLoading = false;
    }
  }

  async function toggleViewTarget(id: string) {
    if (viewingTarget === id) {
      viewingTarget = null;
      return;
    }
    viewingTarget = id;
    if (targetContent[id] === undefined && !targetContentLoading[id]) {
      targetContentLoading = { ...targetContentLoading, [id]: true };
      targetContentError = { ...targetContentError, [id]: '' };
      try {
        const text = await memory_target_content(id);
        targetContent = { ...targetContent, [id]: text };
      } catch (e) {
        targetContentError = { ...targetContentError, [id]: String(e) };
      } finally {
        targetContentLoading = { ...targetContentLoading, [id]: false };
      }
    }
  }

  /** 把该 agent 指令文件全文追加到编辑器 buffer 末尾(仅前端;用户整理后自行保存) */
  function importToLibrary(id: string) {
    const text = (targetContent[id] ?? '').trim();
    if (!text) return;
    memoryContent = memoryContent.trim()
      ? memoryContent.replace(/\s*$/, '\n\n') + text + '\n'
      : text + '\n';
  }

  // 记忆同步面板(与技能面板同款,独立一套 mem 前缀状态)
  let memSyncStage = $state<SyncStage>('closed');
  let memPlans = $state<AgentPlan[]>([]);
  let memSyncError = $state('');
  let memExpanded = $state<Record<string, boolean>>({});
  let memChecked = $state<Record<string, boolean>>({});
  let memBatchApplying = $state(false);
  let memRowApplying = $state<Record<string, boolean>>({});
  let memRowError = $state<Record<string, string>>({});
  let memRowSynced = $state<Record<string, boolean>>({});
  let memRowBackup = $state<Record<string, string>>({});

  function memRowStatus(p: AgentPlan): RowStatus {
    if (!p.supported) return 'unsupported';
    if (p.error) return 'error';
    if (memRowSynced[p.agent_id]) return 'synced';
    if (realChanges(p).length > 0) return 'pending';
    return 'synced';
  }

  function memSelectable(p: AgentPlan): boolean {
    return p.supported && !p.error && !memRowSynced[p.agent_id] && realChanges(p).length > 0;
  }

  const memCheckedCount = $derived(memPlans.filter((p) => memSelectable(p) && memChecked[p.agent_id]).length);
  const memSelectablePlans = $derived(memPlans.filter((p) => memSelectable(p)));
  const memAllPicked = $derived(
    memSelectablePlans.length > 0 && memSelectablePlans.every((p) => memChecked[p.agent_id])
  );

  function memToggleAll() {
    const next = { ...memChecked };
    for (const p of memSelectablePlans) next[p.agent_id] = !memAllPicked;
    memChecked = next;
  }

  async function memStartSync() {
    memSyncStage = 'planning';
    memSyncError = '';
    memPlans = [];
    memExpanded = {};
    memChecked = {};
    memBatchApplying = false;
    memRowApplying = {};
    memRowError = {};
    memRowSynced = {};
    memRowBackup = {};
    try {
      memPlans = await sync_memory_plan();
      memChecked = Object.fromEntries(memPlans.filter(memSelectable).map((p) => [p.agent_id, true]));
      memSyncStage = 'preview';
    } catch (e) {
      memSyncError = String(e);
      memSyncStage = 'preview';
    }
  }

  function memRecordResult(r: ApplyResult) {
    if (r.ok) {
      memRowSynced = { ...memRowSynced, [r.agent_id]: true };
      memRowError = { ...memRowError, [r.agent_id]: '' };
      if (r.backup_path) memRowBackup = { ...memRowBackup, [r.agent_id]: r.backup_path };
    } else {
      memRowError = { ...memRowError, [r.agent_id]: r.error ?? 'apply failed' };
    }
  }

  async function memApplyOne(id: string) {
    if (memRowApplying[id] || memBatchApplying) return;
    memRowApplying = { ...memRowApplying, [id]: true };
    memRowError = { ...memRowError, [id]: '' };
    try {
      const results = await sync_memory_apply([id]);
      if (results[0]) memRecordResult(results[0]);
      if (results[0]?.ok) void loadTargets(); // 注入状态变化,刷新目标面板
    } catch (e) {
      memRowError = { ...memRowError, [id]: String(e) };
    } finally {
      memRowApplying = { ...memRowApplying, [id]: false };
    }
  }

  async function memApplyChecked() {
    const ids = memPlans.filter((p) => memSelectable(p) && memChecked[p.agent_id]).map((p) => p.agent_id);
    if (ids.length === 0 || memBatchApplying) return;
    memBatchApplying = true;
    memSyncError = '';
    memRowApplying = { ...memRowApplying, ...Object.fromEntries(ids.map((id) => [id, true])) };
    try {
      const results = await sync_memory_apply(ids);
      for (const r of results) memRecordResult(r);
      if (results.some((r) => r.ok)) void loadTargets();
    } catch (e) {
      memSyncError = String(e);
    } finally {
      memBatchApplying = false;
      memRowApplying = { ...memRowApplying, ...Object.fromEntries(ids.map((id) => [id, false])) };
    }
  }

  function memToggleExpand(id: string) {
    memExpanded = { ...memExpanded, [id]: !memExpanded[id] };
  }

  function memCloseSync() {
    memSyncStage = 'closed';
  }

  onMount(async () => {
    void loadLibrary();
    void loadMemory();
    void loadTargets();
    try {
      const all = await agents_list();
      agentLabels = Object.fromEntries(all.map((a) => [a.id, a.label]));
    } catch { /* 回退本地映射即可 */ }
    await load();
  });
</script>


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
      <div class="skills-sync">
        <!-- 工具条:导入 / 扫描收编 / 同步到 Agent -->
        <div class="skills-toolbar">
          <span class="skills-hint">{$_('capabilities.skillsSync.libraryHint')}</span>
          <button class="action-btn wide" class:on={installOpen} onclick={toggleInstallPanel} disabled={installingSkills}>
            {$_('capabilities.skillsSync.install')}
          </button>
          <button class="action-btn wide" onclick={importSkill} disabled={importing}>
            {#if importing}<span class="spinner small"></span>{/if}
            {$_('capabilities.skillsSync.import')}
          </button>
          <button class="action-btn wide" class:on={scanOpen} onclick={toggleScan} disabled={adopting}>
            {$_('capabilities.skillsSync.scan')}
          </button>
          <button class="action-btn wide" onclick={checkUpdates} disabled={checkingUpdates}>
            {#if checkingUpdates}<span class="spinner small"></span>{/if}
            {$_('capabilities.skillsSync.checkUpdates')}
          </button>
          <button class="action-btn wide primary" onclick={startSync} disabled={syncStage !== 'closed'}>
            {$_('capabilities.skillsSync.syncToAgents')}
          </button>
        </div>
        {#if importError}
          <p class="error-line">{importError}</p>
        {/if}
        {#if updatesError}
          <p class="error-line">{updatesError}</p>
        {/if}
        {#if updates !== null && allUpToDate && !updatesError}
          <p class="uptodate-note">{$_('capabilities.skillsSync.allUpToDate')}</p>
        {/if}

        <!-- 从 Git 仓库安装(内联展开:精选源卡片 + repo 输入 + 发现列表 + 安全提示) -->
        {#if installOpen}
          <div class="scan-panel">
            <div class="featured-row">
              {#each SKILL_SOURCES as src (src.id)}
                <button
                  type="button"
                  class="source-card"
                  class:sel={repoInput.trim() === src.repo}
                  onclick={() => pickSource(src.repo)}
                  disabled={discovering || installingSkills}
                >
                  <span class="source-name">{src.name}</span>
                  <span class="source-repo">{src.repo}</span>
                  <span class="source-desc">{src.description}</span>
                </button>
              {/each}
            </div>
            <div class="repo-row">
              <input
                class="text-input repo-input"
                type="text"
                placeholder={$_('capabilities.skillsSync.repoPlaceholder')}
                bind:value={repoInput}
                onkeydown={(e) => { if (e.key === 'Enter') { e.preventDefault(); discoverRepo(); } }}
              />
              <button
                class="action-btn wide primary"
                onclick={discoverRepo}
                disabled={discovering || installingSkills || !repoInput.trim()}
              >
                {#if discovering}<span class="spinner small"></span>{/if}
                {$_('capabilities.skillsSync.discover')}
              </button>
            </div>
            {#if discovering}
              <div class="sync-loading"><span class="spinner small"></span> {$_('capabilities.skillsSync.discovering')}</div>
            {/if}
            {#if discoverError}
              <p class="error-line">{discoverError}</p>
            {/if}
            {#if discovery}
              {#if discovery.skills.length === 0}
                <p class="empty">{$_('capabilities.skillsSync.noSkillsInRepo')}</p>
              {:else}
                {@const selectableSkills = discovery.skills.filter((s) => !s.in_library && !installOutcomes[s.name]?.ok)}
                {@const allPicked = selectableSkills.length > 0 && selectableSkills.every((s) => installChecked[s.subdir])}
                <label class="scan-row select-all-row">
                  <input
                    type="checkbox"
                    disabled={installingSkills || selectableSkills.length === 0}
                    checked={allPicked}
                    onchange={() => {
                      const next = { ...installChecked };
                      for (const s of selectableSkills) next[s.subdir] = !allPicked;
                      installChecked = next;
                    }}
                  />
                  <span class="scan-name">{$_('capabilities.skillsSync.selectAll')}</span>
                  <span class="scan-desc">{selectableSkills.length}</span>
                </label>
                {#each discovery.skills as s (s.subdir)}
                  {@const outcome = installOutcomes[s.name]}
                  <label class="scan-row" class:muted-row={s.in_library}>
                    <input
                      type="checkbox"
                      disabled={s.in_library || installingSkills || outcome?.ok === true}
                      checked={!s.in_library && !outcome?.ok && !!installChecked[s.subdir]}
                      onchange={(e) => (installChecked = { ...installChecked, [s.subdir]: e.currentTarget.checked })}
                    />
                    <span class="scan-name">{s.name}</span>
                    {#if s.description}<span class="scan-desc" title={s.description}>{s.description}</span>{/if}
                    {#if s.in_library}
                      <span class="relink-badge">{$_('capabilities.skillsSync.alreadyInstalled')}</span>
                    {/if}
                    {#if outcome}
                      {#if outcome.ok}
                        <span class="adopt-ok">✓ {$_('capabilities.skillsSync.installedOk')}</span>
                      {:else}
                        <span class="adopt-fail">✗ {outcome.detail}</span>
                      {/if}
                    {/if}
                  </label>
                {/each}
                <div class="scan-foot">
                  <button
                    class="action-btn wide primary"
                    onclick={installSelected}
                    disabled={installCheckedCount === 0 || installingSkills}
                  >
                    {#if installingSkills}<span class="spinner small"></span>{/if}
                    {$_('capabilities.skillsSync.installSelected', { values: { count: installCheckedCount } })}
                  </button>
                </div>
              {/if}
            {/if}
            <p class="security-note">{$_('capabilities.skillsSync.securityNote')}</p>
          </div>
        {/if}

        <!-- 同步面板(与服务商页同款:逐行状态 + 单行同步 + 批量,结果就地写回) -->
        {#if syncStage !== 'closed'}
          <div class="sync-panel">
            {#if syncStage === 'planning'}
              <div class="sync-loading"><span class="spinner small"></span> {$_('providers.sync.planning')}</div>
            {:else}
              {#if syncError}
                <p class="error-line">{syncError}</p>
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
                            class="action-btn wide primary"
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
                        <p class="error-line">{p.error}</p>
                      {/if}
                      {#if rowError[p.agent_id]}
                        <p class="error-line">{rowError[p.agent_id]}</p>
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
                <button class="action-btn wide" onclick={closeSync} disabled={batchApplying}>{$_('providers.close')}</button>
                <button class="action-btn wide primary" onclick={applyChecked} disabled={checkedCount === 0 || batchApplying}>
                  {#if batchApplying}<span class="spinner small"></span>{/if}
                  {$_('providers.sync.confirm', { values: { count: checkedCount } })}
                </button>
              </div>
            {/if}
          </div>
        {/if}

        <!-- 扫描收编区(内联展开;结果逐条就地标注,重新扫描才刷新列表) -->
        {#if scanOpen}
          <div class="scan-panel">
            <div class="scan-head">
              <span class="scan-title">{$_('capabilities.skillsSync.scanTitle')}</span>
              <button class="action-btn wide" onclick={runScan} disabled={scanning || adopting}>
                {$_('capabilities.skillsSync.rescan')}
              </button>
            </div>
            {#if scanning}
              <div class="sync-loading"><span class="spinner small"></span> {$_('capabilities.skillsSync.scanning')}</div>
            {:else}
              {#if scanError}
                <p class="error-line">{scanError}</p>
              {/if}
              {#if candidates.length === 0 && !scanError}
                <p class="empty">{$_('capabilities.skillsSync.scanEmpty')}</p>
              {:else if candidates.length > 0}
                {@const selectableCands = candidates.filter((c) => !adoptOutcomes[candKey(c)]?.ok)}
                {@const allCandsPicked = selectableCands.length > 0 && selectableCands.every((c) => adoptChecked[candKey(c)])}
                <label class="scan-row select-all-row">
                  <input
                    type="checkbox"
                    disabled={adopting || selectableCands.length === 0}
                    checked={allCandsPicked}
                    onchange={() => {
                      const next = { ...adoptChecked };
                      for (const c of selectableCands) next[candKey(c)] = !allCandsPicked;
                      adoptChecked = next;
                    }}
                  />
                  <span class="scan-name">{$_('capabilities.skillsSync.selectAll')}</span>
                  <span class="scan-desc">{selectableCands.length}</span>
                </label>
                {#each candidatesByAgent as [agentId, list] (agentId)}
                  <div class="scan-group">
                    <div class="scan-agent">
                      <AgentLogo id={agentId} label={agentLabel(agentId)} />
                      <span class="agent-name">{agentLabel(agentId)}</span>
                    </div>
                    {#each list as c (candKey(c))}
                      {@const key = candKey(c)}
                      {@const outcome = adoptOutcomes[key]}
                      <label class="scan-row">
                        <input
                          type="checkbox"
                          disabled={adopting || outcome?.ok === true}
                          checked={!outcome?.ok && !!adoptChecked[key]}
                          onchange={(e) => (adoptChecked = { ...adoptChecked, [key]: e.currentTarget.checked })}
                        />
                        <span class="scan-name">{c.name}</span>
                        {#if c.description}<span class="scan-desc" title={c.description}>{c.description}</span>{/if}
                        {#if c.in_library}
                          <span class="relink-badge">{$_('capabilities.skillsSync.inLibrary')}</span>
                        {/if}
                        {#if outcome}
                          {#if outcome.ok}
                            <span class="adopt-ok">✓ {$_('capabilities.skillsSync.adoptOk')}</span>
                          {:else}
                            <span class="adopt-fail">✗ {outcome.detail}</span>
                          {/if}
                        {/if}
                      </label>
                    {/each}
                  </div>
                {/each}
                <div class="scan-foot">
                  <button class="action-btn wide primary" onclick={adoptSelected} disabled={adoptCheckedCount === 0 || adopting}>
                    {#if adopting}<span class="spinner small"></span>{/if}
                    {$_('capabilities.skillsSync.adoptSelected', { values: { count: adoptCheckedCount } })}
                  </button>
                </div>
              {/if}
            {/if}
          </div>
        {/if}

        <!-- 技能库:卡片网格(对齐服务商/Agent 页卡片语言);path 收进标题悬停 title -->
        {#if libraryError}
          <p class="error-line">{libraryError}</p>
        {/if}
        <div class="skill-grid">
          {#if libraryLoading}
            <div class="grid-span sync-loading"><span class="spinner small"></span> {$_('capabilities.skillsSync.loading')}</div>
          {:else if library.length === 0 && !libraryError}
            <p class="grid-span empty">{$_('capabilities.skillsSync.empty')}</p>
          {:else}
            {#each library as s (s.name)}
              {@const u = updates?.[s.name]}
              <div class="glass-card skill-card">
                <div class="skill-card-title" title={s.path}>
                  <span class="skill-card-name">{s.name}</span>
                  {#if s.source}
                    <span class="source-badge" title={sourceTitle(s.source)}>{s.source.repo}</span>
                  {/if}
                </div>
                {#if s.description}
                  <p class="skill-card-desc" title={s.description}>{s.description}</p>
                {/if}
                {#if updateErrors[s.name]}
                  <div class="adopt-fail">✗ {updateErrors[s.name]}</div>
                {/if}
                <div class="skill-card-actions">
                  {#if u?.missing}
                    <span class="adopt-fail">{$_('capabilities.skillsSync.missingInSource')}</span>
                  {:else if u?.has_update}
                    <span class="update-badge no-ml">{$_('capabilities.skillsSync.hasUpdate')}</span>
                    <button
                      class="action-btn wide primary"
                      onclick={() => updateOne(s.name)}
                      disabled={!!updatingSkill[s.name]}
                    >
                      {#if updatingSkill[s.name]}<span class="spinner small"></span>{/if}
                      {$_('capabilities.skillsSync.update')}
                    </button>
                  {/if}
                  <span class="spacer"></span>
                  {#if deletingSkill === s.name}
                    <button class="action-btn wide danger" onclick={() => removeSkill(s.name)}>
                      {$_('capabilities.skillsSync.confirmDelete')}
                    </button>
                    <button class="action-btn wide" onclick={() => (deletingSkill = null)}>
                      {$_('capabilities.skillsSync.cancel')}
                    </button>
                  {:else}
                    <button class="action-btn wide" onclick={() => (deletingSkill = s.name)}>
                      {$_('capabilities.skillsSync.delete')}
                    </button>
                  {/if}
                </div>
              </div>
            {/each}
          {/if}
        </div>
      </div>
    {:else if activeTab === 'memory'}
      <div class="skills-sync">
        <!-- 统一记忆编辑器 -->
        <div class="skills-toolbar">
          <span class="skills-hint">{$_('capabilities.memorySync.hint')}</span>
          <button class="action-btn wide primary" onclick={memStartSync} disabled={memSyncStage !== 'closed'}>
            {$_('capabilities.memorySync.syncToAgents')}
          </button>
        </div>
        {#if memReadError}
          <p class="error-line">{memReadError}</p>
        {/if}
        {#if memoryLoading}
          <div class="sync-loading"><span class="spinner small"></span> {$_('capabilities.memorySync.loading')}</div>
        {:else}
          <textarea
            class="memory-editor"
            bind:value={memoryContent}
            spellcheck="false"
            placeholder={$_('capabilities.memorySync.placeholder')}
          ></textarea>
          <div class="memory-actions">
            {#if memoryDirty}
              <span class="dirty-note">● {$_('capabilities.memorySync.unsaved')}</span>
            {:else if memorySavedFlag}
              <span class="adopt-ok">✓ {$_('capabilities.memorySync.saved')}</span>
            {/if}
            <span class="spacer"></span>
            <button class="action-btn wide primary" onclick={saveMemory} disabled={memorySaving || !memoryDirty}>
              {#if memorySaving}<span class="spinner small"></span>{/if}
              {$_('capabilities.memorySync.save')}
            </button>
          </div>
        {/if}

        <!-- 记忆同步面板(与技能面板同款:全选/单行同步/批量,结果就地写回) -->
        {#if memSyncStage !== 'closed'}
          <div class="sync-panel">
            {#if memSyncStage === 'planning'}
              <div class="sync-loading"><span class="spinner small"></span> {$_('providers.sync.planning')}</div>
            {:else}
              {#if memSyncError}
                <p class="error-line">{memSyncError}</p>
              {/if}
              {#if memPlans.length > 0}
                <div class="plan-list">
                  <label class="select-all-plans">
                    <input
                      type="checkbox"
                      class="row-check"
                      disabled={memSelectablePlans.length === 0 || memBatchApplying}
                      checked={memAllPicked}
                      onchange={memToggleAll}
                    />
                    <span>{$_('providers.sync.selectAll')}</span>
                    <span class="selectable-count">{memCheckedCount}/{memSelectablePlans.length}</span>
                  </label>
                  {#each memPlans as p (p.agent_id)}
                    {@const status = memRowStatus(p)}
                    {@const changes = realChanges(p)}
                    {@const skips = skipItems(p)}
                    {@const expandable = p.changes.length > 0}
                    {@const canPick = memSelectable(p)}
                    <div class="plan-item" class:muted={status === 'unsupported'}>
                      <div class="plan-row">
                        <input
                          type="checkbox"
                          class="row-check"
                          disabled={!canPick || memBatchApplying}
                          checked={canPick && !!memChecked[p.agent_id]}
                          onchange={(e) => (memChecked = { ...memChecked, [p.agent_id]: e.currentTarget.checked })}
                          aria-label={agentLabel(p.agent_id)}
                        />
                        <button
                          type="button"
                          class="plan-head"
                          class:expandable
                          disabled={!expandable}
                          onclick={() => memToggleExpand(p.agent_id)}
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
                            <span class="chevron" class:open={!!memExpanded[p.agent_id]}>▾</span>
                          {/if}
                        </button>
                        {#if status === 'pending'}
                          <button
                            class="action-btn wide primary"
                            onclick={() => memApplyOne(p.agent_id)}
                            disabled={!!memRowApplying[p.agent_id] || memBatchApplying}
                          >
                            {#if memRowApplying[p.agent_id]}<span class="spinner small"></span>{/if}
                            {$_('providers.sync.syncOne')}
                          </button>
                        {/if}
                      </div>
                      {#if memRowSynced[p.agent_id] && memRowBackup[p.agent_id]}
                        <code class="config-path" title={memRowBackup[p.agent_id]}>
                          {$_('providers.sync.backup')}: {memRowBackup[p.agent_id]}
                        </code>
                      {/if}
                      {#if p.error}
                        <p class="error-line">{p.error}</p>
                      {/if}
                      {#if memRowError[p.agent_id]}
                        <p class="error-line">{memRowError[p.agent_id]}</p>
                      {/if}
                      {#if expandable && memExpanded[p.agent_id]}
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
                      {/if}
                    </div>
                  {/each}
                </div>
              {/if}
              <div class="panel-actions">
                <button class="action-btn wide" onclick={memCloseSync} disabled={memBatchApplying}>{$_('providers.close')}</button>
                <button class="action-btn wide primary" onclick={memApplyChecked} disabled={memCheckedCount === 0 || memBatchApplying}>
                  {#if memBatchApplying}<span class="spinner small"></span>{/if}
                  {$_('providers.sync.confirm', { values: { count: memCheckedCount } })}
                </button>
              </div>
            {/if}
          </div>
        {/if}

        <!-- 各 agent 指令文件面板 -->
        <div class="scan-panel">
          <div class="scan-head">
            <span class="scan-title">{$_('capabilities.memorySync.targetsTitle')}</span>
            <button class="action-btn wide" onclick={loadTargets} disabled={targetsLoading}>
              {#if targetsLoading}<span class="spinner small"></span>{/if}
              {$_('capabilities.memorySync.refresh')}
            </button>
          </div>
          {#if targetsError}
            <p class="error-line">{targetsError}</p>
          {/if}
          {#if targetsLoading && targets.length === 0}
            <div class="sync-loading"><span class="spinner small"></span> {$_('capabilities.memorySync.loading')}</div>
          {:else if targets.length === 0 && !targetsError}
            <p class="empty">{$_('capabilities.memorySync.targetsEmpty')}</p>
          {:else}
            {#each targets as t (t.agent_id)}
              <div class="plan-item">
                <div class="plan-row">
                  <button type="button" class="plan-head expandable" onclick={() => toggleViewTarget(t.agent_id)}>
                    <AgentLogo id={t.agent_id} label={agentLabel(t.agent_id)} />
                    <div class="plan-info">
                      <div class="plan-title-line">
                        <span class="agent-name">{agentLabel(t.agent_id)}</span>
                        {#if !t.exists}
                          <span class="tag gray">{$_('capabilities.memorySync.statusMissing')}</span>
                        {:else if !t.has_block}
                          <span class="tag yellow">{$_('capabilities.memorySync.statusNoBlock')}</span>
                        {:else}
                          <span
                            class="tag green"
                            title={t.outside_chars > 0
                              ? $_('capabilities.memorySync.outsideChars', { values: { count: t.outside_chars } })
                              : undefined}
                          >{$_('capabilities.memorySync.statusInjected')}</span>
                        {/if}
                      </div>
                      <code class="config-path" title={t.path}>{t.path}</code>
                    </div>
                    <span class="chevron" class:open={viewingTarget === t.agent_id}>▾</span>
                  </button>
                  <button
                    class="action-btn wide"
                    class:on={viewingTarget === t.agent_id}
                    onclick={() => toggleViewTarget(t.agent_id)}
                  >{$_('capabilities.memorySync.view')}</button>
                </div>
                {#if viewingTarget === t.agent_id}
                  {#if targetContentLoading[t.agent_id]}
                    <div class="sync-loading"><span class="spinner small"></span> {$_('capabilities.memorySync.loading')}</div>
                  {:else if targetContentError[t.agent_id]}
                    <p class="error-line">{targetContentError[t.agent_id]}</p>
                  {:else}
                    <pre class="target-content">{targetContent[t.agent_id]?.trim() ? targetContent[t.agent_id] : $_('capabilities.memorySync.emptyFile')}</pre>
                    <div class="scan-foot">
                      <button
                        class="action-btn wide"
                        onclick={() => importToLibrary(t.agent_id)}
                        disabled={!(targetContent[t.agent_id] ?? '').trim()}
                      >{$_('capabilities.memorySync.importToLib')}</button>
                    </div>
                  {/if}
                {/if}
              </div>
            {/each}
          {/if}
        </div>

        <!-- Agent 原生记忆(旧 per-backend 状态,降权:默认收起) -->
        <details class="native-memory">
          <summary>{$_('capabilities.memorySync.nativeTitle')}</summary>
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
        </details>
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
  .spinner.small { width: 12px; height: 12px; display: inline-block; }
  @keyframes spin { to { transform: rotate(360deg); } }

  /* ---------- 技能统一同步 tab ---------- */
  .skills-sync { display: flex; flex-direction: column; gap: 0.75rem; }
  .skills-toolbar { display: flex; align-items: center; gap: 0.5rem; flex-wrap: wrap; }
  .skills-hint { flex: 1; min-width: 200px; font-size: 0.72rem; color: var(--text-muted); }
  .action-btn.wide {
    width: auto; padding: 0 0.75rem; height: 28px; font-size: 0.75rem;
    display: inline-flex; align-items: center; gap: 0.35rem; white-space: nowrap;
  }
  .action-btn.danger { background: rgba(248,113,113,0.15); color: #f87171; }
  .action-btn.danger:hover { background: rgba(248,113,113,0.25); }
  .action-btn.on { background: rgba(0,245,255,0.1); color: var(--neon-cyan); }

  /* 技能库卡片网格(对齐服务商/Agent 页卡片语言) */
  .skill-grid {
    display: grid; gap: 0.75rem;
    grid-template-columns: repeat(auto-fill, minmax(260px, 1fr));
  }
  .grid-span { grid-column: 1 / -1; }
  .skill-card { padding: 0.9rem 1rem; display: flex; flex-direction: column; gap: 0.5rem; }
  .skill-card-title { display: flex; align-items: center; gap: 0.4rem; flex-wrap: wrap; min-width: 0; }
  .skill-card-title .source-badge { margin-left: 0; }
  .skill-card-name { font-weight: 600; font-size: 0.9rem; color: var(--text-primary); }
  .skill-card-desc {
    margin: 0; font-size: 0.75rem; color: var(--text-secondary); line-height: 1.45;
    display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2;
    -webkit-box-orient: vertical; overflow: hidden;
  }
  .skill-card-actions { display: flex; align-items: center; gap: 0.4rem; margin-top: auto; flex-wrap: wrap; }
  .skill-card-actions .spacer { flex: 1; }
  .update-badge.no-ml { margin-left: 0; }

  .sync-loading { display: flex; align-items: center; gap: 0.5rem; font-size: 0.8rem; color: var(--text-secondary); padding: 0.5rem 0; }

  /* 同步面板(结构/配色与服务商页同款) */
  .sync-panel {
    background: rgba(0,0,0,0.15); border: 1px solid rgba(0,245,255,0.25);
    border-radius: 0.5rem; padding: 0.75rem; display: flex; flex-direction: column; gap: 0.6rem;
  }
  .plan-list { display: flex; flex-direction: column; gap: 0.6rem; }
  .select-all-plans {
    display: flex; align-items: center; gap: 0.6rem; cursor: pointer;
    padding-bottom: 0.45rem; border-bottom: 1px solid rgba(255,255,255,0.08);
    color: var(--neon-cyan); font-weight: 600; font-size: 0.8rem;
  }
  .selectable-count { color: var(--text-muted); font-weight: 400; font-size: 0.72rem; }
  .plan-item {
    border: 1px solid rgba(255,255,255,0.08); border-radius: 0.5rem;
    padding: 0.6rem 0.75rem; display: flex; flex-direction: column; gap: 0.4rem;
    background: var(--bg-primary);
  }
  .plan-item.muted { opacity: 0.55; }
  .plan-row { display: flex; align-items: center; gap: 0.6rem; }
  .row-check { flex-shrink: 0; cursor: pointer; }
  .row-check:disabled { cursor: default; opacity: 0.4; }
  .plan-head {
    flex: 1; min-width: 0; display: flex; align-items: center; gap: 0.6rem;
    background: transparent; border: none; padding: 0; margin: 0;
    color: inherit; font: inherit; text-align: left; cursor: default;
  }
  .plan-head.expandable { cursor: pointer; }
  .plan-info { min-width: 0; flex: 1; display: flex; flex-direction: column; gap: 0.2rem; }
  .plan-title-line { display: flex; align-items: center; gap: 0.5rem; flex-wrap: wrap; }
  .agent-name { font-weight: 600; font-size: 0.85rem; }
  .change-summary { font-size: 0.72rem; color: var(--text-secondary); }
  .chevron { flex-shrink: 0; font-size: 0.8rem; color: var(--text-muted); transition: transform 0.15s ease; }
  .chevron.open { transform: rotate(180deg); }
  .config-path {
    font-size: 0.68rem; color: var(--text-muted);
    white-space: nowrap; overflow: hidden; text-overflow: ellipsis; display: block; max-width: 100%;
  }
  .tag { font-size: 0.65rem; padding: 0.1rem 0.5rem; border-radius: 999px; white-space: nowrap; }
  .tag.gray { background: rgba(148,163,184,0.15); color: #cbd5e1; }
  .tag.red { background: rgba(248,113,113,0.15); color: #f87171; }
  .tag.green { background: rgba(74,222,128,0.15); color: #4ade80; }
  .tag.yellow { background: rgba(251,191,36,0.15); color: #fbbf24; }
  .change-list { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 0.25rem; }
  .change { display: flex; align-items: baseline; gap: 0.5rem; font-size: 0.76rem; min-width: 0; }
  .change-action { font-size: 0.65rem; padding: 0.05rem 0.45rem; border-radius: 999px; white-space: nowrap; flex-shrink: 0; }
  .action-add .change-action { background: rgba(74,222,128,0.15); color: #4ade80; }
  .action-update .change-action { background: rgba(251,191,36,0.15); color: #fbbf24; }
  .action-remove .change-action { background: rgba(248,113,113,0.15); color: #f87171; }
  .action-skip .change-action { background: rgba(148,163,184,0.15); color: #cbd5e1; }
  .action-skip { opacity: 0.7; }
  .change-name { font-family: monospace; }
  .change-detail { color: var(--text-muted); font-size: 0.7rem; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .unchanged-note { font-size: 0.72rem; color: var(--text-muted); }
  .panel-actions { display: flex; justify-content: flex-end; gap: 0.5rem; }

  /* 扫描收编区 */
  .scan-panel {
    background: rgba(0,0,0,0.15); border: 1px solid rgba(255,255,255,0.1);
    border-radius: 0.5rem; padding: 0.75rem; display: flex; flex-direction: column; gap: 0.6rem;
  }
  .scan-head { display: flex; align-items: center; justify-content: space-between; gap: 0.5rem; }
  .scan-title { font-size: 0.8rem; font-weight: 600; color: var(--text-primary); }
  .scan-group { display: flex; flex-direction: column; gap: 0.3rem; }
  .scan-agent { display: flex; align-items: center; gap: 0.5rem; }
  .scan-row {
    display: flex; align-items: center; gap: 0.5rem; font-size: 0.78rem; cursor: pointer;
    padding: 0.3rem 0.5rem; border-radius: 0.375rem; background: var(--bg-primary); min-width: 0;
  }
  .scan-row input:disabled { cursor: default; }
  /* 全选行:与普通行区分,底部细线分隔 */
  .select-all-row {
    background: transparent; border-bottom: 1px solid rgba(255,255,255,0.08);
    border-radius: 0; padding-bottom: 0.45rem;
  }
  .select-all-row .scan-name { color: var(--neon-cyan); }
  .scan-name { font-weight: 600; white-space: nowrap; }
  .scan-desc { color: var(--text-muted); font-size: 0.72rem; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; flex: 1; min-width: 0; }
  .relink-badge {
    font-size: 0.62rem; padding: 0.05rem 0.45rem; border-radius: 999px; white-space: nowrap;
    background: rgba(0,245,255,0.1); color: var(--neon-cyan);
  }
  .adopt-ok { font-size: 0.72rem; color: #4ade80; white-space: nowrap; }
  .adopt-fail { font-size: 0.72rem; color: #f87171; }
  .scan-foot { display: flex; justify-content: flex-end; }

  /* 从仓库安装:精选源卡片 + repo 输入行 + 安全提示 */
  .featured-row { display: flex; gap: 0.5rem; flex-wrap: wrap; }
  .source-card {
    flex: 1; min-width: 180px; max-width: 320px; text-align: left; cursor: pointer;
    display: flex; flex-direction: column; gap: 0.2rem;
    background: var(--bg-primary); border: 1px solid rgba(255,255,255,0.1);
    border-radius: 0.5rem; padding: 0.55rem 0.7rem; color: var(--text-primary);
  }
  .source-card:hover { border-color: rgba(0,245,255,0.4); }
  .source-card.sel { border-color: var(--neon-cyan); background: rgba(0,245,255,0.06); }
  .source-card:disabled { opacity: 0.5; cursor: default; }
  .source-name { font-weight: 600; font-size: 0.8rem; }
  .source-repo { font-family: monospace; font-size: 0.68rem; color: var(--neon-cyan); }
  .source-desc { font-size: 0.7rem; color: var(--text-muted); }
  .repo-row { display: flex; gap: 0.5rem; align-items: center; }
  .repo-input { flex: 1; min-width: 0; }
  .muted-row { opacity: 0.55; }
  .security-note { margin: 0; font-size: 0.7rem; color: #fbbf24; }

  /* 来源徽章 / 更新徽章 / 全最新提示 */
  .source-badge {
    font-size: 0.62rem; font-family: monospace; padding: 0.05rem 0.45rem; border-radius: 999px;
    background: rgba(0,245,255,0.1); color: var(--neon-cyan); white-space: nowrap; cursor: help;
    margin-left: 0.4rem;
  }
  .update-badge {
    font-size: 0.62rem; padding: 0.05rem 0.45rem; border-radius: 999px; white-space: nowrap;
    background: rgba(251,146,60,0.15); color: #fb923c; margin-left: 0.4rem;
  }
  .uptodate-note { margin: 0; font-size: 0.72rem; color: #4ade80; }

  /* 统一指令记忆 */
  .memory-editor {
    width: 100%; min-height: 220px; box-sizing: border-box; resize: vertical;
    background: var(--bg-primary); border: 1px solid rgba(255,255,255,0.1); border-radius: 0.5rem;
    color: var(--text-primary); font-family: monospace; font-size: 0.8rem; line-height: 1.55;
    padding: 0.7rem 0.85rem; outline: none;
  }
  .memory-editor:focus { border-color: var(--neon-cyan); }
  .memory-actions { display: flex; align-items: center; gap: 0.5rem; }
  .memory-actions .spacer { flex: 1; }
  .dirty-note { font-size: 0.72rem; color: #fbbf24; }
  .target-content {
    margin: 0; max-height: 260px; overflow: auto;
    background: var(--bg-primary); border: 1px solid rgba(255,255,255,0.08); border-radius: 0.4rem;
    padding: 0.6rem 0.75rem; font-size: 0.72rem; line-height: 1.5; white-space: pre-wrap;
    color: var(--text-secondary);
  }
  .native-memory { border: 1px solid rgba(255,255,255,0.08); border-radius: 0.5rem; padding: 0.6rem 0.75rem; }
  .native-memory summary {
    cursor: pointer; font-size: 0.8rem; color: var(--text-secondary); font-weight: 600;
    user-select: none;
  }
  .native-memory[open] summary { margin-bottom: 0.6rem; }
</style>