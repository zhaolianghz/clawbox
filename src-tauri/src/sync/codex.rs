//! codex adapter: `~/.codex/config.toml`, `[mcp_servers.NAME]` tables.
//!
//! Uses toml_edit so user comments/formatting outside the entries we manage
//! are preserved byte-for-byte. codex has no http transport — http specs are
//! planned as action="skip" and never written (nor recorded as managed).

use super::{diff_changes, ChangeItem, ConfigAdapter};
use crate::commands::config::McpServerSpec;
use serde_json::json;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use toml_edit::{value, DocumentMut, Item, Table};

pub struct CodexAdapter;

const SERVERS_KEY: &str = "mcp_servers";

/// Rendered form of a spec: the toml_edit table we would write plus its
/// serde_json image used for plan/apply equality checks.
fn render(spec: &McpServerSpec) -> Result<(Table, serde_json::Value), String> {
    match spec.kind.as_str() {
        "stdio" => {
            let cmd = spec
                .command
                .as_deref()
                .filter(|c| !c.is_empty())
                .ok_or_else(|| "stdio server has no command".to_string())?;
            let mut t = Table::new();
            t["type"] = value("stdio");
            t["command"] = value(cmd);
            let mut j = serde_json::Map::new();
            j.insert("type".into(), json!("stdio"));
            j.insert("command".into(), json!(cmd));
            if !spec.args.is_empty() {
                let mut arr = toml_edit::Array::new();
                for a in &spec.args {
                    arr.push(a.as_str());
                }
                t["args"] = value(arr);
                j.insert("args".into(), json!(spec.args));
            }
            if !spec.env.is_empty() {
                let mut e = Table::new();
                for (k, v) in &spec.env {
                    e[k] = value(v.as_str());
                }
                t["env"] = Item::Table(e);
                j.insert("env".into(), json!(spec.env));
            }
            Ok((t, serde_json::Value::Object(j)))
        }
        "http" => Err("codex does not support http servers".to_string()),
        other => Err(format!("unsupported server kind: {}", other)),
    }
}

/// Convert a toml_edit item to serde_json for semantic comparison.
fn item_to_json(item: &Item) -> serde_json::Value {
    match item {
        Item::None => serde_json::Value::Null,
        Item::Value(v) => toml_value_to_json(v),
        Item::Table(t) => table_to_json(t),
        Item::ArrayOfTables(a) => {
            serde_json::Value::Array(a.iter().map(table_to_json).collect())
        }
    }
}

fn table_to_json(t: &Table) -> serde_json::Value {
    let mut m = serde_json::Map::new();
    for (k, v) in t.iter() {
        m.insert(k.to_string(), item_to_json(v));
    }
    serde_json::Value::Object(m)
}

fn toml_value_to_json(v: &toml_edit::Value) -> serde_json::Value {
    use toml_edit::Value as V;
    match v {
        V::String(s) => json!(s.value()),
        V::Integer(i) => json!(*i.value()),
        V::Float(f) => json!(*f.value()),
        V::Boolean(b) => json!(*b.value()),
        V::Datetime(d) => json!(d.value().to_string()),
        V::Array(a) => serde_json::Value::Array(a.iter().map(toml_value_to_json).collect()),
        V::InlineTable(t) => {
            let mut m = serde_json::Map::new();
            for (k, val) in t.iter() {
                m.insert(k.to_string(), toml_value_to_json(val));
            }
            serde_json::Value::Object(m)
        }
    }
}

impl CodexAdapter {
    fn load(&self, home: &Path) -> Result<DocumentMut, String> {
        let path = self.config_path(home);
        if !path.exists() {
            return Ok(DocumentMut::new());
        }
        let content = std::fs::read_to_string(&path)
            .map_err(|e| format!("failed to read {}: {}", path.display(), e))?;
        content
            .parse::<DocumentMut>()
            .map_err(|e| format!("failed to parse {}: {}", path.display(), e))
    }

    fn mapped(
        &self,
        desired: &BTreeMap<String, McpServerSpec>,
    ) -> BTreeMap<String, Result<(Table, serde_json::Value), String>> {
        desired
            .iter()
            .filter(|(_, s)| s.enabled)
            .map(|(n, s)| (n.clone(), render(s)))
            .collect()
    }
}

