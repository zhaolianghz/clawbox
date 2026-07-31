// 精选技能源目录:内置的可信 Git 技能仓库,安装区按分类展示、一键填入。
// 每条均经联网核实真实存在且直接包含 SKILL.md 技能目录(链接聚合类
// awesome-* 仓库不收——它们不含可安装的技能内容),且 SKILL.md 位于
// 后端发现深度(DISCOVER_MAX_DEPTH=4)以内。description 内嵌双语。
// 最近核实:2026-07(GitHub API 确认存在 + git tree 抽查 SKILL.md 路径)。

import type { Localized } from './localized';

/** 技能源分类;label 的 i18n 键为 capabilities.skillsSync.categories.{id} */
export type SkillCategory = 'official' | 'dev' | 'web' | 'ops' | 'content' | 'research';

/** 分类展示顺序 */
export const SKILL_CATEGORIES: SkillCategory[] = [
  'official',
  'dev',
  'web',
  'ops',
  'content',
  'research',
];

export interface SkillSourceEntry {
  id: string;
  name: string;
  /** owner/repo 简写(skills_repo_discover 同样接受完整 URL) */
  repo: string;
  category: SkillCategory;
  description: Localized;
}

export const SKILL_SOURCES: SkillSourceEntry[] = [
  // —— 官方 ——
  {
    id: 'anthropic-skills',
    name: 'Anthropic Skills',
    repo: 'anthropics/skills',
    category: 'official',
    description: {
      en: "Anthropic's official skill library: docx/pdf/pptx/xlsx document skills, skill-creator, and more",
      zh: 'Anthropic 官方技能库:docx/pdf/pptx/xlsx 文档处理、skill-creator 等生产级技能',
    },
  },
  // —— 软件开发 ——
  {
    id: 'superpowers',
    name: 'Superpowers',
    repo: 'obra/superpowers',
    category: 'dev',
    description: {
      en: 'Well-known community skill framework: 20+ TDD, debugging, and collaboration skills, multi-agent compatible',
      zh: '知名社区技能框架:TDD、调试、协作等 20+ 软件开发方法论技能,多 agent 兼容',
    },
  },
  {
    id: 'addyosmani-agent-skills',
    name: 'Addy Osmani Agent Skills',
    repo: 'addyosmani/agent-skills',
    category: 'dev',
    description: {
      en: 'Production-grade engineering skills by Addy Osmani: code review, CI/CD, API design, context engineering',
      zh: 'Addy Osmani 出品的工程化技能:代码评审、CI/CD、API 设计、上下文工程等',
    },
  },
  {
    id: 'jeffallan-claude-skills',
    name: 'Full-Stack Skills',
    repo: 'Jeffallan/claude-skills',
    category: 'dev',
    description: {
      en: '66 specialized full-stack developer skills: language experts, architecture, testing, DevOps roles',
      zh: '66 个全栈开发专项技能:各语言专家、架构设计、测试、DevOps 等角色技能',
    },
  },
  // —— Web / 前端 ——
  {
    id: 'vercel-agent-skills',
    name: 'Vercel Agent Skills',
    repo: 'vercel-labs/agent-skills',
    category: 'web',
    description: {
      en: "Vercel's official skills: React/Next.js best practices, composition patterns, deploy to Vercel",
      zh: 'Vercel 官方技能:React/Next.js 最佳实践、组合模式、Vercel 部署',
    },
  },
  {
    id: 'supabase-agent-skills',
    name: 'Supabase Agent Skills',
    repo: 'supabase/agent-skills',
    category: 'web',
    description: {
      en: "Supabase's official skills: Postgres best practices and Supabase platform development",
      zh: 'Supabase 官方技能:Postgres 最佳实践与 Supabase 平台开发',
    },
  },
  // —— 基础设施 / DevOps ——
  {
    id: 'hashicorp-agent-skills',
    name: 'HashiCorp Agent Skills',
    repo: 'hashicorp/agent-skills',
    category: 'ops',
    description: {
      en: "HashiCorp's official skills for Terraform and Packer: code generation, module authoring, image builds",
      zh: 'HashiCorp 官方技能:Terraform/Packer 代码生成、模块编写、镜像构建',
    },
  },
  // —— 内容创作 ——
  {
    id: 'humanizer',
    name: 'Humanizer',
    repo: 'blader/humanizer',
    category: 'content',
    description: {
      en: 'Removes signs of AI-generated writing from text — makes agent output read naturally',
      zh: '去除文本中的 AI 生成痕迹,让 agent 输出读起来更自然',
    },
  },
  {
    id: 'baoyu-skills',
    name: '宝玉 Skills',
    repo: 'JimLiu/baoyu-skills',
    category: 'content',
    description: {
      en: 'Content-creation skills by Baoyu: article illustration, comics, cover images, translation and more',
      zh: '宝玉出品的内容创作技能:文章配图、漫画、封面图、翻译等',
    },
  },
  // —— 研究 / 知识管理 ——
  {
    id: 'last30days',
    name: 'Last 30 Days',
    repo: 'mvanhorn/last30days-skill',
    category: 'research',
    description: {
      en: 'Research any topic across Reddit, X, YouTube, HN and the web from the last 30 days',
      zh: '横跨 Reddit、X、YouTube、HN 与全网,调研任意话题最近 30 天的动态',
    },
  },
  {
    id: 'obsidian-skills',
    name: 'Obsidian Skills',
    repo: 'kepano/obsidian-skills',
    category: 'research',
    description: {
      en: 'Official Obsidian skills: teach your agent Obsidian CLI, Markdown vaults, JSON Canvas',
      zh: 'Obsidian 官方技能:让 agent 掌握 Obsidian CLI、Markdown 库与 JSON Canvas',
    },
  },
  {
    id: 'scientific-agent-skills',
    name: 'Scientific Agent Skills',
    repo: 'K-Dense-AI/scientific-agent-skills',
    category: 'research',
    description: {
      en: 'Turn any agent into an AI scientist: 150+ skills for bioinformatics, chemistry, data analysis',
      zh: '把 agent 变成 AI 科学家:生物信息、化学、数据分析等 150+ 科研技能',
    },
  },
];
