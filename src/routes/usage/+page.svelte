<script lang="ts">
  import { onMount } from 'svelte';
  import { _ } from 'svelte-i18n';
  import { usageSummary, usageRefresh, type UsageSummary, type UsageRefreshReport, type UsageTotals } from '$lib/api/usage';

  type Period = 7 | 30 | 90;
  let period = $state<Period>(30);
  let summary = $state<UsageSummary | null>(null);
  let lastRefresh = $state<UsageRefreshReport | null>(null);
  let loading = $state(false);
  let refreshing = $state(false);
  let error = $state<string | null>(null);
  const AGENT_COLORS = ['#67e8c8', '#8fa8ff', '#d6a8ff', '#f5c542', '#ff9f7a', '#8de0ed'];

  async function loadSummary(days = period) {
    loading = true; error = null;
    try { summary = await usageSummary(days); } catch (e) { error = String(e); } finally { loading = false; }
  }
  async function refresh() {
    refreshing = true; error = null;
    try { lastRefresh = await usageRefresh(); await loadSummary(); } catch (e) { error = String(e); } finally { refreshing = false; }
  }
  function setPeriod(value: Period) { period = value; loadSummary(value); }
  onMount(() => loadSummary());

  function totalTokens(t: UsageTotals) { return t.input + t.cache_read + t.cache_creation + t.output; }
  function fmtTokens(n: number): string {
    if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
    if (n >= 1_000) return `${(n / 1_000).toFixed(1)}k`;
    return String(n);
  }
  function fmtCost(c: number | null | undefined): string {
    if (c == null) return $_('usage.costUnknown');
    if (c === 0) return '$0.00'; if (c < 0.01) return '<$0.01';
    return c < 10 ? `$${c.toFixed(2)}` : c < 1000 ? `$${c.toFixed(1)}` : `$${c.toFixed(0)}`;
  }
  function relTime(iso: string | null | undefined): string {
    if (!iso) return $_('usage.never'); const t = new Date(iso).getTime(); if (Number.isNaN(t)) return iso;
    const mins = Math.floor((Date.now() - t) / 60000); if (mins < 1) return $_('usage.justNow');
    if (mins < 60) return `${mins}m ago`; if (mins < 1440) return `${Math.floor(mins / 60)}h ago`; return `${Math.floor(mins / 1440)}d ago`;
  }
  function colorForAgent(id: string) { return AGENT_COLORS[Math.max(0, (summary?.by_agent.findIndex(a => a.agent_id === id) ?? 0)) % AGENT_COLORS.length]; }

  let days = $derived(summary?.by_day.slice(-period) ?? []);
  let maxDay = $derived(Math.max(...days.map(d => totalTokens(d.totals)), 1));
  let points = $derived(days.map((d, i) => {
    const x = days.length <= 1 ? 50 : (i / (days.length - 1)) * 100;
    const y = 92 - (totalTokens(d.totals) / maxDay) * 76;
    return { ...d, x, y };
  }));
  let linePath = $derived(points.map((p, i) => `${i ? 'L' : 'M'} ${p.x} ${p.y}`).join(' '));
  let areaPath = $derived(points.length ? `${linePath} L 100 100 L 0 100 Z` : '');
  let composition = $derived(summary ? [
    { key: 'input', label: $_('usage.colInput'), value: summary.total.input },
    { key: 'cache_read', label: $_('usage.colCacheRead'), value: summary.total.cache_read },
    { key: 'cache_creation', label: $_('usage.colCacheWrite'), value: summary.total.cache_creation },
    { key: 'output', label: $_('usage.colOutput'), value: summary.total.output },
  ] : []);
  let compositionTotal = $derived(composition.reduce((sum, item) => sum + item.value, 0) || 1);
</script>