impl ConfigAdapter for CodexAdapter {
    fn agent_id(&self) -> &'static str {
        "codex"
    }

    fn config_path(&self, home: &Path) -> PathBuf {
        home.join(".codex").join("config.toml")
    }

    fn plan_mcp(
        &self,
        home: &Path,
        desired: &BTreeMap<String, McpServerSpec>,
        managed: &[String],
    ) -> Result<Vec<ChangeItem>, String> {
        let doc = self.load(home)?;
        let servers = doc.get(SERVERS_KEY).and_then(|i| i.as_table());
        // Reuse the shared JSON differ by projecting both sides to serde_json.
        let mapped: BTreeMap<String, Result<serde_json::Value, String>> = self
            .mapped(desired)
            .into_iter()
            .map(|(n, r)| (n, r.map(|(_, j)| j)))
            .collect();
        Ok(diff_changes(&mapped, managed, |name| {
            servers.and_then(|s| s.get(name)).map(item_to_json)
        }))
    }

    fn apply_mcp(
        &self,
        home: &Path,
        desired: &BTreeMap<String, McpServerSpec>,
        managed: &[String],
    ) -> Result<usize, String> {
        let mut doc = self.load(home)?;
        if doc.get(SERVERS_KEY).is_some() && doc[SERVERS_KEY].as_table().is_none() {
            return Err(format!("\"{}\" is not a TOML table", SERVERS_KEY));
        }
        let mapped = self.mapped(desired);
        let mut applied = 0;

        for (name, rendered) in &mapped {
            if let Ok((table, expected)) = rendered {
                let current = doc
                    .get(SERVERS_KEY)
                    .and_then(|i| i.as_table())
                    .and_then(|s| s.get(name))
                    .map(item_to_json);
                if current.as_ref() != Some(expected) {
                    if doc.get(SERVERS_KEY).is_none() {
                        let mut parent = Table::new();
                        parent.set_implicit(true); // render only [mcp_servers.NAME] headers
                        doc.insert(SERVERS_KEY, Item::Table(parent));
                    }
                    doc[SERVERS_KEY][name] = Item::Table(table.clone());
                    applied += 1;
                }
            }
        }
        for name in managed {
            let still_deployed = matches!(mapped.get(name), Some(Ok(_)));
            if !still_deployed {
                if let Some(servers) = doc.get_mut(SERVERS_KEY).and_then(|i| i.as_table_mut()) {
                    if servers.remove(name).is_some() {
                        applied += 1;
                    }
                }
            }
        }

        let path = self.config_path(home);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)
                .map_err(|e| format!("failed to create {}: {}", dir.display(), e))?;
        }
        std::fs::write(&path, doc.to_string())
            .map_err(|e| format!("failed to write {}: {}", path.display(), e))?;
        Ok(applied)
    }

    /// http specs are skipped by codex and must not enter mcp_managed.
    fn deployed_names(&self, desired: &BTreeMap<String, McpServerSpec>) -> Vec<String> {
        desired
            .iter()
            .filter(|(_, s)| s.enabled && s.kind == "stdio")
            .map(|(n, _)| n.clone())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sync::test_util::*;

    fn rel() -> PathBuf {
        PathBuf::from(".codex").join("config.toml")
    }

    fn read_toml(home: &Path) -> String {
        std::fs::read_to_string(CodexAdapter.config_path(home)).unwrap()
    }

    fn desired_stdio() -> BTreeMap<String, McpServerSpec> {
        let mut d = BTreeMap::new();
        let mut spec = stdio_spec("codegraph", &["serve", "--mcp"]);
        spec.env.insert("RUST_LOG".into(), "info".into());
        d.insert("codegraph".to_string(), spec);
        d
    }

    #[test]
    fn missing_file_plans_add_and_apply_writes_table() {
        let home = TempHome::new();
        let desired = desired_stdio();
        let changes = CodexAdapter.plan_mcp(home.path(), &desired, &[]).unwrap();
        assert_eq!(action_of(&changes, "codegraph"), "add");

        let applied = CodexAdapter.apply_mcp(home.path(), &desired, &[]).unwrap();
        assert_eq!(applied, 1);
        let text = read_toml(home.path());
        assert!(text.contains("[mcp_servers.codegraph]"), "{}", text);
        assert!(text.contains("[mcp_servers.codegraph.env]"), "{}", text);
        assert!(text.contains("command = \"codegraph\""), "{}", text);
        // The implicit parent table must not render a bare [mcp_servers] header.
        assert!(!text.contains("[mcp_servers]\n"), "{}", text);
    }

    #[test]
    fn http_spec_is_skipped_and_not_managed() {
        let home = TempHome::new();
        let mut desired = BTreeMap::new();
        desired.insert("remote".to_string(), http_spec("https://x/mcp"));
        let changes = CodexAdapter.plan_mcp(home.path(), &desired, &[]).unwrap();
        assert_eq!(action_of(&changes, "remote"), "skip");
        assert!(changes[0].detail.contains("does not support http"));

        let applied = CodexAdapter.apply_mcp(home.path(), &desired, &[]).unwrap();
        assert_eq!(applied, 0);
        assert!(!read_toml(home.path()).contains("remote"));
        assert!(CodexAdapter.deployed_names(&desired).is_empty());
    }

    #[test]
    fn comments_and_unrelated_tables_survive_apply() {
        let home = TempHome::new();
        let existing = r#"# user comment at top
model = "gpt-5.2-codex"

[profiles.fast]
model_reasoning_effort = "low" # inline comment

[mcp_servers.node_repl]
type = "stdio"
command = "/Applications/Codex.app/node_repl"
startup_timeout_sec = 120
"#;
        write_file(home.path(), &rel(), existing);

        CodexAdapter.apply_mcp(home.path(), &desired_stdio(), &[]).unwrap();
        let text = read_toml(home.path());
        assert!(text.contains("# user comment at top"));
        assert!(text.contains("# inline comment"));
        assert!(text.contains("model = \"gpt-5.2-codex\""));
        assert!(text.contains("[profiles.fast]"));
        assert!(text.contains("startup_timeout_sec = 120"));
        assert!(text.contains("[mcp_servers.codegraph]"));
    }

    #[test]
    fn same_name_different_content_plans_update_then_unchanged() {
        let home = TempHome::new();
        write_file(
            home.path(),
            &rel(),
            "[mcp_servers.codegraph]\ntype = \"stdio\"\ncommand = \"old\"\n",
        );
        let desired = desired_stdio();
        let changes = CodexAdapter.plan_mcp(home.path(), &desired, &[]).unwrap();
        assert_eq!(action_of(&changes, "codegraph"), "update");

        CodexAdapter.apply_mcp(home.path(), &desired, &[]).unwrap();
        let managed = vec!["codegraph".to_string()];
        let changes = CodexAdapter.plan_mcp(home.path(), &desired, &managed).unwrap();
        assert_eq!(action_of(&changes, "codegraph"), "unchanged");
        // Idempotent: second apply rewrites nothing.
        let before = read_toml(home.path());
        assert_eq!(CodexAdapter.apply_mcp(home.path(), &desired, &managed).unwrap(), 0);
        assert_eq!(before, read_toml(home.path()));
    }

    #[test]
    fn managed_entry_no_longer_desired_is_removed_others_kept() {
        let home = TempHome::new();
        let existing = r#"[mcp_servers.gone]
type = "stdio"
command = "was-ours"

# keep me — user comment attached to their own table
[mcp_servers.user_private]
type = "stdio"
command = "not-ours"
"#;
        write_file(home.path(), &rel(), existing);
        let managed = vec!["gone".to_string()];
        let desired = desired_stdio();

        let changes = CodexAdapter.plan_mcp(home.path(), &desired, &managed).unwrap();
        assert_eq!(action_of(&changes, "gone"), "remove");

        CodexAdapter.apply_mcp(home.path(), &desired, &managed).unwrap();
        let text = read_toml(home.path());
        assert!(!text.contains("[mcp_servers.gone]"));
        assert!(text.contains("[mcp_servers.user_private]"));
        assert!(text.contains("# keep me"));
    }

    #[test]
    fn corrupt_toml_plans_error_not_panic() {
        let home = TempHome::new();
        write_file(home.path(), &rel(), "[mcp_servers\nbroken");
        let err = CodexAdapter
            .plan_mcp(home.path(), &desired_stdio(), &[])
            .unwrap_err();
        assert!(err.contains("parse"), "{}", err);
    }
}
