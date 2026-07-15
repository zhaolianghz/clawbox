//! Permission policy — decides how to answer `session/request_permission`.
//!
//! ReadOnly is the policy used for all v1 review roles: any tool that could
//! mutate the workspace is rejected at the protocol layer, so a reviewer
//! literally cannot write, regardless of its prompt.

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

/// Tool names that mutate the workspace. ACP tool names are not fully
/// standardized across agents, so match on common substrings.
pub fn is_write_tool(tool_name: &str) -> bool {
    let t = tool_name.to_lowercase();
    const WRITE_MARKERS: &[&str] = &[
        "write", "edit", "create", "delete", "remove", "apply_patch",
        "patch", "move", "rename", "mkdir", "chmod",
    ];
    WRITE_MARKERS.iter().any(|m| t.contains(m))
}

pub fn decide(policy: PermissionPolicy, tool_name: &str, options: &[PermOption]) -> PermDecision {
    match policy {
        PermissionPolicy::ReadOnly => {
            if is_write_tool(tool_name) {
                PermDecision::RejectAll
            } else {
                // Prefer an allow_once option; fall back to any allow.
                options
                    .iter()
                    .find(|o| o.kind == "allow_once")
                    .or_else(|| options.iter().find(|o| o.kind.starts_with("allow")))
                    .map(|o| PermDecision::Select(o.option_id.clone()))
                    .unwrap_or(PermDecision::RejectAll)
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
    fn readonly_rejects_write_tools() {
        let d = decide(PermissionPolicy::ReadOnly, "write_file", &opts());
        assert!(matches!(d, PermDecision::RejectAll));
    }

    #[test]
    fn readonly_allows_read_tools() {
        let d = decide(PermissionPolicy::ReadOnly, "read_file", &opts());
        assert!(matches!(d, PermDecision::Select(ref id) if id == "allow"));
    }

    #[test]
    fn is_write_tool_detection() {
        assert!(is_write_tool("write_file"));
        assert!(is_write_tool("edit"));
        assert!(is_write_tool("apply_patch"));
        assert!(!is_write_tool("read_file"));
        assert!(!is_write_tool("grep"));
    }
}
