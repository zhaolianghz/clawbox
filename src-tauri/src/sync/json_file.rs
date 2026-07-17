//! JSON-file adapters: claude-code, opencode, cursor-agent, codebuddy.
//!
//! All four agents keep MCP servers in a JSON object inside a larger config
//! file. One generic adapter handles them, parameterized by file path, the
//! key holding the server map, and a per-agent spec→native mapping. The
//! merge-write reparses the whole document and only mutates entries ClawBox
//! manages — every other key survives (semantically; formatting is
//! serde_json pretty-print).

use super::{diff_changes, ChangeItem, ConfigAdapter};
use crate::commands::config::McpServerSpec;
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

type MapFn = fn(&McpServerSpec) -> Result<Value, String>;

pub struct JsonFileAdapter {
    id: &'static str,
    /// Path segments under $HOME, e.g. `[".config", "opencode", "opencode.json"]`.
    rel: &'static [&'static str],
    /// Top-level key holding the name→server object ("mcpServers" or "mcp").
    servers_key: &'static str,
    map: MapFn,
    /// Document skeleton used when the target file does not exist yet.
    skeleton: fn() -> Value,
}

pub fn claude_code() -> JsonFileAdapter {
    JsonFileAdapter {
        id: "claude-code",
        rel: &[".claude.json"],
        servers_key: "mcpServers",
        map: map_typed,
        skeleton: || json!({}),
    }
}

pub fn codebuddy() -> JsonFileAdapter {
    JsonFileAdapter {
        id: "codebuddy",
        rel: &[".codebuddy", "mcp.json"],
        servers_key: "mcpServers",
        map: map_typed,
        skeleton: || json!({}),
    }
}

pub fn cursor_agent() -> JsonFileAdapter {
    JsonFileAdapter {
        id: "cursor-agent",
        rel: &[".cursor", "mcp.json"],
        servers_key: "mcpServers",
        map: map_untyped,
        skeleton: || json!({}),
    }
}

pub fn opencode() -> JsonFileAdapter {
    JsonFileAdapter {
        id: "opencode",
        rel: &[".config", "opencode", "opencode.json"],
        servers_key: "mcp",
        map: map_opencode,
        skeleton: || json!({"$schema": "https://opencode.ai/config.json"}),
    }
}

/// claude-code / codebuddy: `{"type":"stdio","command",...}` / `{"type":"http","url",...}`.
fn map_typed(spec: &McpServerSpec) -> Result<Value, String> {
    match spec.kind.as_str() {
        "stdio" => {
            let cmd = spec
                .command
                .as_deref()
                .filter(|c| !c.is_empty())
                .ok_or_else(|| "stdio server has no command".to_string())?;
            let mut o = Map::new();
            o.insert("type".into(), json!("stdio"));
            o.insert("command".into(), json!(cmd));
            if !spec.args.is_empty() {
                o.insert("args".into(), json!(spec.args));
            }
            if !spec.env.is_empty() {
                o.insert("env".into(), json!(spec.env));
            }
            Ok(Value::Object(o))
        }
        "http" => {
            let url = spec
                .url
                .as_deref()
                .filter(|u| !u.is_empty())
                .ok_or_else(|| "http server has no url".to_string())?;
            let mut o = Map::new();
            o.insert("type".into(), json!("http"));
            o.insert("url".into(), json!(url));
            if !spec.headers.is_empty() {
                o.insert("headers".into(), json!(spec.headers));
            }
            Ok(Value::Object(o))
        }
        other => Err(format!("unsupported server kind: {}", other)),
    }
}

/// cursor-agent: same shape but without the "type" discriminator.
fn map_untyped(spec: &McpServerSpec) -> Result<Value, String> {
    let mut v = map_typed(spec)?;
    v.as_object_mut().unwrap().remove("type");
    Ok(v)
}

/// opencode: stdio→local (command is an array, env is "environment"),
/// http→remote.
fn map_opencode(spec: &McpServerSpec) -> Result<Value, String> {
    match spec.kind.as_str() {
        "stdio" => {
            let cmd = spec
                .command
                .as_deref()
                .filter(|c| !c.is_empty())
                .ok_or_else(|| "stdio server has no command".to_string())?;
            let mut command = vec![cmd.to_string()];
            command.extend(spec.args.iter().cloned());
            let mut o = Map::new();
            o.insert("type".into(), json!("local"));
            o.insert("command".into(), json!(command));
            if !spec.env.is_empty() {
                o.insert("environment".into(), json!(spec.env));
            }
            o.insert("enabled".into(), json!(true));
            Ok(Value::Object(o))
        }
        "http" => {
            let url = spec
                .url
                .as_deref()
                .filter(|u| !u.is_empty())
                .ok_or_else(|| "http server has no url".to_string())?;
            Ok(json!({"type": "remote", "url": url, "enabled": true}))
        }
        other => Err(format!("unsupported server kind: {}", other)),
    }
}

