// MCP 精选目录:内置的常用 MCP server,添加面板一键预填配置。
// 每条均经联网核实(npm 包 / 官方文档真实存在);宁缺毋滥。
// id 同时作为建议的 server 名(用于「已添加」判定)。description 内嵌双语。

import type { Localized } from './localized';

/** 市场分类;chip 过滤用。official=官方参考实现,其余按用途分。 */
export type McpCategory = 'official' | 'dev' | 'browser' | 'productivity' | 'data';

export const MCP_CATEGORIES: Array<{ id: McpCategory; label: Localized }> = [
  { id: 'official', label: { en: 'Official', zh: '官方' } },
  { id: 'dev', label: { en: 'Dev', zh: '开发' } },
  { id: 'browser', label: { en: 'Browser', zh: '浏览器' } },
  { id: 'productivity', label: { en: 'Productivity', zh: '效率' } },
  { id: 'data', label: { en: 'Data', zh: '数据' } },
];

export interface McpCatalogEntry {
  id: string;
  name: string;
  description: Localized;
  category: McpCategory;
  kind: 'stdio' | 'http';
  command?: string;
  args?: string[];
  /** 需要用户自填的键名:stdio 预填进 env 区,http 预填进 headers 区(值留空) */
  envHint?: string[];
  url?: string;
  docsUrl: string;
}

