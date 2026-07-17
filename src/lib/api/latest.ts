// 最新版本检查 — 仅覆盖 npm 安装的 agent(registry.npmjs.org 支持 CORS,前端直查)。
// Script/DetectOnly 类无统一版本源,返回 null 表示"未知",UI 不显示徽章。
import type { AgentStatus } from './agents';

export interface LatestInfo {
  latest: string | null; // registry 最新版;null = 无法确定
  hasUpdate: boolean; // latest 且与本地已装版本不同
}

const CACHE_KEY = 'clawbox.agents.latest';
const CACHE_TTL_MS = 60 * 60 * 1000; // 1h:打开页面时缓存热则不发请求

interface CacheShape {
  at: number;
  versions: Record<string, string>; // pkg -> latest
}

function readCache(): CacheShape | null {
  try {
    const raw = localStorage.getItem(CACHE_KEY);
    if (!raw) return null;
    const parsed = JSON.parse(raw) as CacheShape;
    if (Date.now() - parsed.at > CACHE_TTL_MS) return null;
    return parsed;
  } catch {
    return null;
  }
}

function writeCache(versions: Record<string, string>) {
  try {
    localStorage.setItem(CACHE_KEY, JSON.stringify({ at: Date.now(), versions } satisfies CacheShape));
  } catch {
    /* storage 满/禁用时静默降级为无缓存 */
  }
}

/** 从 install_command 提取 npm 包名;非 npm 安装返回 null */
export function npmPackageOf(a: AgentStatus): string | null {
  const m = a.install_command?.match(/^npm install -g (?:--force )?(\S+)$/);
  return m ? m[1] : null;
}

/** 探测到的版本串可能带前后缀("codex-cli 0.131.0"、"2.1.179 (Claude Code)"),取第一个 x.y.z */
export function extractSemver(version: string | null): string | null {
  return version?.match(/\d+\.\d+\.\d+(?:[-.][\w.]+)?/)?.[0] ?? null;
}

async function fetchLatest(pkg: string, timeoutMs = 5000): Promise<string | null> {
  const ctrl = new AbortController();
  const timer = setTimeout(() => ctrl.abort(), timeoutMs);
  try {
    const res = await fetch(`https://registry.npmjs.org/${encodeURIComponent(pkg)}/latest`, {
      signal: ctrl.signal,
    });
    if (!res.ok) return null;
    const json = (await res.json()) as { version?: string };
    return json.version ?? null;
  } catch {
    return null;
  } finally {
    clearTimeout(timer);
  }
}

/**
 * 批量查最新版。返回 agent.id -> LatestInfo。
 * force=false 时优先用 1h 缓存;单个失败不影响其它(值为 null)。
 */
export async function checkLatestVersions(
  agents: AgentStatus[],
  force = false
): Promise<Record<string, LatestInfo>> {
  const npmAgents = agents
    .map((a) => ({ a, pkg: npmPackageOf(a) }))
    .filter((x): x is { a: AgentStatus; pkg: string } => x.pkg !== null);

  let versions: Record<string, string>;
  const cached = force ? null : readCache();
  if (cached && npmAgents.every(({ pkg }) => pkg in cached.versions)) {
    versions = cached.versions;
  } else {
    versions = {};
    const results = await Promise.allSettled(npmAgents.map(({ pkg }) => fetchLatest(pkg)));
    results.forEach((r, i) => {
      if (r.status === 'fulfilled' && r.value) versions[npmAgents[i].pkg] = r.value;
    });
    writeCache(versions);
  }

  const out: Record<string, LatestInfo> = {};
  for (const { a, pkg } of npmAgents) {
    const latest = versions[pkg] ?? null;
    const installed = extractSemver(a.version);
    out[a.id] = {
      latest,
      hasUpdate: latest !== null && installed !== null && latest !== installed,
    };
  }
  return out;
}
