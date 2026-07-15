<script lang="ts">
  import { _ } from 'svelte-i18n';
  import { onMount } from 'svelte';
  import { open } from '@tauri-apps/plugin-dialog';
  import {
    acp_list_adapters, acp_install_adapter, review_run, review_list,
    type AdapterInfo, type ReviewReport, type ReviewScope, type RoleAssignment,
  } from '$lib/api/acp';

  let adapters = $state<AdapterInfo[]>([]);
  let projectPath = $state('');
  let scopeKind = $state<'whole' | 'diff'>('diff');
  let diffBase = $state('main');
  let selectedReviewers = $state<Record<string, boolean>>({});
  let running = $state(false);
  let current = $state<ReviewReport | null>(null);
  let history = $state<ReviewReport[]>([]);
  let error = $state('');

  const failedMessage = $derived(
    current && current.status.state === 'failed' ? current.status.message : null
  );

  async function refresh() {
    adapters = await acp_list_adapters();
    history = await review_list();
  }

  async function pickDir() {
    const picked = await open({ directory: true, multiple: false });
    if (typeof picked === 'string') projectPath = picked;
  }

  async function install(id: string) {
    try { await acp_install_adapter(id); await refresh(); }
    catch (e) { error = e instanceof Error ? e.message : String(e); }
  }

  async function run() {
    error = '';
    const reviewers: RoleAssignment[] = adapters
      .filter((a) => a.installed && selectedReviewers[a.id])
      .map((a) => ({ adapter_id: a.id, model: null }));
    if (!projectPath || reviewers.length === 0) {
      error = $_('review.validation');
      return;
    }
    const scope: ReviewScope = scopeKind === 'whole' ? 'whole_project' : { git_diff: { base: diffBase } };
    running = true;
    current = null;
    try {
      current = await review_run(projectPath, scope, reviewers, reviewers[0]);
      history = await review_list();
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
      // The backend persists a Failed report on error — surface it in history.
      history = await review_list();
    } finally {
      running = false;
    }
  }

  onMount(refresh);
</script>

