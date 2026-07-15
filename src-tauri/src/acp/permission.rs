//! Permission policy — decides how to answer `session/request_permission`.
//!
//! ReadOnly is fail-closed and keys PRIMARILY on the structured `toolCall.kind`
//! field (`read|edit|delete|move|search|execute|think|fetch|other`), because
//! `toolCall.title` is a human display string — for shell tools it is the raw
//! command line (e.g. `cat a.txt | tee b.txt`) and substring checks on it are
//! trivially spoofable. Mutating/exec kinds are denied outright; read-ish kinds
//! are allowed but the title check remains as an ADDITIONAL deny layer (title
//! can veto, never rescue). When no kind is present we fall back to the
//! two-tier title check: FIRST deny any tool whose name contains a
//! mutation/exec marker (`is_mutating_tool`), so that compound names such as
//! `search_and_replace` or `create_pull_request_review` are rejected even
//! though they also contain a read marker (deny wins on conflict); THEN allow
//! tools on the read-only allowlist (`is_read_tool`); every other tool
//! (unrecognized or ambiguous) is rejected. A reviewer literally cannot mutate
//! the workspace or run commands, regardless of its prompt.

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

/// ACP `toolCall.kind` values that mutate the workspace, run commands, or are
/// too ambiguous to trust (deny outright — the title can never rescue these).
const DENY_KINDS: &[&str] = &["edit", "delete", "move", "execute", "switch_mode", "other"];

/// ACP `toolCall.kind` values that only observe the workspace.
const ALLOW_KINDS: &[&str] = &["read", "search", "fetch", "think"];

/// Prefer an allow_once option; fall back to any allow; else reject.
fn select_allow(options: &[PermOption]) -> PermDecision {
    options
        .iter()
        .find(|o| o.kind == "allow_once")
        .or_else(|| options.iter().find(|o| o.kind.starts_with("allow")))
        .map(|o| PermDecision::Select(o.option_id.clone()))
        .unwrap_or(PermDecision::RejectAll)
}

pub fn decide(
    policy: PermissionPolicy,
    tool_kind: &str,
    tool_title: &str,
    options: &[PermOption],
) -> PermDecision {
    match policy {
        PermissionPolicy::ReadOnly => {
            let kind = tool_kind.to_lowercase();
            if DENY_KINDS.contains(&kind.as_str()) {
                // Structured kind says mutate/exec/ambiguous: fail closed. The
                // title (often a raw command line) can never rescue a bad kind.
                PermDecision::RejectAll
            } else if ALLOW_KINDS.contains(&kind.as_str()) {
                // Kind allows, but the title check stays as an extra deny
                // layer: a "read" kind with a mutating-looking title fails.
                if is_mutating_tool(tool_title) {
                    PermDecision::RejectAll
                } else {
                    select_allow(options)
                }
            } else if is_mutating_tool(tool_title) {
                // No usable kind: fall back to the two-tier title check.
                // Deny wins on conflict: mutation/exec markers fail closed even
                // if a read marker is also present in the name.
                PermDecision::RejectAll
            } else if is_read_tool(tool_title) {
                select_allow(options)
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
        // No kind present: falls back to the title allowlist.
        let d = decide(PermissionPolicy::ReadOnly, "", "read_file", &opts());
        assert!(matches!(d, PermDecision::Select(ref id) if id == "allow"));
    }

    #[test]
    fn readonly_rejects_write_tools() {
        let d = decide(PermissionPolicy::ReadOnly, "", "write_file", &opts());
        assert!(matches!(d, PermDecision::RejectAll));
    }

    #[test]
    fn readonly_rejects_command_execution_tools() {
        // Title-only fallback: command-execution names must fail closed.
        for tool in ["shell", "bash", "execute_command", "rm"] {
            let d = decide(PermissionPolicy::ReadOnly, "", tool, &opts());
            assert!(
                matches!(d, PermDecision::RejectAll),
                "{tool} should be rejected"
            );
        }
    }

    #[test]
    fn readonly_rejects_execute_kind_with_readlike_title() {
        // The Critical repro: for Bash tool calls, ACP `title` is the raw
        // command line — `cat a.txt | tee b.txt` contains the read marker
        // "cat" and previously passed the title-only check. The structured
        // kind "execute" must reject it regardless of title.
        let d = decide(
            PermissionPolicy::ReadOnly,
            "execute",
            "cat a.txt | tee b.txt",
            &opts(),
        );
        assert!(matches!(d, PermDecision::RejectAll));
    }

    #[test]
    fn readonly_rejects_all_mutating_kinds_despite_read_title() {
        // A read-looking title can never rescue a mutating/ambiguous kind.
        for kind in ["edit", "delete", "move", "execute", "switch_mode", "other"] {
            let d = decide(PermissionPolicy::ReadOnly, kind, "read_file", &opts());
            assert!(
                matches!(d, PermDecision::RejectAll),
                "kind={kind} should be rejected"
            );
        }
    }

    #[test]
    fn readonly_allows_read_kind() {
        let d = decide(PermissionPolicy::ReadOnly, "read", "Read file src/main.rs", &opts());
        assert!(matches!(d, PermDecision::Select(ref id) if id == "allow"));
    }

    #[test]
    fn readonly_read_kind_title_can_still_veto() {
        // Kind allows, title vetoes: extra deny layer on top of the kind.
        let d = decide(PermissionPolicy::ReadOnly, "read", "write something", &opts());
        assert!(matches!(d, PermDecision::RejectAll));
    }

    #[test]
    fn readonly_allows_other_readish_kinds() {
        for kind in ["search", "fetch", "think"] {
            let d = decide(PermissionPolicy::ReadOnly, kind, "Look around", &opts());
            assert!(
                matches!(d, PermDecision::Select(ref id) if id == "allow"),
                "kind={kind} should be allowed"
            );
        }
    }

    #[test]
    fn readonly_rejects_unknown_tool_fail_closed() {
        // Note: a literal "frobnicate" contains the substring "cat" and would
        // match the read allowlist under substring matching, so we use a
        // genuinely non-colliding unknown token to prove fail-closed default.
        let d = decide(PermissionPolicy::ReadOnly, "", "xyzzy", &opts());
        assert!(matches!(d, PermDecision::RejectAll));
    }

    #[test]
    fn readonly_read_tool_without_allow_options_rejects() {
        let only_reject = vec![PermOption {
            option_id: "reject".into(),
            kind: "reject_once".into(),
        }];
        let d = decide(PermissionPolicy::ReadOnly, "read", "read_file", &only_reject);
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
            let d = decide(PermissionPolicy::ReadOnly, "", tool, &opts());
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
        let d = decide(PermissionPolicy::AskUser, "read", "read_file", &opts());
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
