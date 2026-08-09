//! 技能(skills)统一同步。
//!
//! 真源 = 生态共享技能库 `~/.agents/skills/`(openclaw 生态既有约定);
//! 下发 = 在各 agent 的技能目录建指向库的**软链**(绝对路径 target)。
//! 一个技能 = `<库>/<name>/` 目录,内含 SKILL.md(YAML frontmatter,
//! description 可缺)。v1 简化:库中全部技能下发到全部支持的 agent。
//!
//! 存量收编(adopt):agent 目录里的真实技能目录 → 拷入库(库已有同名则
//! 跳过拷贝)→ 原目录整体备份 → 原位替换为指向库的软链。
//!
//! 安全铁律:
//! - 所有路径以显式 `home: &Path` 解析;技能名过 `valid_name` 防路径逃逸。
//! - skip 语义:agent 侧同名**真实目录或指向别处的软链**一律不覆盖。
//! - remove 只删我们建的链:必须是软链且 target 仍在库内,绝不 rm 真实目录。

use super::{AgentPlan, ApplyResult, ChangeItem};
use crate::commands::config::SkillSource;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Serialize, Clone, Debug)]
pub struct SkillEntry {
    pub name: String,
    pub description: String,
    pub path: String,
    /// Git 安装来源(命令层从 config.skills_sources join;文件扫描填 None)。
    pub source: Option<SkillSource>,
}

#[derive(Serialize, Clone, Debug)]
pub struct AdoptCandidate {
    pub agent_id: String,
    pub name: String,
    pub description: String,
    pub path: String,
    /// Skill already exists in library。
    pub in_library: bool,
}

#[derive(Serialize, Clone, Debug)]
pub struct AdoptOutcome {
    pub agent_id: String,
    pub name: String,
    pub ok: bool,
    pub detail: String,
}

#[derive(Deserialize, Clone, Debug)]
pub struct AdoptRequest {
    pub agent_id: String,
    pub name: String,
}

/// 支持技能同步的 agent 及其技能目录(home 相对)。
const SKILL_DIRS: [(&str, &[&str]); 4] = [
    ("claude-code", &[".claude", "skills"]),
    ("openclaw", &[".openclaw", "skills"]),
    ("opencode", &[".config", "opencode", "skills"]),
    ("hermes", &[".hermes", "skills"]),
];

/// 注册表全集,与 providers 注册表同序;不在 SKILL_DIRS 的为 unsupported。
const ALL_AGENTS: [&str; 9] = [
    "claude-code",
    "codex",
    "openclaw",
    "opencode",
    "codebuddy",
    "cursor-agent",
    "kimi",
    "qodercli",
    "hermes",
];

pub fn library_dir(home: &Path) -> PathBuf {
    home.join(".agents").join("skills")
}

pub fn agent_skills_dir(home: &Path, agent_id: &str) -> Option<PathBuf> {
    SKILL_DIRS
        .iter()
        .find(|(id, _)| *id == agent_id)
        .map(|(_, rel)| rel.iter().fold(home.to_path_buf(), |p, s| p.join(s)))
}

/// 技能名合法性:单段目录名,防 `../` 一类路径逃逸。
fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && !name.contains('/')
        && !name.contains('\\')
        && name != "."
        && name != ".."
}

/// SKILL.md frontmatter 轻量解析:第一、二个 `---` 之间的 `description:` 行。
/// name 一律以目录名为准,不读 frontmatter 的 name。
fn frontmatter_description(skill_md: &Path) -> String {
    let Ok(text) = std::fs::read_to_string(skill_md) else {
        return String::new();
    };
    let mut in_frontmatter = false;
    for line in text.lines() {
        let t = line.trim();
        if t == "---" {
            if in_frontmatter {
                break; // 第二个 --- 结束
            }
            in_frontmatter = true;
            continue;
        }
        if in_frontmatter {
            if let Some(v) = t.strip_prefix("description:") {
                return v.trim().trim_matches('"').trim_matches('\'').to_string();
            }
        }
    }
    String::new()
}

/// 目录 → SkillEntry;无 SKILL.md 返回 None。
fn read_entry(dir: &Path) -> Option<SkillEntry> {
    let md = dir.join("SKILL.md");
    if !md.is_file() {
        return None;
    }
    Some(SkillEntry {
        name: dir.file_name()?.to_string_lossy().to_string(),
        description: frontmatter_description(&md),
        path: dir.to_string_lossy().to_string(),
        source: None,
    })
}

/// 扫库:子目录含 SKILL.md 才算技能。库目录不存在 = 空库。
pub fn list_library(home: &Path) -> Result<Vec<SkillEntry>, String> {
    let lib = library_dir(home);
    if !lib.exists() {
        return Ok(vec![]);
    }
    let rd = std::fs::read_dir(&lib)
        .map_err(|e| format!("failed to read {}: {}", lib.display(), e))?;
    let mut out: Vec<SkillEntry> = rd
        .flatten()
        .filter(|e| e.path().is_dir())
        .filter_map(|e| read_entry(&e.path()))
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

/// 递归拷贝目录(软链按其指向的实体拷贝)。`.git` 一律不拷 —— 技能目录里
/// 不该有版本库,仓库安装的根技能场景尤其不能把 .git 落进库。
fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dst)
        .map_err(|e| format!("failed to create {}: {}", dst.display(), e))?;
    let rd = std::fs::read_dir(src)
        .map_err(|e| format!("failed to read {}: {}", src.display(), e))?;
    for entry in rd.flatten() {
        if entry.file_name() == ".git" {
            continue;
        }
        let from = entry.path();
        let to = dst.join(entry.file_name());
        let meta = std::fs::metadata(&from)
            .map_err(|e| format!("failed to stat {}: {}", from.display(), e))?;
        if meta.is_dir() {
            copy_dir_recursive(&from, &to)?;
        } else {
            std::fs::copy(&from, &to)
                .map_err(|e| format!("failed to copy {}: {}", from.display(), e))?;
        }
    }
    Ok(())
}

#[cfg(unix)]
fn make_symlink(target: &Path, link: &Path) -> Result<(), String> {
    std::os::unix::fs::symlink(target, link)
        .map_err(|e| format!("failed to symlink {} -> {}: {}", link.display(), target.display(), e))
}

#[cfg(not(unix))]
fn make_symlink(_target: &Path, _link: &Path) -> Result<(), String> {
    Err("skills symlink sync is not supported on this platform".to_string())
}

