import { invoke } from '@tauri-apps/api/core';
import type { AgentPlan, ApplyResult } from './mcpSync';

// 技能统一同步:技能库真源 = ~/.agents/skills/ 共享目录,软链下发到各 agent。
// 计划/应用复用 MCP/服务商同款 AgentPlan / ApplyResult(sync/mod.rs,snake_case)。
export type { AgentPlan, ApplyResult, ChangeItem } from './mcpSync';

/** 技能的安装来源(Git 仓库追踪;手动导入/收编的技能为 null) */
export interface SkillSource {
  repo: string;
  subdir: string;
  commit: string;
  installed_at: string;
}

/** 技能库条目 */
export interface SkillEntry {
  name: string;
  description: string;
  path: string;
  source: SkillSource | null;
}

/** 仓库发现出的单个技能 */
export interface DiscoveredSkill {
  name: string;
  description: string;
  subdir: string;
  in_library: boolean;
}

/** skills_repo_discover 的结果:规范化 repo + 当前 commit + 技能列表 */
export interface RepoDiscovery {
  repo: string;
  commit: string;
  skills: DiscoveredSkill[];
}

/** 单条安装/更新结果 */
export interface InstallOutcome {
  name: string;
  ok: boolean;
  detail: string;
}

/** 单个库技能的更新检查结果 */
export interface SkillUpdateInfo {
  name: string;
  repo: string;
  current_commit: string;
  latest_commit: string;
  has_update: boolean;
  /** 技能已从源仓库移除 */
  missing: boolean;
}

/** 扫描出的存量技能(某 agent 目录里、尚未纳入统一管理的技能) */
export interface AdoptCandidate {
  agent_id: string;
  name: string;
  description: string;
  path: string;
  /** 同名技能已在库中:收编只替换软链,不复制内容 */
  in_library: boolean;
}

/** 单条收编结果 */
export interface AdoptOutcome {
  agent_id: string;
  name: string;
  ok: boolean;
  detail: string;
}

/** 技能库列表(~/.agents/skills/ 下的技能) */
export function skills_library_list(): Promise<SkillEntry[]> {
  return invoke<SkillEntry[]>('skills_library_list');
}

/** 从本地目录导入一个技能到库 */
export function skills_import(srcDir: string): Promise<SkillEntry> {
  return invoke<SkillEntry>('skills_import', { srcDir });
}

/** 从库中删除技能 */
export function skills_library_remove(name: string): Promise<void> {
  return invoke<void>('skills_library_remove', { name });
}

/** 扫描各 agent 的存量技能,返回可收编候选 */
export function skills_scan(): Promise<AdoptCandidate[]> {
  return invoke<AdoptCandidate[]>('skills_scan');
}

/** 收编勾选的存量技能(复制进库 + 原位换软链);逐条汇报 */
export function skills_adopt(items: { agent_id: string; name: string }[]): Promise<AdoptOutcome[]> {
  return invoke<AdoptOutcome[]>('skills_adopt', { items });
}

/** 计算技能同步到各 agent 的计划(只读) */
export function sync_skills_plan(): Promise<AgentPlan[]> {
  return invoke<AgentPlan[]>('sync_skills_plan');
}

/** 对选中 agent 应用技能同步(软链下发) */
export function sync_skills_apply(agentIds: string[]): Promise<ApplyResult[]> {
  return invoke<ApplyResult[]>('sync_skills_apply', { agentIds });
}

/** 克隆并解析仓库中的技能(repo 支持 owner/repo 简写或完整 URL) */
export function skills_repo_discover(repo: string): Promise<RepoDiscovery> {
  return invoke<RepoDiscovery>('skills_repo_discover', { repo });
}

/** 安装仓库中勾选的技能子目录;逐条汇报 */
export function skills_repo_install(repo: string, subdirs: string[]): Promise<InstallOutcome[]> {
  return invoke<InstallOutcome[]>('skills_repo_install', { repo, subdirs });
}

/** 对所有带来源的库技能检查上游更新 */
export function skills_check_updates(): Promise<SkillUpdateInfo[]> {
  return invoke<SkillUpdateInfo[]>('skills_check_updates');
}

/** 更新指定技能到源仓库最新 commit;逐条汇报 */
export function skills_update(names: string[]): Promise<InstallOutcome[]> {
  return invoke<InstallOutcome[]>('skills_update', { names });
}
