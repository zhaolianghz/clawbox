//! cc-switch 服务商配置一键导入。
//!
//! 数据来源:自动探测 `~/.cc-switch/config.json`(旧版格式),找不到则返回
//! `NeedFile` 让前端弹文件选择器选一个 cc-switch 导出的 JSON。
//!
//! 解析写成内容驱动:递归遍历 JSON,凡带 `settingsConfig` + `name` 的对象即视为一条
//! provider,并从其父级 key 推断 app_type。这样旧版 config.json(providers 按 app 嵌套)
//! 与导出 JSON 两种外层包装都能吃下;app_type 拿不到时再靠内容嗅探兜底。
//!
//! 各 app_type → 端点协议映射(结构取自实际 cc-switch.db):
//!   claude / claude-desktop → env.ANTHROPIC_BASE_URL (+AUTH_TOKEN/API_KEY) → anthropic 槽
//!   codex                   → config(TOML)的 [model_providers.*].base_url + auth.OPENAI_API_KEY → openai 槽
//!   opencode                → options.baseURL / options.apiKey → openai 槽
//!   hermes                  → base_url / api_key,按 api_mode 定槽
//!   gemini                  → env.GOOGLE_GEMINI_BASE_URL / GEMINI_API_KEY → openai 槽
//!
//! 抽取后按归一化 host 合并:同 host 的 anthropic 条目与 openai 条目合成一条双端点候选。

use serde::Serialize;
use serde_json::Value;
use std::path::{Path, PathBuf};
use toml_edit::DocumentMut;

use crate::commands::config::real_home;

/// 一条导入候选(已按 host 合并;去重/落盘由前端做)。camelCase 与前端对齐。
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ImportCandidate {
    pub name: String,
    /// Anthropic 兼容端点;空 = 无该槽。
    pub anthropic_base_url: String,
    /// OpenAI 兼容端点;空 = 无该槽。
    pub openai_base_url: String,
    pub api_key: String,
    pub default_model: String,
    /// 便于前端展示,可空。
    pub website: String,
    /// 来源 app_type(去重后排序),预览里展示「来自 claude+codex」。
    pub source_apps: Vec<String>,
}

/// 预览结果:探测到配置返回候选;未探测到返回 NeedFile 让前端选文件。
#[derive(Serialize, Clone, Debug)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ImportPreview {
    Found { candidates: Vec<ImportCandidate> },
    NeedFile,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Protocol {
    Anthropic,
    Openai,
}

/// 抽取阶段的单条端点(合并前)。
#[derive(Clone, Debug)]
struct Extracted {
    app_type: String,
    name: String,
    website: String,
    protocol: Protocol,
    url: String,
    api_key: String,
    model: String,
}

fn cc_switch_config_path(home: &Path) -> PathBuf {
    home.join(".cc-switch").join("config.json")
}

fn cc_switch_db_path(home: &Path) -> PathBuf {
    home.join(".cc-switch").join("cc-switch.db")
}

/// 只读 cc-switch 的 SQLite 数据源(当前版本 SSOT),从 `providers` 表抽取候选。
/// 每行的 `settings_config`(JSON 字符串)复用与 JSON 路径完全相同的 `extract_provider` 映射。
fn candidates_from_db(db_path: &Path) -> Result<Vec<ImportCandidate>, String> {
    use rusqlite::{Connection, OpenFlags};
    let conn = Connection::open_with_flags(
        db_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| format!("打开 cc-switch 数据库失败: {e}"))?;

    let mut stmt = conn
        .prepare("SELECT app_type, name, settings_config, website_url FROM providers")
        .map_err(|e| format!("读取 providers 表失败(schema 可能不兼容): {e}"))?;

    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
            ))
        })
        .map_err(|e| format!("查询 providers 失败: {e}"))?;

    let mut extracted = Vec::new();
    for r in rows {
        let (app_type, name, sc_str, website) = r.map_err(|e| format!("读取行失败: {e}"))?;
        let sc: Value = serde_json::from_str(&sc_str).unwrap_or(Value::Null);
        // 合成一个和 JSON 路径同形的 provider 对象,复用 extract_provider
        let synthetic = serde_json::json!({
            "name": name,
            "websiteUrl": website.unwrap_or_default(),
            "settingsConfig": sc,
        });
        if let Some(e) = extract_provider(&synthetic, Some(&app_type)) {
            extracted.push(e);
        }
    }
    Ok(merge_extracted(extracted))
}