/// 导入外部目录进库:name = 目录名;库中同名 → Err;无 SKILL.md → Err。
pub fn import(home: &Path, src_dir: &Path) -> Result<SkillEntry, String> {
    if !src_dir.join("SKILL.md").is_file() {
        return Err(format!("{}: missing SKILL.md, not a skill directory", src_dir.display()));
    }
    let name = src_dir
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .filter(|n| valid_name(n))
        .ok_or_else(|| format!("invalid skill dir name: {}", src_dir.display()))?;
    let dst = library_dir(home).join(&name);
    if dst.exists() {
        return Err(format!("Skill already exists in library: {}", name));
    }
    copy_dir_recursive(src_dir, &dst)?;
    read_entry(&dst).ok_or_else(|| format!("import of {} produced no SKILL.md", name))
}

/// 从库删除技能目录。agent 侧残留软链由下次同步的 remove 清理。
pub fn remove_from_library(home: &Path, name: &str) -> Result<(), String> {
    if !valid_name(name) {
        return Err(format!("invalid skill name: {}", name));
    }
    let dir = library_dir(home).join(name);
    if !dir.is_dir() {
        return Err(format!("No such skill in library: {}", name));
    }
    std::fs::remove_dir_all(&dir)
        .map_err(|e| format!("failed to remove {}: {}", dir.display(), e))
}

/// 扫 4 家 agent 技能目录,列出可收编项:真实目录(跳过软链——已是链的
/// 不需要收编)且含 SKILL.md。
pub fn scan(home: &Path) -> Result<Vec<AdoptCandidate>, String> {
    let lib_names: std::collections::HashSet<String> =
        list_library(home)?.into_iter().map(|s| s.name).collect();
    let mut out = Vec::new();
    for (agent_id, _) in SKILL_DIRS {
        let dir = agent_skills_dir(home, agent_id).unwrap();
        if !dir.exists() {
            continue;
        }
        let rd = std::fs::read_dir(&dir)
            .map_err(|e| format!("failed to read {}: {}", dir.display(), e))?;
        for entry in rd.flatten() {
            let p = entry.path();
            if p.is_symlink() || !p.is_dir() {
                continue;
            }
            if let Some(e) = read_entry(&p) {
                out.push(AdoptCandidate {
                    agent_id: agent_id.to_string(),
                    in_library: lib_names.contains(&e.name),
                    name: e.name,
                    description: e.description,
                    path: e.path,
                });
            }
        }
    }
    out.sort_by(|a, b| (a.agent_id.as_str(), a.name.as_str()).cmp(&(b.agent_id.as_str(), b.name.as_str())));
    Ok(out)
}

/// 备份目录时间戳,与 `backup_target` 同格式。
pub(crate) fn backup_stamp() -> String {
    let now = time::OffsetDateTime::now_utc();
    format!(
        "{:04}{:02}{:02}-{:02}{:02}{:02}",
        now.year(),
        u8::from(now.month()),
        now.day(),
        now.hour(),
        now.minute(),
        now.second()
    )
}

/// 收编一条:拷入库(已有同名跳过)→ 原目录整体备份 → 原位换软链。
/// 返回成功摘要(备份路径)。
fn adopt_one(home: &Path, agent_id: &str, name: &str) -> Result<String, String> {
    if !valid_name(name) {
        return Err(format!("invalid skill name: {}", name));
    }
    let dir = agent_skills_dir(home, agent_id)
        .ok_or_else(|| format!("{} does not support skills sync", agent_id))?;
    let src = dir.join(name);
    let meta = src
        .symlink_metadata()
        .map_err(|_| format!("{} does not exist", src.display()))?;
    if meta.file_type().is_symlink() {
        return Err("Already a symlink; nothing to adopt".to_string());
    }
    if !meta.is_dir() {
        return Err(format!("{} is not a directory", src.display()));
    }
    if !src.join("SKILL.md").is_file() {
        return Err(format!("{} is missing SKILL.md", src.display()));
    }
    let lib_target = library_dir(home).join(name);
    if !lib_target.exists() {
        copy_dir_recursive(&src, &lib_target)?;
    }
    let backup = home
        .join(".clawbox")
        .join("backups")
        .join(backup_stamp())
        .join("skills")
        .join(format!("{}__{}", agent_id, name));
    copy_dir_recursive(&src, &backup)?;
    std::fs::remove_dir_all(&src)
        .map_err(|e| format!("failed to remove {}: {}", src.display(), e))?;
    make_symlink(&lib_target, &src)?;
    Ok(format!("backed up to {}", backup.display()))
}

/// 批量收编:单条失败(ok=false 带 detail)不影响其它条。
pub fn adopt(home: &Path, items: &[AdoptRequest]) -> Vec<AdoptOutcome> {
    items
        .iter()
        .map(|req| match adopt_one(home, &req.agent_id, &req.name) {
            Ok(detail) => AdoptOutcome {
                agent_id: req.agent_id.clone(),
                name: req.name.clone(),
                ok: true,
                detail,
            },
            Err(e) => AdoptOutcome {
                agent_id: req.agent_id.clone(),
                name: req.name.clone(),
                ok: false,
                detail: e,
            },
        })
        .collect()
}

/// agent 侧一个条目的技能同步状态。
enum LinkState {
    /// 无条目。
    Absent,
    /// 我们的链:软链且 target == 库中该技能(绝对路径)。
    Ours,
    /// 同名真实目录或指向别处的软链 —— 用户自有,绝不覆盖。
    Foreign,
}

fn link_state(at: &Path, expected_target: &Path) -> LinkState {
    match at.symlink_metadata() {
        Err(_) => LinkState::Absent,
        Ok(m) if m.file_type().is_symlink() => {
            match std::fs::read_link(at) {
                Ok(dest) if dest == expected_target => LinkState::Ours,
                _ => LinkState::Foreign,
            }
        }
        Ok(_) => LinkState::Foreign,
    }
}

/// remove 安全判定:是软链且 target 在库内(前缀)。
fn is_our_link_into_library(at: &Path, lib: &Path) -> bool {
    at.symlink_metadata()
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false)
        && std::fs::read_link(at).map(|t| t.starts_with(lib)).unwrap_or(false)
}

fn plan_one(
    home: &Path,
    dir: &Path,
    library: &[SkillEntry],
    managed: &[String],
) -> Vec<ChangeItem> {
    let lib = library_dir(home);
    let mut changes = Vec::new();
    for skill in library {
        let target = lib.join(&skill.name);
        let (action, detail) = match link_state(&dir.join(&skill.name), &target) {
            LinkState::Absent => ("add", format!("→ {}", target.display())),
            LinkState::Ours => ("unchanged", String::new()),
            LinkState::Foreign => ("skip", "A skill with this name already exists (not managed by ClawBox)".to_string()),
        };
        changes.push(ChangeItem {
            name: skill.name.clone(),
            action: action.into(),
            detail,
        });
    }
    // remove:曾建链、库中已删,且 agent 侧仍是指向库内的软链。
    for name in managed {
        if library.iter().any(|s| &s.name == name) {
            continue;
        }
        if is_our_link_into_library(&dir.join(name), &lib) {
            changes.push(ChangeItem {
                name: name.clone(),
                action: "remove".into(),
                detail: "no longer managed by ClawBox".into(),
            });
        }
    }
    changes
}

