//! 配置导入/导出(issue #2):单个 .clawbox.json,服务商/MCP/技能三 section
//! 各自可选。Spec: docs/superpowers/specs/2026-08-05-config-export-import-design.md
//!
//! 判重口径(与 cc-switch 导入一致):服务商任一端点 URL 与已有条目相同 →
//! 合并(只补空字段,非空不覆盖);MCP 同名同内容跳过、同名异内容覆盖;
//! 技能库内同名跳过。全部 home 参数化,tauri command 薄封装 real_home()。

use super::config::{load_config, save_config, McpServerSpec, ProviderSpec};
use super::config::real_home;
use super::sync::skills_repo_install_at;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

pub const EXPORT_VERSION: u32 = 1;

// ---- 文件格式 --------------------------------------------------------------

/// 导出的服务商条目:ProviderSpec 去掉内部 id/迁移遗留字段。
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct ExportProvider {
    pub name: String,
    #[serde(default)]
    pub anthropic_base_url: String,
    #[serde(default)]
    pub openai_base_url: String,
    #[serde(default)]
    pub api_key: String,
    #[serde(default)]
    pub default_model: String,
    #[serde(default)]
    pub models: Vec<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

/// 导出的技能条目:仅安装来源(commit/时间是本机状态,导入方装最新)。
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ExportSkill {
    pub repo: String,
    #[serde(default)]
    pub subdir: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ExportFile {
    pub clawbox_export: u32,
    #[serde(default)]
    pub exported_at: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub providers: Vec<ExportProvider>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub mcp_servers: BTreeMap<String, McpServerSpec>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub skills: BTreeMap<String, ExportSkill>,
}

// ---- 导出 ------------------------------------------------------------------

fn now_iso() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_default()
}

/// 按勾选生成导出文件。provider_ids/skill_names 为空 = 该 section 不导。
pub fn export_at(
    home: &Path,
    out_path: &Path,
    provider_ids: &[String],
    include_keys: bool,
    include_mcp: bool,
    skill_names: &[String],
) -> Result<(), String> {
    let config = load_config(home)?;
    let providers = config
        .providers
        .iter()
        .filter(|p| provider_ids.contains(&p.id))
        .map(|p| ExportProvider {
            name: p.name.clone(),
            anthropic_base_url: p.anthropic_base_url.clone(),
            openai_base_url: p.openai_base_url.clone(),
            api_key: if include_keys { p.api_key.clone() } else { String::new() },
            default_model: p.default_model.clone(),
            models: p.models.clone(),
            enabled: p.enabled,
        })
        .collect();
    let mcp_servers = if include_mcp {
        config.mcp_servers.clone()
    } else {
        BTreeMap::new()
    };
    let skills = config
        .skills_sources
        .iter()
        .filter(|(name, _)| skill_names.contains(name))
        .map(|(name, s)| (name.clone(), ExportSkill { repo: s.repo.clone(), subdir: s.subdir.clone() }))
        .collect();

    let file = ExportFile {
        clawbox_export: EXPORT_VERSION,
        exported_at: now_iso(),
        providers,
        mcp_servers,
        skills,
    };
    let text = serde_json::to_string_pretty(&file).map_err(|e| format!("serialize failed: {}", e))?;
    if let Some(dir) = out_path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("failed to create {}: {}", dir.display(), e))?;
    }
    std::fs::write(out_path, text + "\n")
        .map_err(|e| format!("failed to write {}: {}", out_path.display(), e))
}

// ---- 导入:解析 + 预览 -----------------------------------------------------

fn parse_export_file(path: &Path) -> Result<ExportFile, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("failed to read {}: {}", path.display(), e))?;
    let file: ExportFile = serde_json::from_str(&text)
        .map_err(|e| format!("not a valid ClawBox export file: {}", e))?;
    if file.clawbox_export > EXPORT_VERSION {
        return Err(format!(
            "file was exported by a newer ClawBox (format v{}), please upgrade",
            file.clawbox_export
        ));
    }
    Ok(file)
}

/// 预览条目。action: "add" | "merge" | "overwrite" | "skip"。
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TransferItem {
    pub name: String,
    pub action: String,
    /// merge 的目标名 / skip 原因等展示信息。
    pub detail: String,
}