/// 读一个 JSON 文件并解析成候选。
fn candidates_from_json_file(path: &Path) -> Result<Vec<ImportCandidate>, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("读取 cc-switch 配置失败 {}: {}", path.display(), e))?;
    let root: Value = serde_json::from_str(&content)
        .map_err(|e| format!("cc-switch 配置解析失败(非合法 JSON): {}", e))?;
    Ok(parse_candidates(&root))
}

/// 已知 app_type key(用于遍历时识别 app 分组;大小写不敏感)。
const KNOWN_APPS: &[&str] = &[
    "claude",
    "claude-desktop",
    "codex",
    "opencode",
    "hermes",
    "gemini",
    "gemini-cli",
    "grokbuild",
    "grok",
];

fn is_known_app(k: &str) -> bool {
    let k = k.to_lowercase();
    KNOWN_APPS.contains(&k.as_str())
}

/// 从 url 取归一化 host(含端口,与前端 `new URL(url).host` 对齐)。取不到返回原串。
fn host_of(url: &str) -> String {
    let s = url.trim();
    let s = s
        .strip_prefix("https://")
        .or_else(|| s.strip_prefix("http://"))
        .unwrap_or(s);
    let end = s.find(['/', '?', '#']).unwrap_or(s.len());
    s[..end].to_lowercase()
}

/// 读嵌套字符串:`get_str(v, &["env", "ANTHROPIC_BASE_URL"])`。缺失/非串返回空串。
fn get_str(v: &Value, path: &[&str]) -> String {
    let mut cur = v;
    for key in path {
        match cur.get(*key) {
            Some(next) => cur = next,
            None => return String::new(),
        }
    }
    cur.as_str().unwrap_or("").trim().to_string()
}

/// 递归遍历,收集所有「带 settingsConfig + name」的 provider 对象。
/// app_hint 为最近一次遇到的已知 app_type key。
fn walk(value: &Value, app_hint: Option<&str>, out: &mut Vec<Extracted>) {
    match value {
        Value::Object(map) => {
            // 是 provider 对象?(有 settingsConfig 且有 name)
            if map.contains_key("settingsConfig") && map.get("name").and_then(|n| n.as_str()).is_some()
            {
                if let Some(e) = extract_provider(value, app_hint) {
                    out.push(e);
                }
                return; // 不再深入 provider 内部
            }
            for (k, v) in map {
                let hint = if is_known_app(k) { Some(k.as_str()) } else { app_hint };
                walk(v, hint, out);
            }
        }
        Value::Array(arr) => {
            for item in arr {
                walk(item, app_hint, out);
            }
        }
        _ => {}
    }
}

/// 从 codex 的 `config`(TOML 字符串)提取 (base_url, model)。
fn parse_codex_toml(config: &Value) -> (String, String) {
    let toml_str = match config.as_str() {
        Some(s) => s,
        None => return (String::new(), String::new()),
    };
    let doc = match toml_str.parse::<DocumentMut>() {
        Ok(d) => d,
        Err(_) => return (String::new(), String::new()),
    };
    let model = doc.get("model").and_then(|i| i.as_str()).unwrap_or("").to_string();

    // 优先 model_provider 指名的那个,否则取第一个 [model_providers.*].base_url
    let mut url = String::new();
    if let Some(mp) = doc.get("model_providers").and_then(|i| i.as_table()) {
        let named = doc.get("model_provider").and_then(|i| i.as_str());
        if let Some(name) = named {
            if let Some(bu) = mp
                .get(name)
                .and_then(|i| i.as_table())
                .and_then(|t| t.get("base_url"))
                .and_then(|i| i.as_str())
            {
                url = bu.to_string();
            }
        }
        if url.is_empty() {
            for (_, item) in mp.iter() {
                if let Some(bu) = item.as_table().and_then(|t| t.get("base_url")).and_then(|i| i.as_str()) {
                    url = bu.to_string();
                    break;
                }
            }
        }
    }
    (url, model)
}