impl JsonFileAdapter {
    fn load(&self, home: &Path) -> Result<Value, String> {
        let path = self.config_path(home);
        if !path.exists() {
            return Ok((self.skeleton)());
        }
        let content = std::fs::read_to_string(&path)
            .map_err(|e| format!("failed to read {}: {}", path.display(), e))?;
        let doc: Value = serde_json::from_str(&content)
            .map_err(|e| format!("failed to parse {}: {}", path.display(), e))?;
        if !doc.is_object() {
            return Err(format!("{}: root is not a JSON object", path.display()));
        }
        Ok(doc)
    }

    /// Enabled desired servers rendered to this agent's native shape;
    /// Err(reason) becomes an action="skip" plan item.
    fn mapped(&self, desired: &BTreeMap<String, McpServerSpec>) -> BTreeMap<String, Result<Value, String>> {
        desired
            .iter()
            .filter(|(_, s)| s.enabled)
            .map(|(n, s)| (n.clone(), (self.map)(s)))
            .collect()
    }

    fn servers<'a>(&self, doc: &'a Value) -> Result<Option<&'a Map<String, Value>>, String> {
        match doc.get(self.servers_key) {
            None => Ok(None),
            Some(v) => v
                .as_object()
                .map(Some)
                .ok_or_else(|| format!("\"{}\" is not a JSON object", self.servers_key)),
        }
    }
}

