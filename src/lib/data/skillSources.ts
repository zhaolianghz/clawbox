// 精选技能源目录:内置的可信 Git 技能仓库,安装区一键填入。
// 每条均经联网核实真实存在且直接包含 SKILL.md 技能目录(链接聚合类
// awesome-* 仓库不收——它们不含可安装的技能内容)。description 内嵌双语。

import type { Localized } from './localized';

export interface SkillSourceEntry {
  id: string;
  name: string;
  /** owner/repo 简写(skills_repo_discover 同样接受完整 URL) */
  repo: string;
  description: Localized;
}

export const SKILL_SOURCES: SkillSourceEntry[] = [
  {
    id: 'anthropic-skills',
    name: 'Anthropic Skills',
    repo: 'anthropics/skills',
    description: {
      en: "Anthropic's official skill library: docx/pdf/pptx/xlsx document skills, skill-creator, and more",
      zh: 'Anthropic 官方技能库:docx/pdf/pptx/xlsx 文档处理、skill-creator 等生产级技能',
    },
  },
  {
    id: 'superpowers',
    name: 'Superpowers',
    repo: 'obra/superpowers',
    description: {
      en: 'Well-known community skill framework: 20+ TDD, debugging, and collaboration skills, multi-agent compatible',
      zh: '知名社区技能框架:TDD、调试、协作等 20+ 软件开发方法论技能,多 agent 兼容',
    },
  },
];