/// 把一条 provider 对象映射为 Extracted。取不到端点 url 的返回 None(无法按 host 归组)。
fn extract_provider(v: &Value, app_hint: Option<&str>) -> Option<Extracted> {
    let name = get_str(v, &["name"]);
    let website = get_str(v, &["websiteUrl"]);
    let sc = v.get("settingsConfig")?;
    let app = app_hint.map(|a| a.to_lowercase()).unwrap_or_default();

    let anthropic_key = || {
        let t = get_str(sc, &["env", "ANTHROPIC_AUTH_TOKEN"]);
        if !t.is_empty() {
            t
        } else {
            get_str(sc, &["env", "ANTHROPIC_API_KEY"])
        }
    };

    let (protocol, url, api_key, model) = match app.as_str() {
        "claude" | "claude-desktop" => (
            Protocol::Anthropic,
            get_str(sc, &["env", "ANTHROPIC_BASE_URL"]),
            anthropic_key(),
            get_str(sc, &["env", "ANTHROPIC_MODEL"]),
        ),
        "codex" => {
            let (url, model) = parse_codex_toml(sc.get("config").unwrap_or(&Value::Null));
            (Protocol::Openai, url, get_str(sc, &["auth", "OPENAI_API_KEY"]), model)
        }
        "opencode" => (
            Protocol::Openai,
            get_str(sc, &["options", "baseURL"]),
            get_str(sc, &["options", "apiKey"]),
            first_model_key(sc.get("models")),
        ),
        "hermes" => {
            let mode = get_str(sc, &["api_mode"]).to_lowercase();
            let protocol = if mode.contains("anthropic") {
                Protocol::Anthropic
            } else {
                Protocol::Openai
            };
            (
                protocol,
                get_str(sc, &["base_url"]),
                get_str(sc, &["api_key"]),
                first_array_str(sc.get("models")),
            )
        }
        "gemini" | "gemini-cli" => {
            let mut url = get_str(sc, &["env", "GOOGLE_GEMINI_BASE_URL"]);
            if url.is_empty() {
                url = get_str(sc, &["env", "GEMINI_BASE_URL"]);
            }
            (
                Protocol::Openai,
                url,
                get_str(sc, &["env", "GEMINI_API_KEY"]),
                get_str(sc, &["env", "GEMINI_MODEL"]),
            )
        }
        // 未知 app_type:内容嗅探兜底
        _ => sniff(sc, &anthropic_key)?,
    };

    let url = url.trim().to_string();
    if url.is_empty() {
        return None; // 无端点 → 无法归组,跳过
    }
    Some(Extracted {
        app_type: if app.is_empty() { "unknown".into() } else { app },
        name,
        website,
        protocol,
        url,
        api_key: api_key.trim().to_string(),
        model: model.trim().to_string(),
    })
}

/// 内容嗅探:app_type 未知时按 settingsConfig 结构推断协议与取值。
fn sniff(sc: &Value, anthropic_key: &dyn Fn() -> String) -> Option<(Protocol, String, String, String)> {
    let anth = get_str(sc, &["env", "ANTHROPIC_BASE_URL"]);
    if !anth.is_empty() {
        return Some((
            Protocol::Anthropic,
            anth,
            anthropic_key(),
            get_str(sc, &["env", "ANTHROPIC_MODEL"]),
        ));
    }
    let oc = get_str(sc, &["options", "baseURL"]);
    if !oc.is_empty() {
        return Some((Protocol::Openai, oc, get_str(sc, &["options", "apiKey"]), first_model_key(sc.get("models"))));
    }
    let gem = {
        let g = get_str(sc, &["env", "GOOGLE_GEMINI_BASE_URL"]);
        if g.is_empty() { get_str(sc, &["env", "GEMINI_BASE_URL"]) } else { g }
    };
    if !gem.is_empty() {
        return Some((Protocol::Openai, gem, get_str(sc, &["env", "GEMINI_API_KEY"]), get_str(sc, &["env", "GEMINI_MODEL"])));
    }
    if sc.get("config").and_then(|c| c.as_str()).is_some() {
        let (url, model) = parse_codex_toml(sc.get("config").unwrap());
        return Some((Protocol::Openai, url, get_str(sc, &["auth", "OPENAI_API_KEY"]), model));
    }
    let hermes = get_str(sc, &["base_url"]);
    if !hermes.is_empty() {
        let mode = get_str(sc, &["api_mode"]).to_lowercase();
        let protocol = if mode.contains("anthropic") { Protocol::Anthropic } else { Protocol::Openai };
        return Some((protocol, hermes, get_str(sc, &["api_key"]), first_array_str(sc.get("models"))));
    }
    None
}

/// opencode 的 models 是对象(model id 作 key),取第一个 key 作默认模型。
fn first_model_key(models: Option<&Value>) -> String {
    models
        .and_then(|m| m.as_object())
        .and_then(|o| o.keys().next())
        .cloned()
        .unwrap_or_default()
}

