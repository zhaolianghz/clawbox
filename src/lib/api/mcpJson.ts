import type { McpServerSpec } from './mcpSync';

/** 解析结果:一个命名的服务器 */
export interface NamedServer {
  name: string;
  spec: McpServerSpec;
}

/** 结构化解析错误(UI 层负责映射为 i18n 文案) */
export type ParseIssue =
  | { code: 'invalidJson'; detail: string }
  | { code: 'notObject' }
  | { code: 'bareSpec' }
  | { code: 'noServers' }
  | { code: 'emptyName' }
  | { code: 'notServer'; name: string }
  | { code: 'missingCommand'; name: string }
  | { code: 'missingUrl'; name: string };

export type ParseResult =
  | { ok: true; servers: NamedServer[] }
  | { ok: false; issues: ParseIssue[] };

/** JSON 模式 textarea 的示例片段(JSON 语法本身语言中立,不进 i18n) */
export const MCP_JSON_EXAMPLE = `{
  "mcpServers": {
    "my-server": {
      "command": "npx",
      "args": ["-y", "some-mcp-package"]
    }
  }
}`;

/** type/transport 取这些值时归为 http */
const HTTP_TYPES = new Set(['http', 'sse', 'streamable-http', 'streamable_http', 'streamablehttp']);

function isPlainObject(v: unknown): v is Record<string, unknown> {
  return typeof v === 'object' && v !== null && !Array.isArray(v);
}

function asStringArray(v: unknown): string[] {
  if (!Array.isArray(v)) return [];
  return v.filter((x) => x != null).map((x) => (typeof x === 'string' ? x : String(x)));
}

function asStringRecord(v: unknown): Record<string, string> {
  if (!isPlainObject(v)) return {};
  const out: Record<string, string> = {};
  for (const [k, val] of Object.entries(v)) {
    if (typeof val === 'string') out[k] = val;
    else if (typeof val === 'number' || typeof val === 'boolean') out[k] = String(val);
  }
  return out;
}

/**
 * 把一个宽松的 server 对象归一化为 McpServerSpec(不做必填校验):
 * - kind:`type`/`transport` 为 http/sse/streamable-http → http;显式 stdio → stdio;
 *   否则有 url 无 command 推断 http,其余 stdio
 * - `env` 或 `environment` 二选一;`disabled: true` / `enabled: false` → enabled: false
 * - 未知键忽略
 */
export function normalizeSpec(raw: Record<string, unknown>): McpServerSpec {
  const typeRaw = raw.type ?? raw.transport;
  const typeStr = typeof typeRaw === 'string' ? typeRaw.toLowerCase() : '';
  const command = typeof raw.command === 'string' ? raw.command : null;
  const url = typeof raw.url === 'string' ? raw.url : null;

  let kind: 'stdio' | 'http';
  if (HTTP_TYPES.has(typeStr)) kind = 'http';
  else if (typeStr === 'stdio') kind = 'stdio';
  else if (url && !command) kind = 'http';
  else kind = 'stdio';

  const enabled = raw.disabled === true ? false : raw.enabled !== false;

  if (kind === 'http') {
    return { kind, command: null, args: [], env: {}, url, headers: asStringRecord(raw.headers), enabled };
  }
  return {
    kind,
    command,
    args: asStringArray(raw.args),
    env: asStringRecord(raw.env ?? raw.environment),
    url: null,
    headers: {},
    enabled,
  };
}

/**
 * 解析用户粘贴的 MCP JSON 片段,兼容:
 * 1. `{"mcpServers": {"name": {...}}}`(Claude Desktop / 各家 README 常见形态)
 * 2. `{"name": {"command": ...}, ...}` 裸 map
 * 3. `{"command": ...}` 单个无名裸 spec → 报错引导包一层名字
 */
export function parseMcpJson(text: string): ParseResult {
  let data: unknown;
  try {
    data = JSON.parse(text);
  } catch (e) {
    return { ok: false, issues: [{ code: 'invalidJson', detail: e instanceof Error ? e.message : String(e) }] };
  }
  if (!isPlainObject(data)) {
    return { ok: false, issues: [{ code: 'notObject' }] };
  }

  let map: Record<string, unknown> = data;
  if ('mcpServers' in data) {
    if (!isPlainObject(data.mcpServers)) {
      return { ok: false, issues: [{ code: 'notObject' }] };
    }
    map = data.mcpServers;
  } else if (typeof data.command === 'string' || typeof data.url === 'string') {
    // 顶层直接就是一个 spec(有 command/url 字符串字段),没有名字
    return { ok: false, issues: [{ code: 'bareSpec' }] };
  }

  const entries = Object.entries(map);
  if (entries.length === 0) {
    return { ok: false, issues: [{ code: 'noServers' }] };
  }

  const issues: ParseIssue[] = [];
  const servers: NamedServer[] = [];
  for (const [rawName, rawSpec] of entries) {
    const name = rawName.trim();
    if (!name) {
      issues.push({ code: 'emptyName' });
      continue;
    }
    if (!isPlainObject(rawSpec)) {
      issues.push({ code: 'notServer', name });
      continue;
    }
    const spec = normalizeSpec(rawSpec);
    if (spec.kind === 'stdio' && !(spec.command ?? '').trim()) {
      issues.push({ code: 'missingCommand', name });
      continue;
    }
    if (spec.kind === 'http' && !(spec.url ?? '').trim()) {
      issues.push({ code: 'missingUrl', name });
      continue;
    }
    servers.push({ name, spec });
  }

  if (issues.length > 0) return { ok: false, issues };
  return { ok: true, servers };
}

/** spec → canonical JSON 对象(省略空字段;enabled 仅在 false 时输出;可被 parseMcpJson 无损读回) */
export function specToJsonObject(spec: McpServerSpec): Record<string, unknown> {
  const out: Record<string, unknown> = {};
  if (spec.kind === 'http') {
    out.type = 'http';
    out.url = spec.url ?? '';
    if (Object.keys(spec.headers).length > 0) out.headers = spec.headers;
  } else {
    out.command = spec.command ?? '';
    if (spec.args.length > 0) out.args = spec.args;
    if (Object.keys(spec.env).length > 0) out.env = spec.env;
  }
  if (!spec.enabled) out.enabled = false;
  return out;
}

/** 序列化为 `{"<name>": {spec}}` 形式的美化 JSON(编辑预填 / 表单切 JSON 用) */
export function serializeServers(servers: NamedServer[]): string {
  const map: Record<string, unknown> = {};
  for (const { name, spec } of servers) {
    map[name] = specToJsonObject(spec);
  }
  return JSON.stringify(map, null, 2);
}