#[derive(Serialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct TransferPreview {
    pub providers: Vec<TransferItem>,
    pub mcp: Vec<TransferItem>,
    pub skills: Vec<TransferItem>,
}

/// 非空端点相同(忽略尾斜杠) → 视为同一服务商。
fn same_endpoint(a: &str, b: &str) -> bool {
    let (a, b) = (a.trim().trim_end_matches('/'), b.trim().trim_end_matches('/'));
    !a.is_empty() && a == b
}

fn merge_target<'a>(existing: &'a [ProviderSpec], inc: &ExportProvider) -> Option<&'a ProviderSpec> {
    existing.iter().find(|p| {
        same_endpoint(&p.anthropic_base_url, &inc.anthropic_base_url)
            || same_endpoint(&p.openai_base_url, &inc.openai_base_url)
    })
}

pub fn import_preview_at(home: &Path, path: &Path) -> Result<TransferPreview, String> {
    let file = parse_export_file(path)?;
    let config = load_config(home)?;
    let mut preview = TransferPreview::default();

    for inc in &file.providers {
        preview.providers.push(match merge_target(&config.providers, inc) {
            Some(t) => TransferItem {
                name: inc.name.clone(),
                action: "merge".into(),
                detail: t.name.clone(),
            },
            None => TransferItem { name: inc.name.clone(), action: "add".into(), detail: String::new() },
        });
    }
    for (name, spec) in &file.mcp_servers {
        preview.mcp.push(match config.mcp_servers.get(name) {
            Some(cur) if cur == spec => TransferItem {
                name: name.clone(),
                action: "skip".into(),
                detail: "identical".into(),
            },
            Some(_) => TransferItem { name: name.clone(), action: "overwrite".into(), detail: String::new() },
            None => TransferItem { name: name.clone(), action: "add".into(), detail: String::new() },
        });
    }
    for (name, s) in &file.skills {
        preview.skills.push(if config.skills_sources.contains_key(name) {
            TransferItem { name: name.clone(), action: "skip".into(), detail: "already installed".into() }
        } else {
            TransferItem { name: name.clone(), action: "add".into(), detail: s.repo.clone() }
        });
    }
    Ok(preview)
}

// ---- 导入:应用 ------------------------------------------------------------

/// 前端勾选:三个 section 各自的条目名(服务商用 name,MCP/技能用键名)。
#[derive(Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct TransferPicks {
    #[serde(default)]
    pub providers: Vec<String>,
    #[serde(default)]
    pub mcp: Vec<String>,
    #[serde(default)]
    pub skills: Vec<String>,
}

#[derive(Serialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct TransferOutcome {
    pub providers_added: usize,
    pub providers_merged: usize,
    pub mcp_applied: usize,
    pub skills_installed: usize,
    /// 逐项失败(如技能克隆失败),不中断其它项。
    pub errors: Vec<String>,
}

/// 导入方本地生成 id:时间戳+计数,避免依赖 uuid crate(唯一性只需本机内)。
fn fresh_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    let nanos = time::OffsetDateTime::now_utc().unix_timestamp_nanos();
    format!("imp-{:x}-{:x}", nanos, N.fetch_add(1, Ordering::Relaxed))
}

/// 合并:只补空字段,非空不覆盖(与 cc-switch 导入口径一致)。
fn merge_into(target: &mut ProviderSpec, inc: &ExportProvider) {
    let fill = |dst: &mut String, src: &str| {
        if dst.trim().is_empty() && !src.trim().is_empty() {
            *dst = src.to_string();
        }
    };
    fill(&mut target.anthropic_base_url, &inc.anthropic_base_url);
    fill(&mut target.openai_base_url, &inc.openai_base_url);
    fill(&mut target.api_key, &inc.api_key);
    fill(&mut target.default_model, &inc.default_model);
    for m in &inc.models {
        if !target.models.contains(m) {
            target.models.push(m.clone());
        }
    }
}

