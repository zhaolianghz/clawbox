<script lang="ts">
  // /usage — Token 用量统计页(路线图 #1)。
  // 设计:四张汇总卡(今日 / 7 / 30 / 全部) + 按天堆叠柱状图(SVG 自写) +
  // 按 agent 折叠列表。空状态与降级(matched_ratio < 0.8)用顶栏徽章提示。
  import { onMount } from 'svelte';
  import { _ } from 'svelte-i18n';
  import {
    usageSummary,
    usageRefresh,
    type UsageSummary,
    type UsageRefreshReport,
    type DayUsage,
  } from '$lib/api/usage';

  let summary = $state<UsageSummary | null>(null);
  let lastRefresh = $state<UsageRefreshReport | null>(null);
  let loading = $state(false);
  let refreshing = $state(false);
  let error = $state<string | null>(null);

  async function loadSummary() {
    loading = true;
    error = null;
    try {
      summary = await usageSummary(30);
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  async function refresh() {
    refreshing = true;
    error = null;
    try {
      lastRefresh = await usageRefresh();
      await loadSummary();
    } catch (e) {
      error = String(e);
    } finally {
      refreshing = false;
    }
  }

  onMount(loadSummary);

  /** 把原始数字格式化成「1.2M」「12k」风格,前端不再依赖外部库。 */
  function fmtTokens(n: number): string {
    if (n >= 1_000_000) return (n / 1_000_000).toFixed(1) + 'M';
    if (n >= 1_000) return (n / 1_000).toFixed(1) + 'k';
    return String(n);
  }

  /** USD 成本:None → 显示 "—";< 0.01 → 显示 <$0.01;否则 2 位小数 */
  function fmtCost(c: number | null | undefined): string {
    if (c == null) return $_('usage.costUnknown');
    if (c === 0) return '$0.00';
    if (c < 0.01) return '<$0.01';
    if (c < 10) return '$' + c.toFixed(2);
    if (c < 1000) return '$' + c.toFixed(1);
    return '$' + c.toFixed(0);
  }

  /** 按口径拆分展示(input/cache_read/cache_creation/output)。 */
  function totalsLine(t: { input: number; cache_read: number; cache_creation: number; output: number }): string {
    return `in ${fmtTokens(t.input)} · cr ${fmtTokens(t.cache_read)} · cc ${fmtTokens(t.cache_creation)} · out ${fmtTokens(t.output)}`;
  }

  /** 给柱状图生成颜色 — 按 agent 索引取色,稳态硬编码而非随机(避免重渲色变)。 */
  const AGENT_COLORS = ['#00f5ff', '#ff6ec7', '#7cf08c', '#f5c542', '#c47cff', '#ff8e54'];
  function colorForAgent(agentId: string, agents: string[]): string {
    const idx = agents.indexOf(agentId);
    return AGENT_COLORS[idx >= 0 ? idx % AGENT_COLORS.length : 0];
  }

  /** SVG 堆叠柱状图 — 接收 by_day + by_agent 列表,渲染每天一柱。 */
  let chartBox = $state<{ w: number; h: number } | null>(null);
  function setupChart(node: SVGSVGElement) {
    const update = () => {
      const r = node.getBoundingClientRect();
      chartBox = { w: Math.max(320, r.width), h: 220 };
    };
    update();
    const ro = new ResizeObserver(update);
    ro.observe(node);
    return { destroy: () => ro.disconnect() };
  }

  function chartSegments(day: DayUsage, agents: string[]): Array<{ y: number; h: number; color: string; agent: string }> {
    const total = day.by_agent.reduce((s, a) => s + (a.totals.input + a.totals.cache_read + a.totals.cache_creation + a.totals.output), 0);
    if (total === 0 || !chartBox) return [];
    const usable = chartBox.h - 20;
    let y = 0;
    return day.by_agent.map((a) => {
      const v = a.totals.input + a.totals.cache_read + a.totals.cache_creation + a.totals.output;
      const h = (v / total) * usable;
      const seg = { y, h, color: colorForAgent(a.agent_id, agents), agent: a.agent_id };
      y += h;
      return seg;
    });
  }

  function relTime(iso: string | null | undefined): string {
    if (!iso) return $_('usage.never');
    const t = new Date(iso);
    if (Number.isNaN(t.getTime())) return iso;
    const diff = Date.now() - t.getTime();
    if (diff < 60_000) return $_('usage.justNow');
    const mins = Math.floor(diff / 60_000);
    if (mins < 60) return `${mins}m ago`;
    const hours = Math.floor(mins / 60);
    if (hours < 24) return `${hours}h ago`;
    const days = Math.floor(hours / 24);
    return `${days}d ago`;
  }

  // 图表所需:全部 agent 顺序(用于颜色索引)
  let agentOrder = $derived(summary ? summary.by_agent.map((a) => a.agent_id) : []);
</script>

<div class="usage-page">
  <header class="page-head">
    <div>
      <h1>{$_('usage.title')}</h1>
      <p class="subtitle">{$_('usage.subtitle')}</p>
    </div>
    <div class="head-actions">
      {#if summary?.parse_health}
        {#if summary.parse_health.matched_ratio < 0.8 && summary.parse_health.matched_ratio > 0}
          <span class="health-badge warn" title={$_('usage.healthWarnHint')}>
            ⚠ {$_('usage.healthWarn', { values: { ratio: Math.round(summary.parse_health.matched_ratio * 100) } })}
          </span>
        {:else if summary.parse_health.matched_ratio >= 0.8 && summary.parse_health.last_scan_at}
          <span class="health-badge ok">✓ {$_('usage.healthOk')}</span>
        {/if}
      {/if}
      <span class="last-scan">{relTime(summary?.parse_health.last_scan_at)}</span>
      <button class="action-btn wide primary" onclick={refresh} disabled={refreshing}>
        {#if refreshing}<span class="spinner small"></span>{/if}
        {$_('usage.refresh')}
      </button>
    </div>
  </header>

  {#if error}
    <div class="error-line">{error}</div>
  {/if}
  {#if loading && !summary}
    <div class="loading">{$_('usage.loading')}</div>
  {/if}

  <!-- 价表 stale banner(age ≤ 30 天:绿色 / 30<a<60:黄 / ≥60:红) -->
  {#if summary?.pricing_meta}
    {@const pm = summary.pricing_meta}
    {#if pm.is_stale}
      <div class="banner stale-warn" role="status">
        {$_('usage.staleBannerStale', { values: { age: pm.age_days, date: pm.snapshot_date } })}
      </div>
    {:else if pm.days_until_stale <= 7}
      <div class="banner stale-soon" role="status">
        {$_('usage.staleBannerWarn', { values: { age: pm.age_days } })}
      </div>
    {:else}
      <div class="banner stale-fresh" role="status">
        {$_('usage.staleBannerFresh', { values: { date: pm.snapshot_date, daysUntil: pm.days_until_stale } })}
      </div>
    {/if}
  {/if}

  {#if summary}
    <!-- 4 张汇总卡 -->
    <div class="stat-grid">
      <div class="stat-card">
        <div class="stat-label">{$_('usage.cardToday')}</div>
        <div class="stat-value">{fmtTokens(todayTokens(summary))}</div>
        <div class="stat-sub">{$_('usage.tokensUnit')}</div>
      </div>
      <div class="stat-card">
        <div class="stat-label">{$_('usage.card7d')}</div>
        <div class="stat-value">{fmtTokens(windowTokens(summary, 7))}</div>
        <div class="stat-sub">{$_('usage.tokensUnit')}</div>
      </div>
      <div class="stat-card">
        <div class="stat-label">{$_('usage.card30d')}</div>
        <div class="stat-value">{fmtTokens(summary.total.input + summary.total.cache_read + summary.total.cache_creation + summary.total.output)}</div>
        <div class="stat-sub">{$_('usage.tokensUnit')}</div>
      </div>
      <div class="stat-card">
        <div class="stat-label">{$_('usage.cardEvents')}</div>
        <div class="stat-value">{fmtTokens(summary.total.events)}</div>
        <div class="stat-sub">{$_('usage.eventsUnit')}</div>
      </div>
      <div class="stat-card">
        <div class="stat-label">{$_('usage.cardCost30d')}</div>
        <div class="stat-value">{fmtCost(summary.total.cost_usd)}</div>
        <div class="stat-sub">{$_('usage.costUnit')}</div>
      </div>
    </div>

    <!-- 按天堆叠柱状图 -->
    {#if summary.by_day.length > 0}
      <section class="card">
        <h2>{$_('usage.chartTitle')}</h2>
        <svg class="usage-chart" use:setupChart viewBox="0 0 {chartBox?.w ?? 600} {chartBox?.h ?? 220}" preserveAspectRatio="none">
          {#if chartBox}
            {#each summary.by_day as day, i (day.date)}
              {@const bw = chartBox.w / Math.max(summary.by_day.length, 1)}
              {@const x = i * bw}
              {#each chartSegments(day, agentOrder) as seg, si (si)}
                <rect
                  x={x + 2}
                  y={seg.y}
                  width={bw - 4}
                  height={seg.h}
                  fill={seg.color}
                  opacity="0.85"
                >
                  <title>{seg.agent}: {day.date}</title>
                </rect>
              {/each}
              <text x={x + bw / 2} y={chartBox.h - 4} text-anchor="middle" font-size="10" fill="currentColor" opacity="0.6">
                {day.date.slice(5)}
              </text>
            {/each}
          {/if}
        </svg>
        <div class="chart-legend">
          {#each agentOrder as a (a)}
            <span class="legend-item">
              <span class="legend-dot" style="background:{colorForAgent(a, agentOrder)}"></span>
              {a}
            </span>
          {/each}
        </div>
      </section>
    {:else}
      <div class="empty">{$_('usage.emptyChart')}</div>
    {/if}

    <!-- 按 agent 折叠列表 -->
    {#if summary.by_agent.length > 0}
      <section class="card">
        <h2>{$_('usage.byAgentTitle')}</h2>
        <div class="agent-list">
          {#each summary.by_agent as a (a.agent_id)}
            <details class="agent-row" open={agentOrder.length <= 3}>
              <summary>
                <span class="agent-name">{a.agent_id}</span>
                <span class="agent-total">{fmtTokens(a.totals.input + a.totals.cache_read + a.totals.cache_creation + a.totals.output)} {$_('usage.tokensUnit')}</span>
                {#if a.totals.cost_usd != null}<span class="agent-cost">{fmtCost(a.totals.cost_usd)}</span>{/if}
              </summary>
              <div class="agent-detail">
                <div class="totals-line">{totalsLine(a.totals)}</div>
                <table class="model-table">
                  <thead>
                    <tr>
                      <th>{$_('usage.colModel')}</th>
                      <th>{$_('usage.colInput')}</th>
                      <th>{$_('usage.colCacheRead')}</th>
                      <th>{$_('usage.colCacheWrite')}</th>
                      <th>{$_('usage.colOutput')}</th>
                      <th>{$_('usage.colEvents')}</th>
                      <th>{$_('usage.colCost')}</th>
                    </tr>
                  </thead>
                  <tbody>
                    {#each a.by_model as m (m.model)}
                      <tr>
                        <td><code>{m.model}</code></td>
                        <td>{fmtTokens(m.totals.input)}</td>
                        <td>{fmtTokens(m.totals.cache_read)}</td>
                        <td>{fmtTokens(m.totals.cache_creation)}</td>
                        <td>{fmtTokens(m.totals.output)}</td>
                        <td>{fmtTokens(m.events)}</td>
                        <td class="cost-cell">{fmtCost(m.totals.cost_usd)}</td>
                      </tr>
                    {/each}
                  </tbody>
                </table>
              </div>
            </details>
          {/each}
        </div>
      </section>
    {:else}
      <div class="empty">{$_('usage.emptyAgent')}</div>
    {/if}

    {#if lastRefresh}
      <div class="refresh-info">
        {$_('usage.refreshed', { values: { events: lastRefresh.added_events, buckets: lastRefresh.added_buckets } })}
      </div>
    {/if}
  {/if}
</div>

<script lang="ts" module>
  // 模块级工具:今日 / N 天窗口汇总(避免组件重渲时重复定义)
  export function todayTokens(s: UsageSummary): number {
    if (s.by_day.length === 0) return 0;
    const today = new Date().toISOString().slice(0, 10);
    const last = s.by_day[s.by_day.length - 1];
    if (last.date !== today) return 0;
    return last.totals.input + last.totals.cache_read + last.totals.cache_creation + last.totals.output;
  }

  export function windowTokens(s: UsageSummary, days: number): number {
    const cutoff = Date.now() - days * 86_400_000;
    let sum = 0;
    for (const d of s.by_day) {
      const t = new Date(d.date).getTime();
      if (t >= cutoff) sum += d.totals.input + d.totals.cache_read + d.totals.cache_creation + d.totals.output;
    }
    return sum;
  }
</script>

<style>
  .usage-page {
    max-width: 1100px;
    padding: 0 0.5rem;
  }
  /* 价表 stale banner — 编辑式克制,3 档颜色 */
  .banner {
    padding: 0.6rem 0.9rem;
    border-radius: 4px;
    font-size: 0.875rem;
    margin-bottom: 1rem;
    border-left: 3px solid currentColor;
  }
  .banner.stale-fresh {
    background: rgba(124, 240, 140, 0.08);
    color: #7cf08c;
  }
  .banner.stale-soon {
    background: rgba(245, 197, 66, 0.10);
    color: #f5c542;
  }
  .banner.stale-warn {
    background: rgba(255, 110, 199, 0.10);
    color: #ff6ec7;
  }
  /* 表格 cost 列等宽对齐 */
  td.cost-cell {
    min-width: 5.5rem;
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
  .page-head {
    display: flex;
    justify-content: space-between;
    align-items: flex-end;
    margin-bottom: 1.25rem;
    gap: 1rem;
  }
  .page-head h1 {
    margin: 0 0 0.2rem 0;
    font-size: 1.5rem;
    font-weight: 600;
    color: var(--text-primary);
  }
  .subtitle {
    margin: 0;
    font-size: 0.85rem;
    color: var(--text-secondary);
  }
  .head-actions {
    display: flex;
    gap: 0.6rem;
    align-items: center;
  }
  .last-scan {
    font-size: 0.8rem;
    color: var(--text-secondary);
  }
  .health-badge {
    font-size: 0.75rem;
    padding: 0.2rem 0.5rem;
    border-radius: 999px;
    border: 1px solid;
  }
  .health-badge.ok {
    border-color: var(--neon-cyan);
    color: var(--neon-cyan);
  }
  .health-badge.warn {
    border-color: #f5c542;
    color: #f5c542;
  }
  .stat-grid {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 0.75rem;
    margin-bottom: 1.25rem;
  }
  .stat-card {
    padding: 0.9rem 1rem;
    background: var(--bg-secondary);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
  }
  .stat-label {
    font-size: 0.72rem;
    color: var(--text-secondary);
    margin-bottom: 0.3rem;
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }
  .stat-value {
    font-size: 1.6rem;
    font-weight: 600;
    color: var(--text-primary);
  }
  .stat-sub {
    font-size: 0.7rem;
    color: var(--text-secondary);
    margin-top: 0.15rem;
  }
  .card {
    padding: 1rem 1.1rem;
    background: var(--bg-secondary);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    margin-bottom: 1rem;
  }
  .card h2 {
    margin: 0 0 0.6rem 0;
    font-size: 1rem;
    font-weight: 600;
    color: var(--text-primary);
  }
  .usage-chart {
    width: 100%;
    height: 220px;
    display: block;
  }
  .chart-legend {
    display: flex;
    flex-wrap: wrap;
    gap: 0.6rem;
    margin-top: 0.5rem;
    font-size: 0.78rem;
    color: var(--text-secondary);
  }
  .legend-item {
    display: inline-flex;
    align-items: center;
    gap: 0.3rem;
  }
  .legend-dot {
    display: inline-block;
    width: 10px;
    height: 10px;
    border-radius: 2px;
  }
  .agent-list {
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
  }
  .agent-row {
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    padding: 0.5rem 0.75rem;
    background: var(--bg-tertiary);
  }
  .agent-row summary {
    cursor: pointer;
    display: flex;
    justify-content: space-between;
    align-items: center;
    list-style: none;
  }
  .agent-row summary::-webkit-details-marker { display: none; }
  .agent-row summary::before {
    content: '▸';
    margin-right: 0.4rem;
    transition: transform 0.15s;
    color: var(--text-secondary);
  }
  .agent-row[open] summary::before {
    transform: rotate(90deg);
  }
  .agent-name {
    font-weight: 500;
    color: var(--text-primary);
  }
  .agent-total {
    font-size: 0.85rem;
    color: var(--neon-cyan);
    font-variant-numeric: tabular-nums;
  }
  .agent-detail {
    margin-top: 0.6rem;
    padding-top: 0.6rem;
    border-top: 1px solid var(--border-subtle);
  }
  .totals-line {
    font-size: 0.78rem;
    color: var(--text-secondary);
    margin-bottom: 0.4rem;
    font-variant-numeric: tabular-nums;
  }
  .model-table {
    width: 100%;
    border-collapse: collapse;
    font-size: 0.8rem;
  }
  .model-table th, .model-table td {
    padding: 0.3rem 0.5rem;
    text-align: right;
    border-bottom: 1px solid var(--border-subtle);
    font-variant-numeric: tabular-nums;
  }
  .model-table th:first-child, .model-table td:first-child {
    text-align: left;
  }
  .model-table th {
    color: var(--text-secondary);
    font-weight: 500;
    font-size: 0.72rem;
  }
  .model-table code {
    background: transparent;
    color: var(--text-primary);
    font-size: 0.78rem;
  }
  .empty {
    padding: 2rem;
    text-align: center;
    color: var(--text-secondary);
    background: var(--bg-secondary);
    border: 1px dashed var(--border-subtle);
    border-radius: var(--radius-md);
  }
  .error-line {
    padding: 0.6rem 0.8rem;
    background: rgba(255, 100, 100, 0.1);
    border: 1px solid rgba(255, 100, 100, 0.4);
    border-radius: var(--radius-md);
    color: #ff6b6b;
    font-size: 0.85rem;
    margin-bottom: 1rem;
  }
  .loading {
    padding: 2rem;
    text-align: center;
    color: var(--text-secondary);
  }
  .refresh-info {
    margin-top: 1rem;
    padding: 0.5rem 0.8rem;
    background: rgba(0, 245, 255, 0.06);
    border-radius: var(--radius-md);
    font-size: 0.78rem;
    color: var(--text-secondary);
  }
  .spinner.small {
    display: inline-block;
    width: 12px;
    height: 12px;
    border: 2px solid var(--border-subtle);
    border-top-color: var(--neon-cyan);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
    margin-right: 0.4rem;
    vertical-align: -2px;
  }
  @keyframes spin { to { transform: rotate(360deg); } }
</style>
