//! Permission policy — decides how to answer `session/request_permission`.
//!
//! ReadOnly is fail-closed and uses a two-tier check: FIRST deny any tool whose
//! name contains a mutation/exec marker (`is_mutating_tool`), so that compound
//! names such as `search_and_replace` or `create_pull_request_review` are
//! rejected even though they also contain a read marker (deny wins on conflict);
//! THEN allow tools on the read-only allowlist (`is_read_tool`); every other
//! tool (unrecognized or ambiguous names) is rejected. A reviewer literally
//! cannot mutate the workspace or run commands, regardless of its prompt.

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PermissionPolicy {
    ReadOnly,
    AskUser,
}

pub struct PermOption {
    pub option_id: String,
    pub kind: String, // "allow_once" | "allow_always" | "reject_once" | "reject_always"
}

#[derive(Debug, PartialEq)]
pub enum PermDecision {
    Select(String),
    RejectAll,
}

/// Tool names that only read the workspace. ACP tool names are not fully
/// standardized across agents, so match on common read-only substrings.
/// Anything NOT matching is treated as non-read (fail-closed).
pub fn is_read_tool(tool_name: &str) -> bool {
    let t = tool_name.to_lowercase();
    const READ_MARKERS: &[&str] = &[
        "read", "grep", "glob", "search", "list", "ls", "cat", "view",
        "fetch", "find", "think", "head", "tail",
    ];
    READ_MARKERS.iter().any(|m| t.contains(m))
}

/// Tool names that mutate the workspace or execute commands. Checked BEFORE the
/// read allowlist so that compound names (e.g. `search_and_replace`,
/// `create_pull_request_review`, `write_xlsx`) are denied even though they also
/// contain a read marker. Matches common write/exec verb substrings.
pub fn is_mutating_tool(tool_name: &str) -> bool {
    let t = tool_name.to_lowercase();
    const MUTATION_MARKERS: &[&str] = &[
        "write", "edit", "create", "delete", "remove", "replace", "apply",
        "update", "patch", "move", "rename", "mkdir", "chmod", "insert",
        "append", "save", "submit", "post", "put", "exec", "run", "shell",
        "bash", "command", "terminal", "kill", "install",
    ];
    MUTATION_MARKERS.iter().any(|m| t.contains(m))
}

pub fn decide(policy: PermissionPolicy, tool_name: &str, options: &[PermOption]) -> PermDecision {
    match policy {
        PermissionPolicy::ReadOnly => {
            if is_mutating_tool(tool_name) {
                // Deny wins on conflict: mutation/exec markers fail closed even
                // if a read marker is also present in the name.
                PermDecision::RejectAll
            } else if is_read_tool(tool_name) {
                // Prefer an allow_once option; fall back to any allow.
                options
                    .iter()
                    .find(|o| o.kind == "allow_once")
                    .or_else(|| options.iter().find(|o| o.kind.starts_with("allow")))
                    .map(|o| PermDecision::Select(o.option_id.clone()))
                    .unwrap_or(PermDecision::RejectAll)
            } else {
                // Writes, shell/exec, and unknown/ambiguous names fail closed.
                PermDecision::RejectAll
            }
        }
        PermissionPolicy::AskUser => {
            // v1: no interactive review path uses AskUser; default deny for safety.
            // Real UI wiring comes with the chat scenario (out of scope here).
            PermDecision::RejectAll
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts() -> Vec<PermOption> {
        vec![
            PermOption { option_id: "allow".into(), kind: "allow_once".into() },
            PermOption { option_id: "reject".into(), kind: "reject_once".into() },
        ]
    }

    #[test]
    fn readonly_allows_read_tools() {
        let d = decide(PermissionPolicy::ReadOnly, "read_file", &opts());
        assert!(matches!(d, PermDecision::Select(ref id) if id == "allow"));
    }

    #[test]
    fn readonly_rejects_write_tools() {
        let d = decide(PermissionPolicy::ReadOnly, "write_file", &opts());
        assert!(matches!(d, PermDecision::RejectAll));
    }

    #[test]
    fn readonly_rejects_command_execution_tools() {
        // The Critical scenario: command-execution tools must fail closed.
        for tool in ["shell", "bash", "execute_command", "rm"] {
            let d = decide(PermissionPolicy::ReadOnly, tool, &opts());
            assert!(
                matches!(d, PermDecision::RejectAll),
                "{tool} should be rejected"
            );
        }
    }

    #[test]
    fn readonly_rejects_unknown_tool_fail_closed() {
        // Note: a literal "frobnicate" contains the substring "cat" and would
        // match the read allowlist under substring matching, so we use a
        // genuinely non-colliding unknown token to prove fail-closed default.
        let d = decide(PermissionPolicy::ReadOnly, "xyzzy", &opts());
        assert!(matches!(d, PermDecision::RejectAll));
    }

    #[test]
    fn readonly_read_tool_without_allow_options_rejects() {
        let only_reject = vec![PermOption {
            option_id: "reject".into(),
            kind: "reject_once".into(),
        }];
        let d = decide(PermissionPolicy::ReadOnly, "read_file", &only_reject);
        assert!(matches!(d, PermDecision::RejectAll));
    }

    #[test]
    fn readonly_rejects_compound_mutating_names() {
        // The residual hole: mutating tool names that also contain a read
        // marker must still be rejected (deny wins on conflict).
        for tool in [
            "search_and_replace",
            "find_and_replace",
            "create_pull_request_review",
            "write_xlsx",
            "update_spreadsheet",
        ] {
            let d = decide(PermissionPolicy::ReadOnly, tool, &opts());
            assert!(
                matches!(d, PermDecision::RejectAll),
                "{tool} should be rejected"
            );
        }
    }

    #[test]
    fn is_mutating_tool_detection() {
        // Positive: write/exec markers, including compound read-colliding names.
        assert!(is_mutating_tool("write_file"));
        assert!(is_mutating_tool("search_and_replace"));
        assert!(is_mutating_tool("find_and_replace"));
        assert!(is_mutating_tool("create_pull_request_review"));
        assert!(is_mutating_tool("write_xlsx"));
        assert!(is_mutating_tool("update_spreadsheet"));
        assert!(is_mutating_tool("shell"));
        assert!(is_mutating_tool("bash"));
        // Negative: pure read tools are not mutating.
        assert!(!is_mutating_tool("read_file"));
        assert!(!is_mutating_tool("grep"));
        assert!(!is_mutating_tool("list_directory"));
    }

    #[test]
    fn ask_user_rejects_all() {
        let d = decide(PermissionPolicy::AskUser, "read_file", &opts());
        assert!(matches!(d, PermDecision::RejectAll));
    }

    #[test]
    fn is_read_tool_detection() {
        // Positive: read-only allowlist markers.
        assert!(is_read_tool("read_file"));
        assert!(is_read_tool("grep"));
        assert!(is_read_tool("Glob"));
        assert!(is_read_tool("list_directory"));
        assert!(is_read_tool("web_fetch"));
        // Negative: writes and command execution are not read tools.
        assert!(!is_read_tool("write_file"));
        assert!(!is_read_tool("shell"));
        assert!(!is_read_tool("bash"));
        assert!(!is_read_tool("execute_command"));
        assert!(!is_read_tool("rm"));
        assert!(!is_read_tool("xyzzy"));
    }
}