pub fn import_apply_at(home: &Path, path: &Path, picks: &TransferPicks) -> Result<TransferOutcome, String> {
    let file = parse_export_file(path)?;
    let mut config = load_config(home)?;
    let mut out = TransferOutcome::default();

    for inc in file.providers.iter().filter(|p| picks.providers.contains(&p.name)) {
        if let Some(target) = config
            .providers
            .iter_mut()
            .find(|p| {
                same_endpoint(&p.anthropic_base_url, &inc.anthropic_base_url)
                    || same_endpoint(&p.openai_base_url, &inc.openai_base_url)
            })
        {
            merge_into(target, inc);
            out.providers_merged += 1;
        } else {
            config.providers.push(ProviderSpec {
                id: fresh_id(),
                name: inc.name.clone(),
                api_key: inc.api_key.clone(),
                base_url: String::new(),
                anthropic_base_url: inc.anthropic_base_url.clone(),
                openai_base_url: inc.openai_base_url.clone(),
                default_model: inc.default_model.clone(),
                models: inc.models.clone(),
                enabled: inc.enabled,
                flavor: None,
            });
            out.providers_added += 1;
        }
    }

    for (name, spec) in file.mcp_servers.iter().filter(|(n, _)| picks.mcp.contains(n)) {
        if config.mcp_servers.get(name) != Some(spec) {
            config.mcp_servers.insert(name.clone(), spec.clone());
            out.mcp_applied += 1;
        }
    }

    save_config(home, &config)?;

    // 技能:按 repo 分组批量装(克隆一次装多个 subdir)。库内已有的跳过。
    // skills_repo_install_at 内部会自行读写 config,须在上面 save 之后调用。
    let mut by_repo: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (name, s) in file.skills.iter().filter(|(n, _)| picks.skills.contains(n)) {
        if config.skills_sources.contains_key(name) {
            continue;
        }
        by_repo.entry(s.repo.clone()).or_default().push(s.subdir.clone());
    }
    for (repo, subdirs) in by_repo {
        match skills_repo_install_at(home, &repo, &subdirs) {
            Ok(outcomes) => {
                for o in outcomes {
                    if o.ok {
                        out.skills_installed += 1;
                    } else {
                        out.errors.push(format!("skill {}: {}", o.name, o.detail));
                    }
                }
            }
            Err(e) => out.errors.push(format!("repo {}: {}", repo, e)),
        }
    }
    Ok(out)
}

// ---- tauri commands --------------------------------------------------------

#[tauri::command]
pub async fn transfer_export(
    path: String,
    provider_ids: Vec<String>,
    include_keys: bool,
    include_mcp: bool,
    skill_names: Vec<String>,
) -> Result<(), String> {
    export_at(&real_home(), Path::new(&path), &provider_ids, include_keys, include_mcp, &skill_names)
}

#[tauri::command]
pub async fn transfer_import_preview(path: String) -> Result<TransferPreview, String> {
    import_preview_at(&real_home(), Path::new(&path))
}