export const MCP_CATALOG: McpCatalogEntry[] = [
  {
    id: 'filesystem',
    category: 'official',
    name: 'Filesystem',
    description: {
      en: 'Official filesystem server: read/write/search local files. Last arg is the allowed directory (defaults to ~, narrow it down)',
      zh: '官方文件系统服务器:读写/搜索本地文件。最后一个参数是允许访问的目录(默认 ~,建议改窄)',
    },
    kind: 'stdio',
    command: 'npx',
    args: ['-y', '@modelcontextprotocol/server-filesystem', '~'],
    docsUrl: 'https://github.com/modelcontextprotocol/servers/tree/main/src/filesystem',
  },
  {
    id: 'memory',
    category: 'official',
    name: 'Memory',
    description: {
      en: 'Official knowledge-graph memory server: persists entities/relations/observations across sessions',
      zh: '官方知识图谱记忆服务器:跨会话保存实体/关系/观察',
    },
    kind: 'stdio',
    command: 'npx',
    args: ['-y', '@modelcontextprotocol/server-memory'],
    docsUrl: 'https://github.com/modelcontextprotocol/servers/tree/main/src/memory',
  },
  {
    id: 'sequential-thinking',
    category: 'official',
    name: 'Sequential Thinking',
    description: {
      en: 'Official structured-thinking server: step-by-step reasoning with branching and revision',
      zh: '官方结构化思考服务器:分步推理、可分支与修订',
    },
    kind: 'stdio',
    command: 'npx',
    args: ['-y', '@modelcontextprotocol/server-sequential-thinking'],
    docsUrl: 'https://github.com/modelcontextprotocol/servers/tree/main/src/sequentialthinking',
  },
  {
    id: 'everything',
    category: 'official',
    name: 'Everything',
    description: {
      en: 'Official reference server covering every MCP feature — great for debugging clients',
      zh: '官方全功能参考服务器:覆盖 MCP 全部特性,适合调试客户端',
    },
    kind: 'stdio',
    command: 'npx',
    args: ['-y', '@modelcontextprotocol/server-everything'],
    docsUrl: 'https://github.com/modelcontextprotocol/servers/tree/main/src/everything',
  },
  {
    id: 'fetch',
    category: 'official',
    name: 'Fetch',
    description: {
      en: 'Official web fetch server: fetches URLs as Markdown (Python implementation, requires uv)',
      zh: '官方网页抓取服务器:抓取 URL 并转为 Markdown(Python 实现,需已安装 uv)',
    },
    kind: 'stdio',
    command: 'uvx',
    args: ['mcp-server-fetch'],
    docsUrl: 'https://github.com/modelcontextprotocol/servers/tree/main/src/fetch',
  },
  {
    id: 'playwright',
    category: 'browser',
    name: 'Playwright',
    description: {
      en: "Microsoft's official browser automation: drives a real browser via the accessibility tree",
      zh: 'Microsoft 官方浏览器自动化:基于无障碍树驱动真实浏览器',
    },
    kind: 'stdio',
    command: 'npx',
    args: ['@playwright/mcp@latest'],
    docsUrl: 'https://github.com/microsoft/playwright-mcp',
  },
  {
    id: 'context7',
    category: 'dev',
    name: 'Context7',
    description: {
      en: 'Up-to-date library docs by Upstash: version-specific API documentation (API key optional)',
      zh: 'Upstash 实时库文档服务器:按版本拉取最新 API 文档(API key 可选)',
    },
    kind: 'stdio',
    command: 'npx',
    args: ['-y', '@upstash/context7-mcp'],
    docsUrl: 'https://github.com/upstash/context7',
  },
  {
    id: 'github',
    category: 'dev',
    name: 'GitHub',
    description: {
      en: 'GitHub official remote server: repos/issues/PRs. Set the Authorization header to "Bearer <PAT>"',
      zh: 'GitHub 官方远程服务器:仓库/issue/PR 操作。Authorization 头填 "Bearer <PAT>"',
    },
    kind: 'http',
    url: 'https://api.githubcopilot.com/mcp/',
    envHint: ['Authorization'],
    docsUrl: 'https://github.com/github/github-mcp-server',
  },
  {
    id: 'chrome-devtools',
    category: 'browser',
    name: 'Chrome DevTools',
    description: {
      en: "Google's official DevTools server: performance traces, network/console inspection, screenshots in a real Chrome",
      zh: 'Google 官方 DevTools 服务器:在真实 Chrome 中做性能 trace、网络/console 检查、截图',
    },
    kind: 'stdio',
    command: 'npx',
    args: ['-y', 'chrome-devtools-mcp@latest'],
    docsUrl: 'https://github.com/ChromeDevTools/chrome-devtools-mcp',
  },
  {
    id: 'deepwiki',
    category: 'dev',
    name: 'DeepWiki',
    description: {
      en: 'Cognition (Devin) official remote server: ask questions about any public GitHub repo. Free, no auth required',
      zh: 'Cognition(Devin)官方远程服务器:向任意 GitHub 公开仓库提问。免费,无需认证',
    },
    kind: 'http',
    url: 'https://mcp.deepwiki.com/mcp',
    docsUrl: 'https://docs.devin.ai/work-with-devin/deepwiki-mcp',
  },
  {
    id: 'supabase',
    category: 'data',
    name: 'Supabase',
    description: {
      en: 'Supabase official server: tables, SQL, logs. Read-only prefilled; requires a personal access token',
      zh: 'Supabase 官方服务器:表管理、SQL、日志。已预填只读模式;需要 personal access token',
    },
    kind: 'stdio',
    command: 'npx',
    args: ['-y', '@supabase/mcp-server-supabase@latest', '--read-only'],
    envHint: ['SUPABASE_ACCESS_TOKEN'],
    docsUrl: 'https://github.com/supabase/mcp',
  },
  {
    id: 'figma',
    category: 'productivity',
    name: 'Figma',
    description: {
      en: 'Figma official remote server: Dev Mode design context and design-to-code. Sign in via OAuth in your agent',
      zh: 'Figma 官方远程服务器:Dev Mode 设计上下文与 design-to-code。在 agent 内通过 OAuth 登录',
    },
    kind: 'http',
    url: 'https://mcp.figma.com/mcp',
    docsUrl: 'https://help.figma.com/hc/en-us/articles/32132100833559-Guide-to-the-Figma-MCP-server',
  },
  {
    id: 'sentry',
    category: 'dev',
    name: 'Sentry',
    description: {
      en: 'Sentry official remote server: query errors and analyze issues. OAuth sign-in in your agent',
      zh: 'Sentry 官方远程服务器:查询错误、分析 issue。在 agent 内通过 OAuth 登录',
    },
    kind: 'http',
    url: 'https://mcp.sentry.dev/mcp',
    docsUrl: 'https://github.com/getsentry/sentry-mcp',
  },
  {
    id: 'linear',
    category: 'productivity',
    name: 'Linear',
    description: {
      en: 'Linear official remote server: issues/projects. OAuth sign-in; a read-only endpoint /mcp/readonly also exists',
      zh: 'Linear 官方远程服务器:issue/项目管理。OAuth 登录;另有只读端点 /mcp/readonly',
    },
    kind: 'http',
    url: 'https://mcp.linear.app/mcp',
    docsUrl: 'https://linear.app/docs/mcp',
  },
  {
    id: 'notion',
    category: 'productivity',
    name: 'Notion',
    description: {
      en: 'Notion official remote server: pages/databases. OAuth only — bearer tokens are not supported',
      zh: 'Notion 官方远程服务器:页面/数据库操作。仅支持 OAuth,不支持直接填 token',
    },
    kind: 'http',
    url: 'https://mcp.notion.com/mcp',
    docsUrl: 'https://developers.notion.com/guides/mcp/get-started-with-mcp',
  },
];