impl ConfigAdapter for JsonFileAdapter {
    fn agent_id(&self) -> &'static str {
        self.id
    }

    fn config_path(&self, home: &Path) -> PathBuf {
        self.rel.iter().fold(home.to_path_buf(), |p, seg| p.join(seg))
    }

    fn plan_mcp(
        &self,
        home: &Path,
        desired: &BTreeMap<String, McpServerSpec>,
        managed: &[String],
    ) -> Result<Vec<ChangeItem>, String> {
        let doc = self.load(home)?;
        let servers = self.servers(&doc)?;
        let mapped = self.mapped(desired);
        Ok(diff_changes(&mapped, managed, |name| {
            servers.and_then(|s| s.get(name).cloned())
        }))
    }

    fn apply_mcp(
        &self,
        home: &Path,
        desired: &BTreeMap<String, McpServerSpec>,
        managed: &[String],
    ) -> Result<usize, String> {
        let mut doc = self.load(home)?;
        // Validate before mutating so a malformed servers key aborts cleanly.
        self.servers(&doc)?;
        let mapped = self.mapped(desired);

        let root = doc.as_object_mut().unwrap();
        let servers = root
            .entry(self.servers_key.to_string())
            .or_insert_with(|| Value::Object(Map::new()))
            .as_object_mut()
            .unwrap();

        let mut applied = 0;
        for (name, rendered) in &mapped {
            if let Ok(value) = rendered {
                if servers.get(name) != Some(value) {
                    servers.insert(name.clone(), value.clone());
                    applied += 1;
                }
            }
        }
        for name in managed {
            let still_deployed = matches!(mapped.get(name), Some(Ok(_)));
            if !still_deployed && servers.remove(name).is_some() {
                applied += 1;
            }
        }

        let path = self.config_path(home);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)
                .map_err(|e| format!("failed to create {}: {}", dir.display(), e))?;
        }
        let content = serde_json::to_string_pretty(&doc)
            .map_err(|e| format!("failed to serialize {}: {}", path.display(), e))?;
        std::fs::write(&path, content + "\n")
            .map_err(|e| format!("failed to write {}: {}", path.display(), e))?;
        Ok(applied)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sync::test_util::*;

    fn desired_two() -> BTreeMap<String, McpServerSpec> {
        let mut d = BTreeMap::new();
        d.insert("codegraph".to_string(), stdio_spec("codegraph", &["serve", "--mcp"]));
        d.insert("ctx7".to_string(), http_spec("https://mcp.context7.com/mcp"));
        d
    }

    fn read_doc(home: &Path, adapter: &JsonFileAdapter) -> Value {
        serde_json::from_str(&std::fs::read_to_string(adapter.config_path(home)).unwrap()).unwrap()
    }

    fn all_adapters() -> Vec<JsonFileAdapter> {
        vec![claude_code(), codebuddy(), cursor_agent(), opencode()]
    }

    // -- generic suite, run against every JSON adapter ----------------------

    #[test]
    fn missing_file_plans_all_add_and_apply_creates_it() {
        for adapter in all_adapters() {
            let home = TempHome::new();
            let desired = desired_two();
            let changes = adapter.plan_mcp(home.path(), &desired, &[]).unwrap();
            assert_eq!(changes.len(), 2, "{}", adapter.id);
            assert!(changes.iter().all(|c| c.action == "add"), "{}", adapter.id);

            let applied = adapter.apply_mcp(home.path(), &desired, &[]).unwrap();
            assert_eq!(applied, 2, "{}", adapter.id);
            let doc = read_doc(home.path(), &adapter);
            let servers = doc[adapter.servers_key].as_object().unwrap();
            assert!(servers.contains_key("codegraph"), "{}", adapter.id);
            assert!(servers.contains_key("ctx7"), "{}", adapter.id);
        }
    }

    #[test]
    fn apply_is_idempotent_and_replan_is_unchanged() {
        for adapter in all_adapters() {
            let home = TempHome::new();
            let desired = desired_two();
            adapter.apply_mcp(home.path(), &desired, &[]).unwrap();
            let managed = vec!["codegraph".to_string(), "ctx7".to_string()];

            let changes = adapter.plan_mcp(home.path(), &desired, &managed).unwrap();
            assert!(
                changes.iter().all(|c| c.action == "unchanged"),
                "{}: {:?}",
                adapter.id,
                changes
            );
            let before = read_doc(home.path(), &adapter);
            let applied = adapter.apply_mcp(home.path(), &desired, &managed).unwrap();
            assert_eq!(applied, 0, "{}", adapter.id);
            assert_eq!(before, read_doc(home.path(), &adapter), "{}", adapter.id);
        }
    }

    #[test]
    fn unrelated_keys_and_servers_survive_apply() {
        for adapter in all_adapters() {
            let home = TempHome::new();
            // Simulated user file: private top-level state + a server ClawBox
            // does not manage.
            let existing = json!({
                "numStartups": 42,
                "projects": {"/tmp/x": {"history": ["a", "b"]}},
                adapter.servers_key: {
                    "user_private": {"command": "secret-tool", "args": ["--x"]}
                }
            });
            write_file(
                home.path(),
                &adapter.rel.iter().collect::<PathBuf>(),
                &serde_json::to_string_pretty(&existing).unwrap(),
            );

            adapter.apply_mcp(home.path(), &desired_two(), &[]).unwrap();
            let doc = read_doc(home.path(), &adapter);
            assert_eq!(doc["numStartups"], json!(42), "{}", adapter.id);
            assert_eq!(doc["projects"]["/tmp/x"]["history"], json!(["a", "b"]), "{}", adapter.id);
            assert_eq!(
                doc[adapter.servers_key]["user_private"],
                json!({"command": "secret-tool", "args": ["--x"]}),
                "{}",
                adapter.id
            );
            assert!(doc[adapter.servers_key].get("codegraph").is_some(), "{}", adapter.id);
        }
    }

    #[test]
    fn existing_entry_with_different_content_plans_update() {
        for adapter in all_adapters() {
            let home = TempHome::new();
            let existing = json!({
                adapter.servers_key: {"codegraph": {"command": "old-binary"}}
            });
            write_file(
                home.path(),
                &adapter.rel.iter().collect::<PathBuf>(),
                &existing.to_string(),
            );
            let changes = adapter.plan_mcp(home.path(), &desired_two(), &[]).unwrap();
            assert_eq!(action_of(&changes, "codegraph"), "update", "{}", adapter.id);
            assert_eq!(action_of(&changes, "ctx7"), "add", "{}", adapter.id);

            adapter.apply_mcp(home.path(), &desired_two(), &[]).unwrap();
            let doc = read_doc(home.path(), &adapter);
            assert_ne!(doc[adapter.servers_key]["codegraph"], json!({"command": "old-binary"}));
        }
    }

    #[test]
    fn managed_but_no_longer_desired_is_removed_others_kept() {
        for adapter in all_adapters() {
            let home = TempHome::new();
            let existing = json!({
                "private": true,
                adapter.servers_key: {
                    "gone": {"command": "was-ours"},
                    "user_private": {"command": "not-ours"}
                }
            });
            write_file(
                home.path(),
                &adapter.rel.iter().collect::<PathBuf>(),
                &existing.to_string(),
            );
            let managed = vec!["gone".to_string()];
            let desired = desired_two(); // no "gone"

            let changes = adapter.plan_mcp(home.path(), &desired, &managed).unwrap();
            assert_eq!(action_of(&changes, "gone"), "remove", "{}", adapter.id);

            adapter.apply_mcp(home.path(), &desired, &managed).unwrap();
            let doc = read_doc(home.path(), &adapter);
            assert!(doc[adapter.servers_key].get("gone").is_none(), "{}", adapter.id);
            assert!(doc[adapter.servers_key].get("user_private").is_some(), "{}", adapter.id);
            assert_eq!(doc["private"], json!(true), "{}", adapter.id);
        }
    }

    #[test]
    fn disabled_desired_server_is_treated_as_remove() {
        for adapter in all_adapters() {
            let home = TempHome::new();
            let mut desired = desired_two();
            adapter.apply_mcp(home.path(), &desired, &[]).unwrap();

            desired.get_mut("codegraph").unwrap().enabled = false;
            let managed = vec!["codegraph".to_string(), "ctx7".to_string()];
            let changes = adapter.plan_mcp(home.path(), &desired, &managed).unwrap();
            assert_eq!(action_of(&changes, "codegraph"), "remove", "{}", adapter.id);
            assert_eq!(action_of(&changes, "ctx7"), "unchanged", "{}", adapter.id);

            adapter.apply_mcp(home.path(), &desired, &managed).unwrap();
            let doc = read_doc(home.path(), &adapter);
            assert!(doc[adapter.servers_key].get("codegraph").is_none(), "{}", adapter.id);
        }
    }

    #[test]
    fn corrupt_target_file_plans_error_not_panic() {
        for adapter in all_adapters() {
            let home = TempHome::new();
            write_file(
                home.path(),
                &adapter.rel.iter().collect::<PathBuf>(),
                "{ not json",
            );
            let err = adapter.plan_mcp(home.path(), &desired_two(), &[]).unwrap_err();
            assert!(err.contains("parse"), "{}: {}", adapter.id, err);
        }
    }

    // -- per-agent mapping shapes -------------------------------------------

    #[test]
    fn claude_mapping_shapes() {
        let mut spec = stdio_spec("codegraph", &["serve"]);
        spec.env.insert("K".into(), "V".into());
        assert_eq!(
            map_typed(&spec).unwrap(),
            json!({"type": "stdio", "command": "codegraph", "args": ["serve"], "env": {"K": "V"}})
        );
        let mut h = http_spec("https://x/mcp");
        h.headers.insert("Authorization".into(), "Bearer t".into());
        assert_eq!(
            map_typed(&h).unwrap(),
            json!({"type": "http", "url": "https://x/mcp", "headers": {"Authorization": "Bearer t"}})
        );
    }

    #[test]
    fn cursor_mapping_has_no_type_field() {
        let spec = stdio_spec("codegraph", &["serve"]);
        assert_eq!(
            map_untyped(&spec).unwrap(),
            json!({"command": "codegraph", "args": ["serve"]})
        );
        assert_eq!(
            map_untyped(&http_spec("https://x/mcp")).unwrap(),
            json!({"url": "https://x/mcp"})
        );
    }

    #[test]
    fn opencode_mapping_uses_command_array_and_environment() {
        let mut spec = stdio_spec("codegraph", &["serve", "--mcp"]);
        spec.env.insert("K".into(), "V".into());
        assert_eq!(
            map_opencode(&spec).unwrap(),
            json!({
                "type": "local",
                "command": ["codegraph", "serve", "--mcp"],
                "environment": {"K": "V"},
                "enabled": true
            })
        );
        assert_eq!(
            map_opencode(&http_spec("https://x/mcp")).unwrap(),
            json!({"type": "remote", "url": "https://x/mcp", "enabled": true})
        );
    }

    #[test]
    fn stdio_without_command_is_skipped_in_plan() {
        let home = TempHome::new();
        let adapter = claude_code();
        let mut desired = BTreeMap::new();
        let mut broken = stdio_spec("", &[]);
        broken.command = None;
        desired.insert("broken".to_string(), broken);
        let changes = adapter.plan_mcp(home.path(), &desired, &[]).unwrap();
        assert_eq!(action_of(&changes, "broken"), "skip");
    }
}