<div class="usage-page">
  <header class="page-head">
    <div><div class="eyebrow">{$_('usage.eyebrow')}</div><h1>{$_('usage.title')}</h1><p class="subtitle">{$_('usage.subtitle')}</p></div>
    <div class="head-actions">
      {#if summary?.parse_health && summary.parse_health.matched_ratio < 0.8 && summary.parse_health.matched_ratio > 0}<span class="health-badge warn" title={$_('usage.healthWarnHint')}>⚠ {$_('usage.healthWarn', { values: { ratio: Math.round(summary.parse_health.matched_ratio * 100) } })}</span>{:else if summary?.parse_health?.last_scan_at}<span class="health-badge ok">● {$_('usage.healthOk')}</span>{/if}
      <span class="last-scan">{$_('usage.synced')} {relTime(summary?.parse_health.last_scan_at)}</span>
      <button class="refresh-btn" onclick={refresh} disabled={refreshing} aria-label={$_('usage.refresh')}>{#if refreshing}<span class="spinner"></span>{/if}↻</button>
    </div>
  </header>

  {#if error}<div class="error-line" role="alert">{error}</div>{/if}
  {#if loading && !summary}<div class="loading">{$_('usage.loading')}</div>{/if}
  {#if summary?.pricing_meta}{@const pm = summary.pricing_meta}<div class="price-note {pm.is_stale ? 'stale' : ''}" role="status">{pm.is_stale ? $_('usage.staleBannerStale', { values: { age: pm.age_days, date: pm.snapshot_date } }) : $_('usage.priceVerified', { values: { date: pm.snapshot_date } })}</div>{/if}

  {#if summary}
    <section class="hero-grid">
      <div class="hero-stat"><div class="stat-label">{$_('usage.primaryMetric', { values: { days: period } })}</div><strong>{fmtTokens(totalTokens(summary.total))}</strong><span>{$_('usage.tokensUnit')}</span><div class="hero-foot">{fmtTokens(totalTokens(summary.total) / Math.max(days.length, 1))} {$_('usage.perDay')}</div></div>
      <div class="mini-stat"><span>{$_('usage.cardCost30d')}</span><strong>{fmtCost(summary.total.cost_usd)}</strong><small>{$_('usage.costUnit')}</small></div>
      <div class="mini-stat"><span>{$_('usage.cardEvents')}</span><strong>{fmtTokens(summary.total.events)}</strong><small>{$_('usage.eventsUnit')}</small></div>
    </section>

    <section class="card chart-card">
      <div class="section-head"><div><h2>{$_('usage.chartTitle')}</h2><p>{$_('usage.chartHint')}</p></div><div class="periods" role="group" aria-label={$_('usage.periodLabel')}>{#each [7, 30, 90] as value}<button class:active={period === value} onclick={() => setPeriod(value as Period)}>{value}D</button>{/each}</div></div>
      {#if points.length}<div class="chart-wrap"><div class="chart-y"><span>{fmtTokens(maxDay)}</span><span>{fmtTokens(maxDay / 2)}</span><span>0</span></div><svg class="usage-chart" viewBox="0 0 100 100" preserveAspectRatio="none" aria-label={$_('usage.chartTitle')} role="img"><defs><linearGradient id="usage-fill" x1="0" x2="0" y1="0" y2="1"><stop offset="0" stop-color="#67e8c8" stop-opacity=".28"/><stop offset="1" stop-color="#67e8c8" stop-opacity="0"/></linearGradient></defs><path d={areaPath} fill="url(#usage-fill)"/><path d={linePath} fill="none" stroke="#67e8c8" stroke-width="1.2" vector-effect="non-scaling-stroke"/>{#each points as point (point.date)}<circle cx={point.x} cy={point.y} r="1.5" fill="#101b21" stroke="#67e8c8" stroke-width="1" vector-effect="non-scaling-stroke"><title>{point.date}: {fmtTokens(totalTokens(point.totals))}</title></circle>{/each}</svg></div><div class="chart-labels">{#each points as point (point.date)}<span>{point.date.slice(5)}</span>{/each}</div>{:else}<div class="empty">{$_('usage.emptyChart')}</div>{/if}
    </section>

    <div class="lower-grid">
      <section class="card composition"><div class="section-head"><div><h2>{$_('usage.compositionTitle')}</h2><p>{$_('usage.compositionHint')}</p></div></div><div class="composition-bar">{#each composition as item}<span class={item.key} style={`width:${(item.value / compositionTotal) * 100}%`} title={`${item.label}: ${fmtTokens(item.value)}`}></span>{/each}</div><div class="composition-list">{#each composition as item}<div><span class="legend-dot {item.key}"></span><span>{item.label}</span><strong>{fmtTokens(item.value)}</strong><small>{Math.round((item.value / compositionTotal) * 100)}%</small></div>{/each}</div></section>
      <section class="card"><div class="section-head"><div><h2>{$_('usage.byAgentTitle')}</h2><p>{$_('usage.agentHint')}</p></div></div>{#if summary.by_agent.length}<div class="agent-list">{#each summary.by_agent as agent (agent.agent_id)}{@const value = totalTokens(agent.totals)}<details class="agent-row" open={summary.by_agent.length <= 2}><summary><span class="agent-name"><i style={`background:${colorForAgent(agent.agent_id)}`}></i>{agent.agent_id}</span><span class="agent-share">{Math.round((value / Math.max(totalTokens(summary.total), 1)) * 100)}%</span><strong>{fmtTokens(value)}</strong></summary><div class="agent-bar"><span style={`width:${(value / Math.max(totalTokens(summary.total), 1)) * 100}%;background:${colorForAgent(agent.agent_id)}`}></span></div><div class="agent-detail"><div class="totals-line">in {fmtTokens(agent.totals.input)} · cache {fmtTokens(agent.totals.cache_read + agent.totals.cache_creation)} · out {fmtTokens(agent.totals.output)} · {fmtCost(agent.totals.cost_usd)}</div><table class="model-table"><thead><tr><th>{$_('usage.colModel')}</th><th>{$_('usage.colInput')}</th><th>{$_('usage.colOutput')}</th><th>{$_('usage.colCost')}</th></tr></thead><tbody>{#each agent.by_model as model (model.model)}<tr><td><code>{model.model}</code></td><td>{fmtTokens(model.totals.input)}</td><td>{fmtTokens(model.totals.output)}</td><td>{fmtCost(model.totals.cost_usd)}</td></tr>{/each}</tbody></table></div></details>{/each}</div>{:else}<div class="empty">{$_('usage.emptyAgent')}</div>{/if}</section>
    </div>
    {#if lastRefresh}<div class="refresh-info">{$_('usage.refreshed', { values: { events: lastRefresh.added_events, buckets: lastRefresh.added_buckets } })}</div>{/if}
  {/if}
</div>

<style>
  .usage-page{max-width:1120px;padding:0 .5rem;color:var(--text-primary)}
  .page-head{display:flex;justify-content:space-between;align-items:flex-end;gap:1rem;margin-bottom:1.75rem}.eyebrow{font-size:.68rem;letter-spacing:.14em;text-transform:uppercase;color:#67e8c8;margin-bottom:.45rem}.page-head h1{margin:0 0 .25rem;font-size:1.7rem;font-weight:650}.subtitle{margin:0;color:var(--text-secondary);font-size:.85rem}.head-actions{display:flex;align-items:center;gap:.65rem}.last-scan{color:var(--text-secondary);font-size:.75rem}.health-badge{font-size:.72rem}.health-badge.ok{color:#67e8c8}.health-badge.warn{color:#f5c542}.refresh-btn{border:1px solid var(--border-subtle);background:var(--bg-secondary);color:var(--text-primary);width:2rem;height:2rem;border-radius:6px;cursor:pointer;font-size:1.1rem}.refresh-btn:hover{border-color:#67e8c8;color:#67e8c8}.price-note{padding:.55rem .75rem;margin-bottom:1rem;border-left:2px solid #67e8c8;background:rgba(103,232,200,.06);font-size:.75rem;color:var(--text-secondary)}.price-note.stale{border-color:#f5c542;color:#f5c542}.error-line{padding:.6rem .8rem;margin-bottom:1rem;background:rgba(255,100,100,.1);border:1px solid rgba(255,100,100,.35);border-radius:6px;color:#ff8585;font-size:.82rem}.loading,.empty{padding:2rem;text-align:center;color:var(--text-secondary)}
  .hero-grid{display:grid;grid-template-columns:2fr 1fr 1fr;gap:.75rem;margin-bottom:1rem}.hero-stat,.mini-stat,.card{background:var(--bg-secondary);border:1px solid var(--border-subtle);border-radius:8px}.hero-stat{padding:1.25rem 1.35rem;background:linear-gradient(120deg,rgba(103,232,200,.11),var(--bg-secondary) 55%)}.stat-label,.mini-stat span{display:block;color:var(--text-secondary);font-size:.7rem;letter-spacing:.06em;text-transform:uppercase}.hero-stat strong{display:inline-block;font-size:2.8rem;line-height:1.15;margin-top:.3rem;font-weight:650;letter-spacing:-.04em}.hero-stat>span{margin-left:.45rem;color:var(--text-secondary);font-size:.75rem}.hero-foot{margin-top:.5rem;color:#67e8c8;font-size:.75rem}.mini-stat{padding:1.15rem 1.1rem;display:flex;flex-direction:column;justify-content:center}.mini-stat strong{font-size:1.5rem;margin:.45rem 0 .15rem;font-variant-numeric:tabular-nums}.mini-stat small{color:var(--text-secondary);font-size:.72rem}.card{padding:1.1rem 1.2rem;margin-bottom:1rem}.section-head{display:flex;justify-content:space-between;align-items:flex-start;gap:1rem;margin-bottom:1rem}.card h2{margin:0;font-size:.95rem;font-weight:600}.section-head p{margin:.25rem 0 0;color:var(--text-secondary);font-size:.73rem}.periods{display:flex;border:1px solid var(--border-subtle);border-radius:6px;overflow:hidden}.periods button{border:0;border-right:1px solid var(--border-subtle);background:transparent;color:var(--text-secondary);padding:.35rem .65rem;font-size:.7rem;cursor:pointer}.periods button:last-child{border:0}.periods button.active{background:rgba(103,232,200,.13);color:#67e8c8}.chart-wrap{height:230px;display:flex;gap:.65rem}.chart-y{width:2.4rem;display:flex;flex-direction:column;justify-content:space-between;text-align:right;color:var(--text-secondary);font-size:.65rem;padding:0 0 .2rem}.usage-chart{width:calc(100% - 3rem);height:100%;background:repeating-linear-gradient(to bottom,transparent 0,transparent calc(50% - 1px),var(--border-subtle) 50%,transparent calc(50% + 1px));overflow:visible}.chart-labels{margin-left:3rem;display:flex;justify-content:space-between;color:var(--text-secondary);font-size:.65rem}.lower-grid{display:grid;grid-template-columns:.9fr 1.1fr;gap:0 1rem}.composition-bar{height:9px;display:flex;overflow:hidden;border-radius:99px;background:var(--bg-tertiary);margin:.7rem 0 1rem}.composition-bar span{min-width:2px}.input{background:#67e8c8}.cache_read{background:#8fa8ff}.cache_creation{background:#d6a8ff}.output{background:#f5c542}.composition-list{display:grid;gap:.65rem}.composition-list div{display:grid;grid-template-columns:auto 1fr auto auto;gap:.45rem;align-items:center;font-size:.76rem}.composition-list strong{font-variant-numeric:tabular-nums}.composition-list small{width:2.4rem;text-align:right;color:var(--text-secondary)}.legend-dot{width:7px;height:7px;border-radius:50%}.agent-list{display:flex;flex-direction:column;gap:.35rem}.agent-row{padding:.6rem .7rem;border:1px solid var(--border-subtle);border-radius:6px;background:var(--bg-tertiary)}.agent-row summary{display:grid;grid-template-columns:1fr auto auto;gap:.8rem;align-items:center;cursor:pointer;list-style:none;font-size:.78rem}.agent-row summary::-webkit-details-marker{display:none}.agent-name{display:flex;align-items:center;gap:.45rem;font-weight:550}.agent-name i{width:7px;height:7px;border-radius:50%}.agent-share{color:var(--text-secondary);font-size:.7rem}.agent-row strong{font-variant-numeric:tabular-nums;color:#67e8c8}.agent-bar{height:3px;background:var(--bg-secondary);border-radius:3px;margin:.55rem 0}.agent-bar span{display:block;height:100%;border-radius:3px}.agent-detail{border-top:1px solid var(--border-subtle);padding-top:.6rem;margin-top:.6rem}.totals-line{font-size:.7rem;color:var(--text-secondary);margin-bottom:.5rem}.model-table{width:100%;border-collapse:collapse;font-size:.72rem}.model-table th,.model-table td{padding:.3rem .35rem;text-align:right;border-bottom:1px solid var(--border-subtle);font-variant-numeric:tabular-nums}.model-table th:first-child,.model-table td:first-child{text-align:left}.model-table th{color:var(--text-secondary);font-weight:500;font-size:.65rem}.model-table code{font-size:.7rem;color:var(--text-primary);background:transparent}.refresh-info{font-size:.72rem;color:var(--text-secondary);padding:.55rem}.spinner{display:inline-block;width:10px;height:10px;border:2px solid var(--border-subtle);border-top-color:#67e8c8;border-radius:50%;animation:spin .8s linear infinite}@keyframes spin{to{transform:rotate(360deg)}}
  @media(max-width:760px){.page-head{align-items:flex-start;flex-direction:column}.head-actions{width:100%;justify-content:flex-end}.hero-grid,.lower-grid{grid-template-columns:1fr}.hero-stat strong{font-size:2.3rem}.chart-wrap{height:190px}.model-table{display:block;overflow-x:auto;white-space:nowrap}}
</style>
