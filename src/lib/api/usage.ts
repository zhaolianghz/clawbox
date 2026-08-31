import { invoke } from '@tauri-apps/api/core';

/** 单口径用量(input/cache_read/cache_creation/output/events)。 */
export interface UsageTotals {
  input: number;
  cache_read: number;
  cache_creation: number;
  output: number;
  events: number;
  /** 折算成本(USD)。null = model 不在价表里;Some(0) = 官方免费。 */
  cost_usd: number | null;
}

/** 价格表快照元信息,前端展示 stale banner 用。 */
export interface PricingMeta {
  /** 价表快照日期(YYYY-MM-DD) */
  snapshot_date: string;
  /** 当前距快照多少天 */
  age_days: number;
  /** 距 stale 还有多少天(可负,负数表示已 stale) */
  days_until_stale: number;
  /** 当前是否已 stale */
  is_stale: boolean;
  /** 覆盖 model 数(在价表里有官方价) */
  covered_models: number;
}

/** 一个 model 的用量。 */
export interface ModelUsage {
  model: string;
  totals: UsageTotals;
  events: number;
}

/** 一个 agent 的用量(含 by_model)。 */
export interface AgentUsage {
  agent_id: string;
  totals: UsageTotals;
  by_model: ModelUsage[];
}

/** 单日用量(每 agent 一行)。 */
export interface DayUsage {
  date: string;
  totals: UsageTotals;
  by_agent: AgentUsage[];
}

/** 解析健康状态(来自各 adapter 的 ParseStats 累计)。 */
export interface ParseHealth {
  matched_ratio: number;
  last_scan_at: string | null;
}

/** 顶层 summary。 */
export interface UsageSummary {
  total: UsageTotals;
  by_day: DayUsage[];
  by_agent: AgentUsage[];
  parse_health: ParseHealth;
  window_days: number;
  /** 价表快照元信息(banner / stale 提示)。缺省 = 后端还没填。 */
  pricing_meta?: PricingMeta;
}

/** 单次刷新报告(对应 aggregate::RefreshReport 的核心字段)。 */
export interface UsageRefreshReport {
  added_events: number;
  added_buckets: number;
  scanned_at: string;
}

/** Provider 视角的用量(同名 provider 跨 agent 求和)。 */
export interface ProviderUsage {
  provider_name: string;
  totals: UsageTotals;
}

/** 取最近 N 天汇总;windowDays 默认 30。 */
export function usageSummary(windowDays: number = 30): Promise<UsageSummary> {
  return invoke<UsageSummary>('usage_summary', { windowDays });
}

/** 触发一次扫描,把本地 JSONL 增量解析并落月桶。 */
export function usageRefresh(): Promise<UsageRefreshReport> {
  return invoke<UsageRefreshReport>('usage_refresh');
}

/** 按 provider 名汇总(同一 provider 被多个 agent 共享时按当时 binding 分摊)。 */
export function usageProviderSummary(): Promise<ProviderUsage[]> {
  return invoke<ProviderUsage[]>('usage_provider_summary');
}