/// 为注册表全部 agent 生成技能同步计划;库读取失败落在各支持 agent 的
/// error 上,不炸整体。
pub fn plan_all(home: &Path, managed: &HashMap<String, Vec<String>>) -> Vec<AgentPlan> {
    let library = list_library(home);
    ALL_AGENTS
        .iter()
        .map(|id| {
            let agent_id = id.to_string();
            let Some(dir) = agent_skills_dir(home, id) else {
                return AgentPlan {
                    agent_id,
                    supported: false,
                    config_path: String::new(),
                    changes: vec![],
                    error: None,
                };
            };
            let config_path = dir.to_string_lossy().to_string();
            match &library {
                Err(e) => AgentPlan {
                    agent_id,
                    supported: true,
                    config_path,
                    changes: vec![],
                    error: Some(e.clone()),
                },
                Ok(lib) => {
                    let empty = vec![];
                    let m = managed.get(*id).unwrap_or(&empty);
                    AgentPlan {
                        agent_id,
                        supported: true,
                        config_path,
                        changes: plan_one(home, &dir, lib, m),
                        error: None,
                    }
                }
            }
        })
        .collect()
}

/// 对单个 agent 应用技能同步:add 建链、remove 删我们的链(带二次安全
/// 校验),skip/unchanged 不动。返回变更数。
pub fn apply_agent(home: &Path, agent_id: &str, managed: &[String]) -> Result<usize, String> {
    let dir = agent_skills_dir(home, agent_id)
        .ok_or_else(|| format!("{} skills sync is not supported", agent_id))?;
    let library = list_library(home)?;
    let lib = library_dir(home);
    let mut applied = 0;
    for skill in &library {
        let target = lib.join(&skill.name);
        let at = dir.join(&skill.name);
        if let LinkState::Absent = link_state(&at, &target) {
            std::fs::create_dir_all(&dir)
                .map_err(|e| format!("failed to create {}: {}", dir.display(), e))?;
            make_symlink(&target, &at)?;
            applied += 1;
        }
    }
    for name in managed {
        if library.iter().any(|s| &s.name == name) {
            continue;
        }
        let at = dir.join(name);
        if is_our_link_into_library(&at, &lib) {
            std::fs::remove_file(&at)
                .map_err(|e| format!("failed to remove link {}: {}", at.display(), e))?;
            applied += 1;
        }
    }
    Ok(applied)
}

/// apply 成功后应记入 skills_managed 的名字:库技能中 agent 侧现为
/// "指向库中该技能的软链"的(add 成功 + 原本 unchanged;skip 的用户自有
/// 目录不算我们管理)。
pub fn deployed_names(home: &Path, agent_id: &str) -> Vec<String> {
    let Some(dir) = agent_skills_dir(home, agent_id) else {
        return vec![];
    };
    let Ok(library) = list_library(home) else {
        return vec![];
    };
    let lib = library_dir(home);
    library
        .into_iter()
        .filter(|s| matches!(link_state(&dir.join(&s.name), &lib.join(&s.name)), LinkState::Ours))
        .map(|s| s.name)
        .collect()
}

/// 应用到一个 agent:备份(技能目录是目录,backup_target 对非文件返回
/// None,自然跳过)、应用、汇报。调用方在成功后更新 skills_managed。
pub fn apply_one(home: &Path, agent_id: &str, managed: &[String]) -> ApplyResult {
    let id = agent_id.to_string();
    let Some(dir) = agent_skills_dir(home, agent_id) else {
        return ApplyResult {
            agent_id: id,
            ok: false,
            backup_path: None,
            applied: 0,
            error: Some("agent not supported for skills sync".to_string()),
        };
    };
    let backup_path = match super::backup_target(home, agent_id, &dir) {
        Ok(p) => p,
        Err(e) => {
            return ApplyResult {
                agent_id: id,
                ok: false,
                backup_path: None,
                applied: 0,
                error: Some(e),
            }
        }
    };
    match apply_agent(home, agent_id, managed) {
        Ok(applied) => ApplyResult {
            agent_id: id,
            ok: true,
            backup_path,
            applied,
            error: None,
        },
        Err(e) => ApplyResult {
            agent_id: id,
            ok: false,
            backup_path,
            applied: 0,
            error: Some(e),
        },
    }
}

// ---- Git 仓库安装引擎:discover / install / check-updates / update ----------
//
// 安装记录(SkillSource)持久化在 Config.skills_sources,由命令层读写;
// 本模块只做 git 与文件操作,home 参数化。git 操作全程
// GIT_TERMINAL_PROMPT=0,防私有仓 clone 挂在交互提问上。

#[derive(Serialize, Clone, Debug)]
pub struct DiscoveredSkill {
    pub name: String,
    pub description: String,
    /// 仓内相对路径;根目录技能为 ""。
    pub subdir: String,
    pub in_library: bool,
}

#[derive(Serialize, Clone, Debug)]
pub struct RepoDiscovery {
    /// 归一化后的仓库 URL。
    pub repo: String,
    pub commit: String,
    pub skills: Vec<DiscoveredSkill>,
}

#[derive(Serialize, Clone, Debug)]
pub struct InstallOutcome {
    pub name: String,
    pub ok: bool,
    pub detail: String,
}

#[derive(Serialize, Clone, Debug)]
pub struct SkillUpdateInfo {
    pub name: String,
    pub repo: String,
    pub current_commit: String,
    pub latest_commit: String,
    pub has_update: bool,
    /// subdir 在仓库最新版中已消失(或不再是技能)。
    pub missing: bool,
}

/// 目录递归发现的最大深度(相对 clone 根的路径段数;根 = 0)。
const DISCOVER_MAX_DEPTH: usize = 4;

/// URL 归一化:owner/repo 简写 → https://github.com/owner/repo.git;
/// 全 URL(含 "://" 或 scp 风格 git@…)原样接受。
pub fn normalize_repo_url(input: &str) -> String {
    let s = input.trim();
    if s.contains("://") || s.starts_with("git@") {
        return s.to_string();
    }
    let parts: Vec<&str> = s.split('/').collect();
    let simple = |p: &str| {
        !p.is_empty()
            && p.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    };
    if parts.len() == 2 && parts.iter().all(|p| simple(p)) {
        let repo = parts[1].strip_suffix(".git").unwrap_or(parts[1]);
        return format!("https://github.com/{}/{}.git", parts[0], repo);
    }
    s.to_string()
}

