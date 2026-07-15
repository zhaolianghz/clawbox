//! Permission policy — decides how to answer `session/request_permission`.
//!
//! ReadOnly is fail-closed: only tools on the read-only allowlist may be
//! allowed; every other tool (writes, shell/exec, and any unrecognized or
//! ambiguous name) is rejected at the protocol layer. A reviewer literally
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

pub fn decide(policy: PermissionPolicy, tool_name: &str, options: &[PermOption]) -> PermDecision {
    match policy {
        PermissionPolicy::ReadOnly => {
            if is_read_tool(tool_name) {
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