<div class="review-page">
  <h1>{$_('review.title')}</h1>

  {#if adapters.filter((a) => a.installed).length === 0}
    <div class="glass-card empty-adapters">
      <p>{$_('review.noAdapters')}</p>
      {#each adapters as a (a.id)}
        <div class="adapter-row">
          <span>{a.label}</span>
          <button class="neon-button" onclick={() => install(a.id)}>{$_('review.install')}</button>
        </div>
      {/each}
    </div>
  {:else}
    <div class="glass-card form">
      <label class="row">
        <span>{$_('review.projectPath')}</span>
        <input type="text" bind:value={projectPath} placeholder="/path/to/project" />
        <button class="neon-button" onclick={pickDir}>{$_('review.browse')}</button>
      </label>

      <div class="row">
        <span>{$_('review.scope')}</span>
        <select bind:value={scopeKind}>
          <option value="diff">{$_('review.gitDiff')}</option>
          <option value="whole">{$_('review.wholeProject')}</option>
        </select>
        {#if scopeKind === 'diff'}
          <input type="text" bind:value={diffBase} placeholder="main" />
        {/if}
      </div>

      <div class="row">
        <span>{$_('review.reviewers')}</span>
        <div class="reviewer-list">
          {#each adapters.filter((a) => a.installed) as a (a.id)}
            <label class="reviewer-chip">
              <input type="checkbox" bind:checked={selectedReviewers[a.id]} />
              {a.label}
            </label>
          {/each}
        </div>
      </div>

      {#if error}<div class="error">{error}</div>{/if}

      <button class="neon-button primary" onclick={run} disabled={running}>
        {running ? $_('review.running') : $_('review.run')}
      </button>
    </div>

    {#if current}
      <div class="glass-card report">
        <h2>
          {$_('review.summary')}
          {#if failedMessage !== null}<span class="failed-badge">failed</span>{/if}
        </h2>
        <p class="summary">{failedMessage !== null ? failedMessage : current.summary}</p>
        <h2>{$_('review.findings')} ({current.findings.length})</h2>
        {#each current.findings as f}
          <div class="finding" data-sev={f.severity}>
            <span class="sev">{f.severity}</span>
            <span class="loc">{f.file}{f.line != null ? ':' + f.line : ''}</span>
            <span class="ftitle">{f.title}</span>
            <p class="fdetail">{f.detail}</p>
          </div>
        {/each}
      </div>
    {/if}

    <div class="glass-card history">
      <h2>{$_('review.history')}</h2>
      {#if history.length === 0}
        <p class="muted">{$_('review.empty')}</p>
      {:else}
        {#each history as r (r.task_id)}
          <button class="history-item" onclick={() => (current = r)}>
            <span>
              {r.task_id}
              {#if r.status.state === 'failed'}<span class="failed-badge">failed</span>{/if}
            </span>
            <span class="muted">{r.findings.length} findings · {new Date(r.created_at * 1000).toLocaleString()}</span>
          </button>
        {/each}
      {/if}
    </div>
  {/if}
</div>

<style>
  .review-page { max-width: 900px; margin: 0 auto; }
  .review-page h1 { color: var(--neon-cyan); text-shadow: var(--glow-cyan); margin-bottom: 1.5rem; }
  .glass-card { padding: 1.5rem; margin-bottom: 1.5rem; }
  .row { display: flex; align-items: center; gap: 0.75rem; margin-bottom: 1rem; }
  .row > span:first-child { min-width: 90px; color: var(--text-secondary); }
  .row input[type="text"], .row select {
    flex: 1; padding: 0.6rem 0.75rem; background: var(--bg-tertiary);
    border: 1px solid rgba(255,255,255,0.1); border-radius: 0.5rem; color: var(--text-primary);
  }
  .reviewer-list { display: flex; gap: 0.75rem; flex-wrap: wrap; }
  .reviewer-chip { display: flex; align-items: center; gap: 0.4rem; }
  .neon-button {
    background: var(--bg-tertiary); border: 1px solid var(--neon-cyan); color: var(--neon-cyan);
    padding: 0.6rem 1.2rem; border-radius: 0.5rem; cursor: pointer;
  }
  .neon-button.primary { background: linear-gradient(135deg, var(--neon-cyan), var(--neon-purple)); color: #001; }
  .neon-button:disabled { opacity: 0.5; cursor: not-allowed; }
  .error { color: var(--neon-pink); margin-bottom: 1rem; }
  .summary { color: var(--text-secondary); white-space: pre-wrap; }
  .finding { padding: 0.75rem 0; border-top: 1px solid rgba(255,255,255,0.08); }
  .finding .sev { font-size: 0.7rem; font-weight: 700; padding: 0.1rem 0.5rem; border-radius: 999px; margin-right: 0.5rem; }
  .finding[data-sev="error"] .sev { background: rgba(255,0,110,0.15); color: var(--neon-pink); }
  .finding[data-sev="warning"] .sev { background: rgba(255,136,0,0.15); color: var(--neon-orange); }
  .finding[data-sev="info"] .sev { background: rgba(0,245,255,0.15); color: var(--neon-cyan); }
  .failed-badge {
    font-size: 0.7rem; font-weight: 700; padding: 0.1rem 0.5rem; border-radius: 999px;
    margin-left: 0.5rem; background: rgba(255,0,110,0.15); color: var(--neon-pink);
  }
  .finding .loc { font-family: monospace; color: var(--text-muted); margin-right: 0.5rem; }
  .fdetail { margin: 0.4rem 0 0; color: var(--text-secondary); }
  .history-item {
    display: flex; justify-content: space-between; width: 100%; text-align: left;
    padding: 0.6rem; background: none; border: none; border-top: 1px solid rgba(255,255,255,0.08);
    color: var(--text-primary); cursor: pointer;
  }
  .muted { color: var(--text-muted); }
  .empty-adapters .adapter-row { display: flex; justify-content: space-between; align-items: center; padding: 0.5rem 0; }
</style>