/// hermes 的 models 是数组,取首元素字符串。
fn first_array_str(models: Option<&Value>) -> String {
    models
        .and_then(|m| m.as_array())
        .and_then(|a| a.first())
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string()
}

/// JSON 解析入口:遍历 → 抽取 → 按 host 合并。
pub fn parse_candidates(root: &Value) -> Vec<ImportCandidate> {
    let mut extracted = Vec::new();
    walk(root, None, &mut extracted);
    merge_extracted(extracted)
}

/// 把抽取出的端点按 host 合并成双端点候选(保留首见顺序)。
fn merge_extracted(extracted: Vec<Extracted>) -> Vec<ImportCandidate> {
    let mut order: Vec<String> = Vec::new();
    let mut groups: std::collections::HashMap<String, Vec<Extracted>> = std::collections::HashMap::new();
    for e in extracted {
        let host = host_of(&e.url);
        if host.is_empty() {
            continue;
        }
        groups.entry(host.clone()).or_insert_with(|| {
            order.push(host.clone());
            Vec::new()
        });
        groups.get_mut(&host).unwrap().push(e);
    }

    order
        .into_iter()
        .map(|host| merge_group(&groups[&host]))
        .collect()
}

fn merge_group(group: &[Extracted]) -> ImportCandidate {
    let first_non_empty = |f: &dyn Fn(&Extracted) -> String| -> String {
        group.iter().map(f).find(|s| !s.is_empty()).unwrap_or_default()
    };

    let anthropic = group.iter().find(|e| e.protocol == Protocol::Anthropic);
    let openai = group.iter().find(|e| e.protocol == Protocol::Openai);

    // key:anthropic 槽优先,openai 兜底(与前端 fetchSlot 语义一致)
    let api_key = anthropic
        .map(|e| e.api_key.clone())
        .filter(|k| !k.is_empty())
        .or_else(|| openai.map(|e| e.api_key.clone()).filter(|k| !k.is_empty()))
        .unwrap_or_default();

    let mut source_apps: Vec<String> = group.iter().map(|e| e.app_type.clone()).collect();
    source_apps.sort();
    source_apps.dedup();

    ImportCandidate {
        name: first_non_empty(&|e| e.name.clone()),
        anthropic_base_url: anthropic.map(|e| e.url.clone()).unwrap_or_default(),
        openai_base_url: openai.map(|e| e.url.clone()).unwrap_or_default(),
        api_key,
        default_model: first_non_empty(&|e| e.model.clone()),
        website: first_non_empty(&|e| e.website.clone()),
        source_apps,
    }
}