/// 仓库 URL 尾段(去 .git)—— 根目录技能的名字。
fn repo_base_name(repo: &str) -> String {
    let tail = repo
        .trim_end_matches('/')
        .rsplit(['/', ':'])
        .next()
        .unwrap_or("skill");
    let name = tail.strip_suffix(".git").unwrap_or(tail);
    if valid_name(name) { name.to_string() } else { "skill".to_string() }
}

fn run_git(args: &[&str]) -> Result<String, String> {
    let output = crate::proc::command("git")
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                "Git is required (not found in PATH)".to_string()
            } else {
                format!("failed to run git: {}", e)
            }
        })?;
    if !output.status.success() {
        return Err(format!(
            "git {} failed: {}",
            args.first().unwrap_or(&""),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// 浅 clone 到临时目录;Drop 时清理,不留垃圾。
struct TempClone {
    path: PathBuf,
}

impl Drop for TempClone {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn clone_repo(repo: &str) -> Result<TempClone, String> {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    repo.hash(&mut h);
    let dest = std::env::temp_dir().join(format!(
        "clawbox-skill-repo-{}-{:x}",
        std::process::id(),
        h.finish()
    ));
    let _ = std::fs::remove_dir_all(&dest); // 残留则清掉,保证 HEAD 新鲜
    run_git(&["clone", "--depth", "1", repo, &dest.to_string_lossy()])?;
    Ok(TempClone { path: dest })
}

fn head_commit(clone_dir: &Path) -> Result<String, String> {
    run_git(&["-C", &clone_dir.to_string_lossy(), "rev-parse", "HEAD"])
}

/// clone 内递归找技能目录(含 SKILL.md):跳过 .git/node_modules,深度
/// ≤ DISCOVER_MAX_DEPTH;根目录本身也算(subdir="",name=仓名)。
fn discover_in_clone(clone_dir: &Path, repo: &str) -> Vec<(String, String, String)> {
    // (name, description, subdir)
    let mut found = Vec::new();
    if clone_dir.join("SKILL.md").is_file() {
        found.push((
            repo_base_name(repo),
            frontmatter_description(&clone_dir.join("SKILL.md")),
            String::new(),
        ));
    }
    fn walk(base: &Path, dir: &Path, depth: usize, out: &mut Vec<(String, String, String)>) {
        if depth > DISCOVER_MAX_DEPTH {
            return;
        }
        let Ok(rd) = std::fs::read_dir(dir) else { return };
        for entry in rd.flatten() {
            let p = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if !p.is_dir() || p.is_symlink() || name == ".git" || name == "node_modules" {
                continue;
            }
            if p.join("SKILL.md").is_file() {
                let subdir = p
                    .strip_prefix(base)
                    .map(|r| r.to_string_lossy().replace('\\', "/"))
                    .unwrap_or_default();
                out.push((name, frontmatter_description(&p.join("SKILL.md")), subdir));
                continue; // 技能目录内部不再下钻
            }
            walk(base, &p, depth + 1, out);
        }
    }
    walk(clone_dir, clone_dir, 1, &mut found);
    found.sort_by(|a, b| a.2.cmp(&b.2));
    found
}

/// 发现仓库里的技能。repo 输入接受全 URL 或 owner/repo 简写。
pub fn repo_discover(home: &Path, repo_input: &str) -> Result<RepoDiscovery, String> {
    let repo = normalize_repo_url(repo_input);
    let clone = clone_repo(&repo)?;
    let commit = head_commit(&clone.path)?;
    let lib_names: std::collections::HashSet<String> =
        list_library(home)?.into_iter().map(|s| s.name).collect();
    let skills = discover_in_clone(&clone.path, &repo)
        .into_iter()
        .map(|(name, description, subdir)| DiscoveredSkill {
            in_library: lib_names.contains(&name),
            name,
            description,
            subdir,
        })
        .collect();
    Ok(RepoDiscovery { repo, commit, skills })
}

fn now_iso8601() -> String {
    let now = time::OffsetDateTime::now_utc();
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        now.year(),
        u8::from(now.month()),
        now.day(),
        now.hour(),
        now.minute(),
        now.second()
    )
}

/// 安装选中的 subdir(""=根技能)。返回 (逐条结果, 成功技能的来源记录);
/// 来源由命令层写进 Config.skills_sources。库同名 → 该条 ok=false 不覆盖。
pub fn repo_install(
    home: &Path,
    repo_input: &str,
    subdirs: &[String],
) -> Result<(Vec<InstallOutcome>, Vec<(String, SkillSource)>), String> {
    let repo = normalize_repo_url(repo_input);
    let clone = clone_repo(&repo)?;
    let commit = head_commit(&clone.path)?;
    let mut outcomes = Vec::new();
    let mut sources = Vec::new();
    for subdir in subdirs {
        let src = if subdir.is_empty() { clone.path.clone() } else { clone.path.join(subdir) };
        let name = if subdir.is_empty() {
            repo_base_name(&repo)
        } else {
            src.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default()
        };
        let fail = |detail: String| InstallOutcome { name: name.clone(), ok: false, detail };
        if subdir.contains("..") || !valid_name(&name) {
            outcomes.push(fail(format!("invalid subdir: {}", subdir)));
            continue;
        }
        if !src.join("SKILL.md").is_file() {
            outcomes.push(fail(format!("{} is missing SKILL.md", subdir)));
            continue;
        }
        let dst = library_dir(home).join(&name);
        if dst.exists() {
            outcomes.push(fail("Skill already exists in library".to_string()));
            continue;
        }
        match copy_dir_recursive(&src, &dst) {
            Ok(()) => {
                sources.push((
                    name.clone(),
                    SkillSource {
                        repo: repo.clone(),
                        subdir: subdir.clone(),
                        commit: commit.clone(),
                        installed_at: now_iso8601(),
                    },
                ));
                outcomes.push(InstallOutcome {
                    name,
                    ok: true,
                    detail: format!("installed @ {}", &commit[..commit.len().min(12)]),
                });
            }
            Err(e) => outcomes.push(fail(e)),
        }
    }
    Ok((outcomes, sources))
}

/// 检查更新:按 repo 分组各克隆一次,HEAD 与记录的 commit 比对;subdir
/// 已消失(或不再含 SKILL.md)→ missing。
pub fn check_updates(
    sources: &HashMap<String, SkillSource>,
) -> Result<Vec<SkillUpdateInfo>, String> {
    let mut by_repo: HashMap<&str, Vec<(&String, &SkillSource)>> = HashMap::new();
    for (name, src) in sources {
        by_repo.entry(src.repo.as_str()).or_default().push((name, src));
    }
    let mut out = Vec::new();
    for (repo, group) in by_repo {
        let clone = clone_repo(repo)?;
        let latest = head_commit(&clone.path)?;
        for (name, src) in group {
            let dir = if src.subdir.is_empty() {
                clone.path.clone()
            } else {
                clone.path.join(&src.subdir)
            };
            out.push(SkillUpdateInfo {
                name: name.clone(),
                repo: repo.to_string(),
                current_commit: src.commit.clone(),
                latest_commit: latest.clone(),
                has_update: src.commit != latest,
                missing: !dir.join("SKILL.md").is_file(),
            });
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

/// 覆盖更新:现库目录备份到 ~/.clawbox/backups/<stamp>/skills-lib/<name>/,
/// 整目录替换为仓库新版。返回 (逐条结果, 更新后的来源记录)。
pub fn update(
    home: &Path,
    names: &[String],
    sources: &HashMap<String, SkillSource>,
) -> (Vec<InstallOutcome>, Vec<(String, SkillSource)>) {
    let mut outcomes = Vec::new();
    let mut updated = Vec::new();
    // 按 repo 分组,一仓一 clone。
    let mut by_repo: HashMap<String, Vec<String>> = HashMap::new();
    for name in names {
        match sources.get(name) {
            Some(src) => by_repo.entry(src.repo.clone()).or_default().push(name.clone()),
            None => outcomes.push(InstallOutcome {
                name: name.clone(),
                ok: false,
                detail: "No source record (skill was not installed from Git)".to_string(),
            }),
        }
    }
    let stamp = backup_stamp();
    for (repo, group) in by_repo {
        let clone = match clone_repo(&repo) {
            Ok(c) => c,
            Err(e) => {
                for name in group {
                    outcomes.push(InstallOutcome { name, ok: false, detail: e.clone() });
                }
                continue;
            }
        };
        let commit = match head_commit(&clone.path) {
            Ok(c) => c,
            Err(e) => {
                for name in group {
                    outcomes.push(InstallOutcome { name, ok: false, detail: e.clone() });
                }
                continue;
            }
        };
        for name in group {
            let src = &sources[&name]; // 分组来自 sources,必存在
            match update_one(home, &name, src, &clone.path, &stamp) {
                Ok(detail) => {
                    updated.push((
                        name.clone(),
                        SkillSource {
                            repo: repo.clone(),
                            subdir: src.subdir.clone(),
                            commit: commit.clone(),
                            installed_at: now_iso8601(),
                        },
                    ));
                    outcomes.push(InstallOutcome { name, ok: true, detail });
                }
                Err(e) => outcomes.push(InstallOutcome { name, ok: false, detail: e }),
            }
        }
    }
    (outcomes, updated)
}

fn update_one(
    home: &Path,
    name: &str,
    src: &SkillSource,
    clone_dir: &Path,
    stamp: &str,
) -> Result<String, String> {
    if !valid_name(name) || src.subdir.contains("..") {
        return Err(format!("invalid skill name/subdir: {}", name));
    }
    let new_dir = if src.subdir.is_empty() { clone_dir.to_path_buf() } else { clone_dir.join(&src.subdir) };
    if !new_dir.join("SKILL.md").is_file() {
        return Err(format!("Skill no longer exists in the latest repo version (subdir: {})", src.subdir));
    }
    let lib_dir = library_dir(home).join(name);
    let backup = home
        .join(".clawbox")
        .join("backups")
        .join(stamp)
        .join("skills-lib")
        .join(name);
    if lib_dir.is_dir() {
        copy_dir_recursive(&lib_dir, &backup)?;
        std::fs::remove_dir_all(&lib_dir)
            .map_err(|e| format!("failed to remove {}: {}", lib_dir.display(), e))?;
    }
    copy_dir_recursive(&new_dir, &lib_dir)?;
    Ok(format!("backed up to {}", backup.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sync::test_util::*;

    /// 在任意目录下造一个技能目录(SKILL.md + 附件)。
    fn write_skill(base: &Path, name: &str, description: &str) -> PathBuf {
        let dir = base.join(name);
        std::fs::create_dir_all(dir.join("scripts")).unwrap();
        let fm = if description.is_empty() {
            format!("---\nname: {}\n---\nbody\n", name)
        } else {
            format!("---\nname: {}\ndescription: {}\n---\nbody\n", name, description)
        };
        std::fs::write(dir.join("SKILL.md"), fm).unwrap();
        std::fs::write(dir.join("scripts").join("run.sh"), "#!/bin/sh\n").unwrap();
        dir
    }

    fn lib_skill(home: &TempHome, name: &str, description: &str) -> PathBuf {
        write_skill(&library_dir(home.path()), name, description)
    }

    fn action_map(changes: &[ChangeItem]) -> Vec<(String, String)> {
        changes.iter().map(|c| (c.name.clone(), c.action.clone())).collect()
    }

    // ---- 库列表与 frontmatter ----

    #[test]
    fn library_list_parses_frontmatter_and_ignores_non_skills() {
        let home = TempHome::new();
        lib_skill(&home, "beta", "the second skill");
        lib_skill(&home, "alpha", ""); // 无 description
        // 无 SKILL.md 的目录、散文件:都不算技能
        std::fs::create_dir_all(library_dir(home.path()).join("not-a-skill")).unwrap();
        std::fs::write(library_dir(home.path()).join("README.md"), "x").unwrap();

        let list = list_library(home.path()).unwrap();
        assert_eq!(
            list.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(),
            vec!["alpha", "beta"]
        );
        assert_eq!(list[0].description, "");
        assert_eq!(list[1].description, "the second skill");
        assert!(list[1].path.ends_with("beta"));
        // 库目录不存在 = 空库
        let empty = TempHome::new();
        assert!(list_library(empty.path()).unwrap().is_empty());
    }

    #[test]
    fn import_copies_and_rejects_duplicates_and_non_skills() {
        let home = TempHome::new();
        let src = write_skill(&home.path().join("downloads"), "newskill", "desc");
        let entry = import(home.path(), &src).unwrap();
        assert_eq!(entry.name, "newskill");
        assert_eq!(entry.description, "desc");
        assert!(library_dir(home.path()).join("newskill").join("scripts").join("run.sh").is_file());
        // 同名 → Err
        let err = import(home.path(), &src).unwrap_err();
        assert!(err.contains("already exists"), "{}", err);
        // 无 SKILL.md → Err
        let bad = home.path().join("downloads").join("junk");
        std::fs::create_dir_all(&bad).unwrap();
        let err = import(home.path(), &bad).unwrap_err();
        assert!(err.contains("SKILL.md"), "{}", err);
    }

    #[test]
    fn remove_from_library_validates_name() {
        let home = TempHome::new();
        lib_skill(&home, "gone", "");
        remove_from_library(home.path(), "gone").unwrap();
        assert!(!library_dir(home.path()).join("gone").exists());
        assert!(remove_from_library(home.path(), "gone").is_err()); // 已不存在
        assert!(remove_from_library(home.path(), "../escape").is_err());
        assert!(remove_from_library(home.path(), "").is_err());
    }

    // ---- plan / apply ----

    #[test]
    fn plan_apply_add_creates_link_then_unchanged_idempotent() {
        let home = TempHome::new();
        lib_skill(&home, "sk", "d");
        let managed = HashMap::new();
        let plans = plan_all(home.path(), &managed);
        assert_eq!(plans.len(), 9);
        let cc = plans.iter().find(|p| p.agent_id == "claude-code").unwrap();
        assert!(cc.supported);
        assert!(cc.config_path.ends_with("skills"), "{}", cc.config_path);
        assert_eq!(action_map(&cc.changes), vec![("sk".to_string(), "add".to_string())]);
        // codex 不支持
        let codex = plans.iter().find(|p| p.agent_id == "codex").unwrap();
        assert!(!codex.supported && codex.changes.is_empty());

        assert_eq!(apply_agent(home.path(), "claude-code", &[]).unwrap(), 1);
        let at = home.path().join(".claude").join("skills").join("sk");
        assert!(at.is_symlink());
        assert_eq!(
            std::fs::read_link(&at).unwrap(),
            library_dir(home.path()).join("sk")
        );
        // 软链可用:透过链能读到 SKILL.md
        assert!(at.join("SKILL.md").is_file());
        // 幂等
        let plans = plan_all(home.path(), &managed);
        let cc = plans.iter().find(|p| p.agent_id == "claude-code").unwrap();
        assert_eq!(cc.changes[0].action, "unchanged");
        assert_eq!(apply_agent(home.path(), "claude-code", &[]).unwrap(), 0);
        assert_eq!(deployed_names(home.path(), "claude-code"), vec!["sk"]);
    }

    #[test]
    fn plan_skips_foreign_dir_and_foreign_link_and_never_overwrites() {
        let home = TempHome::new();
        lib_skill(&home, "mine", "");
        // agent 侧同名真实目录(用户自有)
        write_skill(&home.path().join(".hermes").join("skills"), "mine", "user version");
        // agent 侧另一个技能是指向别处的软链
        lib_skill(&home, "other", "");
        let elsewhere = write_skill(&home.path().join("elsewhere"), "other", "");
        let hermes_dir = home.path().join(".hermes").join("skills");
        std::os::unix::fs::symlink(&elsewhere, hermes_dir.join("other")).unwrap();

        let plans = plan_all(home.path(), &HashMap::new());
        let hermes = plans.iter().find(|p| p.agent_id == "hermes").unwrap();
        assert_eq!(
            action_map(&hermes.changes),
            vec![("mine".to_string(), "skip".to_string()), ("other".to_string(), "skip".to_string())]
        );
        assert!(hermes.changes[0].detail.contains("not managed by ClawBox"));

        // apply 不动任何一个:真实目录仍是目录,别处链 target 不变
        assert_eq!(apply_agent(home.path(), "hermes", &[]).unwrap(), 0);
        assert!(!hermes_dir.join("mine").is_symlink());
        assert_eq!(std::fs::read_link(hermes_dir.join("other")).unwrap(), elsewhere);
        // skip 的不进 deployed_names
        assert!(deployed_names(home.path(), "hermes").is_empty());
    }

    #[test]
    fn remove_deletes_only_our_links_into_library() {
        let home = TempHome::new();
        lib_skill(&home, "sk", "");
        apply_agent(home.path(), "opencode", &[]).unwrap();
        let at = home.path().join(".config").join("opencode").join("skills").join("sk");
        assert!(at.is_symlink());

        // 库删掉,managed 记着 → plan remove,apply 只删链
        remove_from_library(home.path(), "sk").unwrap();
        let managed_map: HashMap<String, Vec<String>> =
            [("opencode".to_string(), vec!["sk".to_string()])].into();
        let plans = plan_all(home.path(), &managed_map);
        let oc = plans.iter().find(|p| p.agent_id == "opencode").unwrap();
        assert_eq!(action_map(&oc.changes), vec![("sk".to_string(), "remove".to_string())]);
        assert_eq!(apply_agent(home.path(), "opencode", &["sk".to_string()]).unwrap(), 1);
        assert!(!at.exists() && !at.is_symlink());

        // managed 记着但 agent 侧是真实目录 → 不删(绝不 rm 真实目录)
        let user_dir = write_skill(
            &home.path().join(".config").join("opencode").join("skills"),
            "sk",
            "user rebuilt",
        );
        assert_eq!(apply_agent(home.path(), "opencode", &["sk".to_string()]).unwrap(), 0);
        assert!(user_dir.is_dir());
        // managed 记着但链指向别处 → 不删
        let elsewhere = write_skill(&home.path().join("elsewhere"), "sk2", "");
        std::os::unix::fs::symlink(
            &elsewhere,
            home.path().join(".config").join("opencode").join("skills").join("sk2"),
        )
        .unwrap();
        assert_eq!(apply_agent(home.path(), "opencode", &["sk2".to_string()]).unwrap(), 0);
        assert!(home.path().join(".config").join("opencode").join("skills").join("sk2").is_symlink());
    }

    // ---- scan / adopt ----

    #[test]
    fn scan_lists_real_dirs_skips_links_and_marks_in_library() {
        let home = TempHome::new();
        lib_skill(&home, "known", "");
        // claude-code:一个库里已有的、一个新的、一个软链(跳过)、一个无 SKILL.md(跳过)
        let cc = home.path().join(".claude").join("skills");
        write_skill(&cc, "known", "user copy");
        write_skill(&cc, "fresh", "brand new");
        std::os::unix::fs::symlink(library_dir(home.path()).join("known"), cc.join("linked")).unwrap();
        std::fs::create_dir_all(cc.join("junk")).unwrap();

        let found = scan(home.path()).unwrap();
        let names: Vec<(String, String, bool)> = found
            .iter()
            .map(|c| (c.agent_id.clone(), c.name.clone(), c.in_library))
            .collect();
        assert_eq!(
            names,
            vec![
                ("claude-code".to_string(), "fresh".to_string(), false),
                ("claude-code".to_string(), "known".to_string(), true),
            ]
        );
        assert_eq!(found[0].description, "brand new");
    }

    #[test]
    fn adopt_copies_backs_up_and_replaces_with_link() {
        let home = TempHome::new();
        let cc = home.path().join(".claude").join("skills");
        write_skill(&cc, "fresh", "to adopt");
        // 库已有同名的场景:hermes 侧的 known 只换链不拷贝
        lib_skill(&home, "known", "library version");
        write_skill(&home.path().join(".hermes").join("skills"), "known", "agent version");

        let outcomes = adopt(
            home.path(),
            &[
                AdoptRequest { agent_id: "claude-code".into(), name: "fresh".into() },
                AdoptRequest { agent_id: "hermes".into(), name: "known".into() },
                AdoptRequest { agent_id: "claude-code".into(), name: "no-such".into() },
                AdoptRequest { agent_id: "codex".into(), name: "fresh".into() },
            ],
        );
        assert_eq!(outcomes.len(), 4);
        // ① fresh:拷入库 + 备份 + 换链
        assert!(outcomes[0].ok, "{}", outcomes[0].detail);
        let lib_fresh = library_dir(home.path()).join("fresh");
        assert!(lib_fresh.join("SKILL.md").is_file());
        assert!(lib_fresh.join("scripts").join("run.sh").is_file());
        assert!(cc.join("fresh").is_symlink());
        assert_eq!(std::fs::read_link(cc.join("fresh")).unwrap(), lib_fresh);
        let backup_root = home.path().join(".clawbox").join("backups");
        let stamp_dir = std::fs::read_dir(&backup_root).unwrap().next().unwrap().unwrap().path();
        assert!(stamp_dir.join("skills").join("claude-code__fresh").join("SKILL.md").is_file());
        // ② known:库已有同名 → 不覆盖库内容,只备份换链
        assert!(outcomes[1].ok, "{}", outcomes[1].detail);
        let lib_md = std::fs::read_to_string(library_dir(home.path()).join("known").join("SKILL.md")).unwrap();
        assert!(lib_md.contains("library version"), "{}", lib_md);
        assert!(home.path().join(".hermes").join("skills").join("known").is_symlink());
        // ③④ 失败条目隔离:不存在的技能、不支持的 agent
        assert!(!outcomes[2].ok && outcomes[2].detail.contains("does not exist"));
        assert!(!outcomes[3].ok && outcomes[3].detail.contains("does not support"));
    }

    #[test]
    fn adopted_skill_plans_unchanged_afterwards() {
        let home = TempHome::new();
        write_skill(&home.path().join(".openclaw").join("skills"), "sk", "");
        let outcomes = adopt(
            home.path(),
            &[AdoptRequest { agent_id: "openclaw".into(), name: "sk".into() }],
        );
        assert!(outcomes[0].ok, "{}", outcomes[0].detail);
        let plans = plan_all(home.path(), &HashMap::new());
        let oc = plans.iter().find(|p| p.agent_id == "openclaw").unwrap();
        assert_eq!(action_map(&oc.changes), vec![("sk".to_string(), "unchanged".to_string())]);
        // 其它支持的 agent 侧变成 add(库里有了)
        let cc = plans.iter().find(|p| p.agent_id == "claude-code").unwrap();
        assert_eq!(cc.changes[0].action, "add");
    }

    // ---- Git 安装引擎(本地 file:// 仓造数,全程无网络) ----

    fn git_in(dir: &Path, args: &[&str]) {
        let out = crate::proc::command("git")
            .args(args)
            .current_dir(dir)
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@example.com")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@example.com")
            .output()
            .unwrap();
        assert!(out.status.success(), "git {:?}: {}", args, String::from_utf8_lossy(&out.stderr));
    }

    /// tempdir 里造一个可 clone 的本地仓;返回 (仓路径, file:// URL)。
    fn init_repo(base: &Path, name: &str) -> (PathBuf, String) {
        let dir = base.join(name);
        std::fs::create_dir_all(&dir).unwrap();
        git_in(&dir, &["init"]);
        (dir.clone(), format!("file://{}", dir.display()))
    }

    fn commit_all(repo: &Path, msg: &str) {
        git_in(repo, &["add", "-A"]);
        git_in(repo, &["commit", "-m", msg, "--allow-empty"]);
    }

    #[test]
    fn normalize_repo_url_rules() {
        assert_eq!(
            normalize_repo_url("anthropics/skills"),
            "https://github.com/anthropics/skills.git"
        );
        // 简写带 .git 不重复追加
        assert_eq!(normalize_repo_url("a/b.git"), "https://github.com/a/b.git");
        assert_eq!(normalize_repo_url("  a-b_c/d.e  "), "https://github.com/a-b_c/d.e.git");
        // 全 URL / scp 风格原样
        assert_eq!(normalize_repo_url("https://gitlab.com/a/b.git"), "https://gitlab.com/a/b.git");
        assert_eq!(normalize_repo_url("git@github.com:a/b.git"), "git@github.com:a/b.git");
        assert_eq!(normalize_repo_url("file:///tmp/x"), "file:///tmp/x");
        // 非简写形状原样(交给 git 报错)
        assert_eq!(normalize_repo_url("a/b/c"), "a/b/c");
    }

    #[test]
    fn repo_discover_finds_root_and_nested_skills_within_depth() {
        let home = TempHome::new();
        // 根技能仓
        let (root_repo, root_url) = init_repo(home.path(), "root-skill");
        std::fs::write(root_repo.join("SKILL.md"), "---\ndescription: root one\n---\n").unwrap();
        commit_all(&root_repo, "init");
        let d = repo_discover(home.path(), &root_url).unwrap();
        assert_eq!(d.repo, root_url);
        assert_eq!(d.commit.len(), 40, "{}", d.commit);
        assert_eq!(d.skills.len(), 1);
        assert_eq!(d.skills[0].name, "root-skill"); // 根技能 name = 仓名
        assert_eq!(d.skills[0].subdir, "");
        assert_eq!(d.skills[0].description, "root one");
        assert!(!d.skills[0].in_library);

        // 一仓多技能 + 深度边界 + in_library
        let (multi, multi_url) = init_repo(home.path(), "multi");
        for (rel, desc) in [("skills/a", "skill a"), ("skills/b", "skill b")] {
            let p = multi.join(rel);
            std::fs::create_dir_all(&p).unwrap();
            std::fs::write(p.join("SKILL.md"), format!("---\ndescription: {}\n---\n", desc)).unwrap();
        }
        let deep_ok = multi.join("l1/l2/l3/l4"); // 深度 4:收
        std::fs::create_dir_all(&deep_ok).unwrap();
        std::fs::write(deep_ok.join("SKILL.md"), "---\n---\n").unwrap();
        let too_deep = multi.join("d1/d2/d3/d4/d5"); // 深度 5:不收
        std::fs::create_dir_all(&too_deep).unwrap();
        std::fs::write(too_deep.join("SKILL.md"), "---\n---\n").unwrap();
        std::fs::create_dir_all(multi.join("node_modules/fake")).unwrap();
        std::fs::write(multi.join("node_modules/fake/SKILL.md"), "x").unwrap();
        commit_all(&multi, "init");
        lib_skill(&home, "a", ""); // 库中已有同名 a
        let d = repo_discover(home.path(), &multi_url).unwrap();
        let got: Vec<(&str, &str, bool)> =
            d.skills.iter().map(|s| (s.name.as_str(), s.subdir.as_str(), s.in_library)).collect();
        assert_eq!(
            got,
            vec![("l4", "l1/l2/l3/l4", false), ("a", "skills/a", true), ("b", "skills/b", false)]
        );

        // 无 SKILL.md 仓 → 空列表
        let (empty, empty_url) = init_repo(home.path(), "empty");
        std::fs::write(empty.join("README.md"), "no skills").unwrap();
        commit_all(&empty, "init");
        assert!(repo_discover(home.path(), &empty_url).unwrap().skills.is_empty());
    }

    #[test]
    fn repo_install_copies_records_source_and_isolates_conflicts() {
        let home = TempHome::new();
        let (multi, multi_url) = init_repo(home.path(), "multi");
        for rel in ["skills/a", "skills/b"] {
            let p = multi.join(rel);
            std::fs::create_dir_all(&p).unwrap();
            std::fs::write(p.join("SKILL.md"), "---\ndescription: x\n---\n").unwrap();
        }
        commit_all(&multi, "init");
        lib_skill(&home, "b", "already here"); // 同名冲突

        let (outcomes, sources) =
            repo_install(home.path(), &multi_url, &["skills/a".to_string(), "skills/b".to_string()])
                .unwrap();
        assert_eq!(outcomes.len(), 2);
        assert!(outcomes[0].ok, "{}", outcomes[0].detail);
        assert!(!outcomes[1].ok && outcomes[1].detail.contains("already exists"), "{}", outcomes[1].detail);
        assert!(library_dir(home.path()).join("a").join("SKILL.md").is_file());
        // source 只记成功的 a
        assert_eq!(sources.len(), 1);
        let (name, src) = &sources[0];
        assert_eq!(name, "a");
        assert_eq!(src.repo, multi_url);
        assert_eq!(src.subdir, "skills/a");
        assert_eq!(src.commit.len(), 40);
        assert!(src.installed_at.ends_with('Z') && src.installed_at.contains('T'));

        // 根技能安装:.git 不落库
        let (root_repo, root_url) = init_repo(home.path(), "rooty");
        std::fs::write(root_repo.join("SKILL.md"), "---\n---\n").unwrap();
        commit_all(&root_repo, "init");
        let (outcomes, _) = repo_install(home.path(), &root_url, &[String::new()]).unwrap();
        assert!(outcomes[0].ok, "{}", outcomes[0].detail);
        let installed = library_dir(home.path()).join("rooty");
        assert!(installed.join("SKILL.md").is_file());
        assert!(!installed.join(".git").exists(), ".git must not be copied into the library");
    }

    #[test]
    fn check_updates_detects_new_commits_and_missing_subdir() {
        let home = TempHome::new();
        let (repo, url) = init_repo(home.path(), "up");
        let a = repo.join("skills/a");
        std::fs::create_dir_all(&a).unwrap();
        std::fs::write(a.join("SKILL.md"), "---\n---\nv1\n").unwrap();
        commit_all(&repo, "init");
        let (_, sources) = repo_install(home.path(), &url, &["skills/a".to_string()]).unwrap();
        let sources: HashMap<String, SkillSource> = sources.into_iter().collect();

        // 无新 commit → 无更新
        let infos = check_updates(&sources).unwrap();
        assert_eq!(infos.len(), 1);
        assert!(!infos[0].has_update && !infos[0].missing);
        assert_eq!(infos[0].current_commit, infos[0].latest_commit);

        // 仓库前进一个 commit → has_update
        std::fs::write(a.join("SKILL.md"), "---\n---\nv2\n").unwrap();
        commit_all(&repo, "bump");
        let infos = check_updates(&sources).unwrap();
        assert!(infos[0].has_update && !infos[0].missing);
        assert_ne!(infos[0].current_commit, infos[0].latest_commit);

        // subdir 删除 → missing
        std::fs::remove_dir_all(&a).unwrap();
        commit_all(&repo, "drop a");
        let infos = check_updates(&sources).unwrap();
        assert!(infos[0].missing);
    }

    #[test]
    fn update_backs_up_replaces_and_refreshes_commit() {
        let home = TempHome::new();
        let (repo, url) = init_repo(home.path(), "up2");
        let a = repo.join("sk");
        std::fs::create_dir_all(&a).unwrap();
        std::fs::write(a.join("SKILL.md"), "---\ndescription: v1\n---\n").unwrap();
        commit_all(&repo, "init");
        let (_, sources) = repo_install(home.path(), &url, &["sk".to_string()]).unwrap();
        let old_commit = sources[0].1.commit.clone();
        let sources: HashMap<String, SkillSource> = sources.into_iter().collect();

        // 仓库出 v2
        std::fs::write(a.join("SKILL.md"), "---\ndescription: v2\n---\n").unwrap();
        std::fs::write(a.join("extra.txt"), "new file").unwrap();
        commit_all(&repo, "v2");

        let (outcomes, updated) = update(home.path(), &["sk".to_string()], &sources);
        assert!(outcomes[0].ok, "{}", outcomes[0].detail);
        // 库内容已替换为 v2
        let lib_md =
            std::fs::read_to_string(library_dir(home.path()).join("sk").join("SKILL.md")).unwrap();
        assert!(lib_md.contains("v2"), "{}", lib_md);
        assert!(library_dir(home.path()).join("sk").join("extra.txt").is_file());
        // 备份是 v1
        let backups = home.path().join(".clawbox").join("backups");
        let stamp = std::fs::read_dir(&backups).unwrap().next().unwrap().unwrap().path();
        let backup_md =
            std::fs::read_to_string(stamp.join("skills-lib").join("sk").join("SKILL.md")).unwrap();
        assert!(backup_md.contains("v1"), "{}", backup_md);
        // source.commit 已刷新
        assert_eq!(updated.len(), 1);
        assert_ne!(updated[0].1.commit, old_commit);
        assert_eq!(updated[0].1.repo, url);

        // 无来源记录 → ok=false
        let (outcomes, updated) = update(home.path(), &["nope".to_string()], &sources);
        assert!(!outcomes[0].ok && outcomes[0].detail.contains("No source record"), "{}", outcomes[0].detail);
        assert!(updated.is_empty());
    }

    #[test]
    fn apply_one_reports_and_backup_skips_directories() {
        let home = TempHome::new();
        lib_skill(&home, "sk", "");
        // 技能目录已存在(目录不是文件)→ backup_target 返回 None
        std::fs::create_dir_all(home.path().join(".claude").join("skills")).unwrap();
        let r = apply_one(home.path(), "claude-code", &[]);
        assert!(r.ok);
        assert!(r.backup_path.is_none());
        assert_eq!(r.applied, 1);
        let r = apply_one(home.path(), "codex", &[]);
        assert!(!r.ok);
        assert!(r.error.unwrap().contains("not supported"));
    }
}
