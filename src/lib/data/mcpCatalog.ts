// MCP 精选目录:内置的常用 MCP server,添加面板一键预填配置。
// 每条均经联网核实(npm 包 / 官方文档真实存在);宁缺毋滥。
// id 同时作为建议的 server 名(用于「已添加」判定)。description 内嵌双语。

import type { Localized } from './localized';

export interface McpCatalogEntry {
  id: string;
  name: string;
  description: Localized;
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
];
