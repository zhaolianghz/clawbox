import { invoke } from '@tauri-apps/api/core';

// OpenClaw-only stats. Hermes stats (insights) land in a later iteration.
// Mirrors the Rust `Stats` struct. `usage` and `health` are raw JSON from
// openclaw and may be null when the gateway is not running.
export interface Stats {
  gateway_running: boolean;
  usage: unknown | null;
  health: unknown | null;
}

export interface DashboardMetrics {
  gatewayRunning: boolean;
  totalTokens: number | null;
  totalCost: number | null;
  apiCalls: number | null;
}

export async function get_stats(days = 30): Promise<Stats> {
  try {
    return await invoke<Stats>('get_stats', { days });
  } catch {
    return { gateway_running: false, usage: null, health: null };
  }
}

// Recursively sum numeric fields whose key matches a pattern. openclaw's
// usage-cost JSON shape varies by version, so we extract defensively rather
// than assuming a fixed schema.
function sumByKey(obj: unknown, keys: string[]): number | null {
  let total = 0;
  let found = false;

  const visit = (node: unknown) => {
    if (node === null || typeof node !== 'object') return;
    if (Array.isArray(node)) {
      node.forEach(visit);
      return;
    }
    for (const [k, v] of Object.entries(node as Record<string, unknown>)) {
      if (typeof v === 'number' && keys.some((key) => k.toLowerCase().includes(key))) {
        total += v;
        found = true;
      } else if (typeof v === 'object') {
        visit(v);
      }
    }
  };

  visit(obj);
  return found ? total : null;
}

export function extractMetrics(stats: Stats): DashboardMetrics {
  return {
    gatewayRunning: stats.gateway_running,
    totalTokens: sumByKey(stats.usage, ['token']),
    totalCost: sumByKey(stats.usage, ['cost', 'usd']),
    apiCalls: sumByKey(stats.usage, ['calls', 'requests', 'count']),
  };
}