/// 一键导入候选来源(自动优先级):
///   1. 指定 path(前端文件选择器结果):按扩展名 .db → SQLite,否则当 JSON。
///   2. 自动:`~/.cc-switch/cc-switch.db`(当前版本 SSOT)。
///   3. 自动:`~/.cc-switch/config.json`(旧版格式)。
///   4. 都没有 → NeedFile,由前端弹文件选择器。
#[tauri::command]
pub async fn cc_switch_import_preview(path: Option<String>) -> Result<ImportPreview, String> {
    if let Some(p) = path {
        let pb = PathBuf::from(&p);
        let candidates = if p.to_lowercase().ends_with(".db") {
            candidates_from_db(&pb)?
        } else {
            candidates_from_json_file(&pb)?
        };
        return Ok(ImportPreview::Found { candidates });
    }

    let home = real_home();
    let db = cc_switch_db_path(&home);
    let json = cc_switch_config_path(&home);

    if db.exists() {
        match candidates_from_db(&db) {
            Ok(candidates) => return Ok(ImportPreview::Found { candidates }),
            // DB 存在但读失败:有旧版 json 就兜底,否则把错误如实报出
            Err(e) => {
                if !json.exists() {
                    return Err(e);
                }
            }
        }
    }
    if json.exists() {
        return Ok(ImportPreview::Found {
            candidates: candidates_from_json_file(&json)?,
        });
    }
    Ok(ImportPreview::NeedFile)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn find<'a>(cs: &'a [ImportCandidate], host_frag: &str) -> &'a ImportCandidate {
        cs.iter()
            .find(|c| c.anthropic_base_url.contains(host_frag) || c.openai_base_url.contains(host_frag))
            .unwrap_or_else(|| panic!("no candidate matching {host_frag} in {cs:?}"))
    }

    #[test]
    fn claude_entry_maps_to_anthropic_slot_with_auth_token() {
        let root = json!({
            "providers": {
                "claude": { "providers": { "id1": {
                    "id": "id1", "name": "DeepSeek", "websiteUrl": "https://deepseek.com",
                    "settingsConfig": { "env": {
                        "ANTHROPIC_BASE_URL": "https://api.deepseek.com/anthropic",
                        "ANTHROPIC_AUTH_TOKEN": "sk-test-anth",
                        "ANTHROPIC_MODEL": "deepseek-chat"
                    }}
                }}, "current": "id1" }
            }
        });
        let cs = parse_candidates(&root);
        assert_eq!(cs.len(), 1);
        let c = &cs[0];
        assert_eq!(c.anthropic_base_url, "https://api.deepseek.com/anthropic");
        assert_eq!(c.openai_base_url, "");
        assert_eq!(c.api_key, "sk-test-anth");
        assert_eq!(c.default_model, "deepseek-chat");
        assert_eq!(c.name, "DeepSeek");
        assert_eq!(c.website, "https://deepseek.com");
        assert_eq!(c.source_apps, vec!["claude".to_string()]);
    }

    #[test]
    fn claude_api_key_fallback_when_no_auth_token() {
        let root = json!({ "claude": { "id1": {
            "name": "X", "settingsConfig": { "env": {
                "ANTHROPIC_BASE_URL": "https://x.example.com",
                "ANTHROPIC_API_KEY": "sk-test-apikey"
            }}
        }}});
        let cs = parse_candidates(&root);
        assert_eq!(cs[0].api_key, "sk-test-apikey");
    }

    #[test]
    fn codex_entry_extracts_base_url_and_model_from_toml() {
        let toml = "model = \"deepseek-chat\"\nmodel_provider = \"deepseek\"\n\n[model_providers.deepseek]\nname = \"deepseek\"\nbase_url = \"https://api.deepseek.com/v1\"\n";
        let root = json!({ "codex": { "id1": {
            "name": "DeepSeek", "settingsConfig": {
                "auth": { "OPENAI_API_KEY": "sk-test-oa" },
                "config": toml
            }
        }}});
        let cs = parse_candidates(&root);
        assert_eq!(cs.len(), 1);
        let c = &cs[0];
        assert_eq!(c.openai_base_url, "https://api.deepseek.com/v1");
        assert_eq!(c.anthropic_base_url, "");
        assert_eq!(c.api_key, "sk-test-oa");
        assert_eq!(c.default_model, "deepseek-chat");
    }

    #[test]
    fn opencode_and_gemini_and_hermes_map_to_expected_slots() {
        let root = json!({
            "opencode": { "o1": { "name": "OC", "settingsConfig": {
                "options": { "baseURL": "https://oc.example.com/v1", "apiKey": "sk-oc" },
                "models": { "m-1": { "name": "M1" } }
            }}},
            "gemini": { "g1": { "name": "Gemini", "settingsConfig": { "env": {
                "GOOGLE_GEMINI_BASE_URL": "https://generativelanguage.googleapis.com",
                "GEMINI_API_KEY": "sk-gem", "GEMINI_MODEL": "gemini-pro"
            }}}},
            "hermes": { "h1": { "name": "Herm", "settingsConfig": {
                "base_url": "https://herm.example.com", "api_key": "sk-herm",
                "api_mode": "anthropic-messages", "models": ["h-model"]
            }}}
        });
        let cs = parse_candidates(&root);
        let oc = find(&cs, "oc.example.com");
        assert_eq!(oc.openai_base_url, "https://oc.example.com/v1");
        assert_eq!(oc.api_key, "sk-oc");
        assert_eq!(oc.default_model, "m-1");

        let gem = find(&cs, "generativelanguage.googleapis.com");
        assert_eq!(gem.openai_base_url, "https://generativelanguage.googleapis.com");
        assert_eq!(gem.api_key, "sk-gem");

        let herm = find(&cs, "herm.example.com");
        assert_eq!(herm.anthropic_base_url, "https://herm.example.com"); // api_mode=anthropic → anthropic 槽
        assert_eq!(herm.openai_base_url, "");
    }

    #[test]
    fn same_host_claude_and_codex_merge_into_one_dual_endpoint() {
        let toml = "model = \"deepseek-chat\"\nmodel_provider = \"ds\"\n[model_providers.ds]\nbase_url = \"https://api.deepseek.com/v1\"\n";
        let root = json!({
            "claude": { "c": { "name": "DeepSeek", "settingsConfig": { "env": {
                "ANTHROPIC_BASE_URL": "https://api.deepseek.com/anthropic",
                "ANTHROPIC_AUTH_TOKEN": "sk-anth"
            }}}},
            "codex": { "x": { "name": "DeepSeek", "settingsConfig": {
                "auth": { "OPENAI_API_KEY": "sk-oa" }, "config": toml
            }}}
        });
        let cs = parse_candidates(&root);
        assert_eq!(cs.len(), 1, "same host should merge: {cs:?}");
        let c = &cs[0];
        assert_eq!(c.anthropic_base_url, "https://api.deepseek.com/anthropic");
        assert_eq!(c.openai_base_url, "https://api.deepseek.com/v1");
        assert_eq!(c.api_key, "sk-anth"); // anthropic 槽 key 优先
        assert_eq!(c.source_apps, vec!["claude".to_string(), "codex".to_string()]);
    }

    #[test]
    fn flat_and_nested_wrappers_yield_same_result() {
        let inner = json!({ "name": "X", "settingsConfig": { "env": {
            "ANTHROPIC_BASE_URL": "https://x.example.com", "ANTHROPIC_AUTH_TOKEN": "sk-x"
        }}});
        let nested = json!({ "providers": { "claude": { "providers": { "id1": inner.clone() }, "current": "id1" } } });
        let flat = json!({ "claude": { "id1": inner } });
        assert_eq!(parse_candidates(&nested), parse_candidates(&flat));
    }

    #[test]
    fn entries_without_endpoint_are_skipped() {
        let root = json!({
            "gemini": { "g": { "name": "Empty", "settingsConfig": { "env": {}, "config": {} } } },
            "codex": { "c": { "name": "OpenAI Official", "settingsConfig": {
                "auth": { "OPENAI_API_KEY": "sk-x" }, "config": "model = \"gpt-5\"\n"
            }}}
        });
        // 两条都无端点 url → 全跳过
        assert!(parse_candidates(&root).is_empty());
    }

    #[test]
    fn host_of_strips_scheme_and_path_keeps_port() {
        assert_eq!(host_of("https://api.deepseek.com/v1"), "api.deepseek.com");
        assert_eq!(host_of("http://localhost:11434/v1"), "localhost:11434");
    }

    #[test]
    fn reads_providers_from_sqlite_db() {
        use rusqlite::Connection;
        // 临时 DB,建与真实 cc-switch 同名同形的 providers 表(子集列)
        let dir = std::env::temp_dir().join(format!("ccsw-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let db_path = dir.join("cc-switch.db");
        let _ = std::fs::remove_file(&db_path);
        {
            let conn = Connection::open(&db_path).unwrap();
            conn.execute_batch(
                "CREATE TABLE providers (
                    id TEXT NOT NULL, app_type TEXT NOT NULL, name TEXT NOT NULL,
                    settings_config TEXT NOT NULL, website_url TEXT,
                    PRIMARY KEY (id, app_type));",
            )
            .unwrap();
            let claude_sc = r#"{"env":{"ANTHROPIC_BASE_URL":"https://api.deepseek.com/anthropic","ANTHROPIC_AUTH_TOKEN":"sk-db-anth"}}"#;
            let codex_sc = r#"{"auth":{"OPENAI_API_KEY":"sk-db-oa"},"config":"model = \"deepseek-chat\"\nmodel_provider = \"ds\"\n[model_providers.ds]\nbase_url = \"https://api.deepseek.com/v1\"\n"}"#;
            conn.execute(
                "INSERT INTO providers (id, app_type, name, settings_config, website_url) VALUES (?,?,?,?,?)",
                rusqlite::params!["1", "claude", "DeepSeek", claude_sc, "https://deepseek.com"],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO providers (id, app_type, name, settings_config, website_url) VALUES (?,?,?,?,?)",
                rusqlite::params!["2", "codex", "DeepSeek", codex_sc, Option::<String>::None],
            )
            .unwrap();
        }

        let cs = candidates_from_db(&db_path).unwrap();
        assert_eq!(cs.len(), 1, "同 host 的 claude+codex 应合并: {cs:?}");
        let c = &cs[0];
        assert_eq!(c.anthropic_base_url, "https://api.deepseek.com/anthropic");
        assert_eq!(c.openai_base_url, "https://api.deepseek.com/v1");
        assert_eq!(c.api_key, "sk-db-anth"); // anthropic 槽 key 优先
        assert_eq!(c.source_apps, vec!["claude".to_string(), "codex".to_string()]);

        std::fs::remove_file(&db_path).ok();
    }
}