#[tauri::command]
pub async fn transfer_import_apply(path: String, picks: TransferPicks) -> Result<TransferOutcome, String> {
    import_apply_at(&real_home(), Path::new(&path), &picks)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sync::test_util::TempHome;
    use super::super::config::SkillSource;

    fn seed_provider(id: &str, name: &str, anth: &str, openai: &str, key: &str) -> ProviderSpec {
        ProviderSpec {
            id: id.into(),
            name: name.into(),
            api_key: key.into(),
            base_url: String::new(),
            anthropic_base_url: anth.into(),
            openai_base_url: openai.into(),
            default_model: "m1".into(),
            models: vec!["m1".into()],
            enabled: true,
            flavor: None,
        }
    }

    fn seed_home() -> TempHome {
        let home = TempHome::new();
        let mut c = load_config(home.path()).unwrap();
        c.providers = vec![seed_provider("a", "Alpha", "https://a.example.com/", "", "sk-a")];
        c.mcp_servers.insert(
            "ctx7".into(),
            McpServerSpec {
                kind: "stdio".into(),
                command: Some("npx".into()),
                args: vec!["-y".into(), "ctx7".into()],
                env: Default::default(),
                url: None,
                headers: Default::default(),
                enabled: true,
            },
        );
        c.skills_sources.insert(
            "sk1".into(),
            SkillSource { repo: "https://github.com/x/y".into(), subdir: "sk1".into(), commit: "c".into(), installed_at: "t".into() },
        );
        save_config(home.path(), &c).unwrap();
        home
    }

    #[test]
    fn export_roundtrips_and_respects_selection() {
        let home = seed_home();
        let out = home.path().join("share.clawbox.json");
        export_at(home.path(), &out, &["a".into()], true, true, &["sk1".into()]).unwrap();
        let file = parse_export_file(&out).unwrap();
        assert_eq!(file.clawbox_export, 1);
        assert_eq!(file.providers.len(), 1);
        assert_eq!(file.providers[0].api_key, "sk-a");
        assert_eq!(file.mcp_servers.len(), 1);
        assert_eq!(file.skills["sk1"].repo, "https://github.com/x/y");
    }

    #[test]
    fn export_can_strip_keys_and_sections() {
        let home = seed_home();
        let out = home.path().join("share.clawbox.json");
        export_at(home.path(), &out, &["a".into()], false, false, &[]).unwrap();
        let file = parse_export_file(&out).unwrap();
        assert_eq!(file.providers[0].api_key, "");
        assert!(file.mcp_servers.is_empty());
        assert!(file.skills.is_empty());
    }

    #[test]
    fn preview_marks_add_merge_skip() {
        let src = seed_home();
        let out = src.path().join("share.clawbox.json");
        export_at(src.path(), &out, &["a".into()], true, true, &["sk1".into()]).unwrap();

        // 目标机:同端点服务商(应 merge)、同名同内容 MCP(应 skip)、已装技能(应 skip)
        let dst = seed_home();
        {
            let mut c = load_config(dst.path()).unwrap();
            c.providers[0].name = "AlphaLocal".into(); // 名字不同、端点相同 → merge 到它
            save_config(dst.path(), &c).unwrap();
        }
        let p = import_preview_at(dst.path(), &out).unwrap();
        assert_eq!(p.providers[0], TransferItem { name: "Alpha".into(), action: "merge".into(), detail: "AlphaLocal".into() });
        assert_eq!(p.mcp[0].action, "skip");
        assert_eq!(p.skills[0].action, "skip");

        // 空目标机:全 add
        let empty = TempHome::new();
        let p = import_preview_at(empty.path(), &out).unwrap();
        assert_eq!(p.providers[0].action, "add");
        assert_eq!(p.mcp[0].action, "add");
        assert_eq!(p.skills[0].action, "add");
    }

    #[test]
    fn apply_adds_and_merges_without_overwriting_nonempty() {
        let src = seed_home();
        let out = src.path().join("share.clawbox.json");
        export_at(src.path(), &out, &["a".into()], true, true, &[]).unwrap();

        let dst = TempHome::new();
        {
            // 同端点、key 已有(不得被覆盖)、缺 openai 端点(应被补上)
            let mut c = load_config(dst.path()).unwrap();
            let mut p = seed_provider("z", "Local", "https://a.example.com", "", "sk-local");
            p.default_model = String::new(); // 空,应被补成 m1
            c.providers = vec![p];
            save_config(dst.path(), &c).unwrap();
        }
        let picks = TransferPicks { providers: vec!["Alpha".into()], mcp: vec!["ctx7".into()], skills: vec![] };
        let o = import_apply_at(dst.path(), &out, &picks).unwrap();
        assert_eq!((o.providers_added, o.providers_merged, o.mcp_applied), (0, 1, 1));

        let c = load_config(dst.path()).unwrap();
        assert_eq!(c.providers.len(), 1);
        assert_eq!(c.providers[0].api_key, "sk-local"); // 非空不覆盖
        assert_eq!(c.providers[0].default_model, "m1"); // 空字段补齐
        assert!(c.mcp_servers.contains_key("ctx7"));

        // 空目标机 add:生成新 id 且非空
        let empty = TempHome::new();
        let o = import_apply_at(empty.path(), &out, &picks).unwrap();
        assert_eq!(o.providers_added, 1);
        let c = load_config(empty.path()).unwrap();
        assert!(!c.providers[0].id.is_empty());
        assert_eq!(c.providers[0].name, "Alpha");
    }

    #[test]
    fn unknown_version_and_bad_json_are_rejected() {
        let home = TempHome::new();
        let p = home.path().join("bad.clawbox.json");
        std::fs::write(&p, "{\"clawboxExport\": 99}").unwrap();
        let err = import_preview_at(home.path(), &p).unwrap_err();
        assert!(err.contains("newer ClawBox"), "{}", err);
        std::fs::write(&p, "not json").unwrap();
        assert!(import_preview_at(home.path(), &p).is_err());
    }
}
