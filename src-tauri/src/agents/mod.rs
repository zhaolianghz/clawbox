//! Unified agent registry — the single authority for every agent CLI ClawBox
//! can detect/install. Spec: docs/superpowers/specs/2026-07-16-agent-management-center-design.md

pub mod install;

use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::OnceLock;
use std::time::Duration;

#[derive(Serialize, Clone, Copy, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum AgentKind {
    NativeCli,
    Runtime,
    Gateway,
}

pub enum InstallMethod {
    Npm { package: &'static str, force: bool },
    Pipx { package: &'static str },
    Script {
        /// Unix (macOS/Linux) installer URL, run via `curl -fsSL {url} | bash`.
        unix: &'static str,
        /// Windows installer URL, run via PowerShell `irm {url} | iex`.
        windows: &'static str,
    },
    /// Platform package manager (node: brew on macOS / winget on Windows).
    PlatformPkg,
    /// No auto-install; detection + docs link only.
    DetectOnly,
}

pub struct AgentDef {
    pub id: &'static str,
    pub label: &'static str,
    pub binary: &'static str,
    pub kind: AgentKind,
    pub install: InstallMethod,
    pub check_probe: &'static [&'static str],
    /// Extra absolute binary locations to probe when the binary is NOT on PATH.
    /// Each entry may start with `~` (home-relative). GUI-launched apps inherit
    /// a minimal PATH that routinely misses `~/.local/bin`, nvm prefixes, etc.,
    /// so a pure PATH probe false-negatives installed agents (notably Claude
    /// Code's native installer). See AgentDef::resolve().
    pub fallback_paths: &'static [&'static str],
    pub depends_on: &'static [&'static str],
    pub docs_url: Option<&'static str>,
}

static AGENTS: &[AgentDef] = &[
    AgentDef {
        id: "node", label: "Node.js", binary: "node",
        kind: AgentKind::Runtime, install: InstallMethod::PlatformPkg,
        check_probe: &["--version"], fallback_paths: &[],
        depends_on: &[], docs_url: Some("https://nodejs.org"),
    },
    AgentDef {
        id: "aider", label: "Aider", binary: "aider",
        kind: AgentKind::NativeCli,
        install: InstallMethod::Pipx { package: "aider-chat" },
        check_probe: &["--version"], fallback_paths: &["~/.local/bin/aider"],
        depends_on: &[], docs_url: Some("https://aider.chat"),
    },
    AgentDef {
        id: "claude-code", label: "Claude Code", binary: "claude",
        kind: AgentKind::NativeCli,
        install: InstallMethod::Npm { package: "@anthropic-ai/claude-code", force: false },
        check_probe: &["--version"],
        // Native installer (claude.ai/install.sh) lays the binary down at
        // ~/.local/bin/claude (covered generically in resolve()) and, on some
        // versions, ~/.claude/local/claude. Declared explicitly because that
        // latter path is Claude-Code-specific and not a general bin dir.
        fallback_paths: &["~/.claude/local/claude"],
        depends_on: &[], docs_url: Some("https://docs.anthropic.com/en/docs/claude-code"),
    },
    AgentDef {
        id: "codex", label: "Codex", binary: "codex",
        kind: AgentKind::NativeCli,
        install: InstallMethod::Npm { package: "@openai/codex", force: false },
        check_probe: &["--version"], fallback_paths: &[],
        depends_on: &[], docs_url: Some("https://developers.openai.com/codex/cli"),
    },
    AgentDef {
        id: "openclaw", label: "OpenClaw", binary: "openclaw",
        kind: AgentKind::Gateway,
        install: InstallMethod::Npm { package: "openclaw", force: false },
        check_probe: &["--version"], fallback_paths: &[],
        depends_on: &[], docs_url: Some("https://www.npmjs.com/package/openclaw"),
    },
    AgentDef {
        id: "opencode", label: "OpenCode", binary: "opencode",
        kind: AgentKind::NativeCli,
        install: InstallMethod::Npm { package: "opencode-ai", force: false },
        check_probe: &["--version"], fallback_paths: &[],
        depends_on: &[], docs_url: Some("https://opencode.ai"),
    },
    AgentDef {
        id: "codebuddy", label: "CodeBuddy", binary: "codebuddy",
        kind: AgentKind::NativeCli,
        install: InstallMethod::Npm { package: "@tencent-ai/codebuddy-code", force: false },
        check_probe: &["--version"], fallback_paths: &[],
        depends_on: &[], docs_url: Some("https://www.codebuddy.ai"),
    },
    AgentDef {
        id: "cursor-agent", label: "Cursor", binary: "cursor-agent",
        kind: AgentKind::NativeCli,
        install: InstallMethod::Script { unix: "https://cursor.com/install", windows: "https://cursor.com/install" },
        check_probe: &["--version"], fallback_paths: &[],
        depends_on: &[], docs_url: Some("https://cursor.com/cli"),
    },
    AgentDef {
        id: "kimi", label: "Kimi", binary: "kimi",
        kind: AgentKind::NativeCli,
        install: InstallMethod::Script { unix: "https://code.kimi.com/kimi-code/install.sh", windows: "https://code.kimi.com/kimi-code/install.sh" },
        check_probe: &["--version"], fallback_paths: &[],
        depends_on: &[], docs_url: Some("https://code.kimi.com"),
    },
    AgentDef {
        id: "qodercli", label: "Qoder", binary: "qodercli",
        kind: AgentKind::NativeCli,
        install: InstallMethod::Script { unix: "https://qoder.com/install", windows: "https://qoder.com/install" },
        check_probe: &["--version"], fallback_paths: &[],
        depends_on: &[], docs_url: Some("https://qoder.com"),
    },
    AgentDef {
        id: "hermes", label: "Hermes", binary: "hermes",
        kind: AgentKind::Gateway,
        // Hermes (NousResearch/hermes-agent) 分发跨平台安装脚本:
        //   unix:    curl -fsSL https://hermes-agent.nousresearch.com/install.sh | bash
        //   windows: irm https://hermes-agent.nousresearch.com/install.ps1 | iex
        install: InstallMethod::Script {
            unix: "https://hermes-agent.nousresearch.com/install.sh",
            windows: "https://hermes-agent.nousresearch.com/install.ps1",
        },
        check_probe: &["--version"], fallback_paths: &[],
        depends_on: &[], docs_url: Some("https://github.com/NousResearch/hermes-agent"),
    },
    AgentDef {
        id: "gemini", label: "Gemini CLI", binary: "gemini",
        kind: AgentKind::NativeCli,
        install: InstallMethod::Npm { package: "@google/gemini-cli", force: false },
        check_probe: &["--version"], fallback_paths: &[],
        depends_on: &[], docs_url: Some("https://github.com/google-gemini/gemini-cli"),
    },
    AgentDef {
        id: "cline", label: "Cline", binary: "cline",
        kind: AgentKind::NativeCli,
        install: InstallMethod::Npm { package: "cline", force: false },
        check_probe: &["--version"], fallback_paths: &[],
        docs_url: Some("https://docs.cline.bot/cline-cli/installation"),
        depends_on: &[],
    },
    AgentDef {
        // badlogic/pi:npm 包已迁移为 @earendil-works/pi-coding-agent
        // (旧名 @mariozechner/pi-coding-agent)。设计上不支持 MCP。
        id: "pi", label: "Pi", binary: "pi",
        kind: AgentKind::NativeCli,
        install: InstallMethod::Npm { package: "@earendil-works/pi-coding-agent", force: false },
        check_probe: &["--version"], fallback_paths: &[],
        docs_url: Some("https://github.com/badlogic/pi-mono"),
        depends_on: &[],
    },
    AgentDef {
        id: "qwen-code", label: "Qwen Code", binary: "qwen",
        kind: AgentKind::NativeCli,
        install: InstallMethod::Npm { package: "@qwen-code/qwen-code", force: false },
        check_probe: &["--version"], fallback_paths: &[],
        docs_url: Some("https://qwenlm.github.io/qwen-code-docs/"),
        depends_on: &[],
    },
    AgentDef {
        // bytedance/trae-agent:不发 npm/PyPI,仅源码安装(clone + uv sync),
        // 故 DetectOnly。注意 Trae IDE 无 CLI,这里接的是开源 trae-agent。
        id: "trae-agent", label: "Trae Agent", binary: "trae-cli",
        kind: AgentKind::NativeCli,
        install: InstallMethod::DetectOnly,
        check_probe: &["--version"], fallback_paths: &[],
        docs_url: Some("https://github.com/bytedance/trae-agent"),
        depends_on: &[],
    },
    AgentDef {
        // DeepSeek Harness(dsh):一切皆插件的 agent harness;Web UI 从
        // `npx @deepseek-ai/dsh web` 启动,服务商配置在 ~/.dsh/settings.yaml
        // (llm-pi-ai.providers)+ ~/.dsh/.credentials.yaml(refs)。
        id: "dsh", label: "DeepSeek Harness", binary: "dsh",
        kind: AgentKind::NativeCli,
        install: InstallMethod::Npm { package: "@deepseek-ai/dsh", force: false },
        check_probe: &["--version"], fallback_paths: &[],
        docs_url: Some("https://github.com/deepseek-ai/deepseek-harness"),
        depends_on: &["node"],
    },
];

pub fn agents() -> &'static [AgentDef] {
    AGENTS
}

pub fn find_agent(id: &str) -> Option<&'static AgentDef> {
    AGENTS.iter().find(|a| a.id == id)
}

/// Resolve `~` and `~user` to an absolute path. Returns None if the entry
/// doesn't start with `~` or the home dir is unavailable.
fn expand_home(p: &str) -> Option<PathBuf> {
    let rest = p.strip_prefix('~')?;
    let home = dirs::home_dir()?;
    Some(if rest.is_empty() {
        home
    } else {
        // rest begins with the platform separator (we only emit unix-style
        // "~/.local/..." entries); trim leading '/' defensively.
        home.join(rest.trim_start_matches('/'))
    })
}

/// Whether a path is an executable file (unix: executable bit; windows: any
/// file). Used both for PATH-walk matches and fallback-path existence checks.
#[cfg(unix)]
fn is_executable(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    match std::fs::metadata(p) {
        Ok(m) => m.is_file() && (m.permissions().mode() & 0o111 != 0),
        Err(_) => false,
    }
}
#[cfg(not(unix))]
fn is_executable(p: &Path) -> bool {
    p.is_file()
}

/// Executable file extensions to try for a bare binary name, most-specific
/// first. On Windows a CLI on PATH is almost never extensionless: `npm i -g`
/// writes `claude.cmd` (+ a `.ps1` sibling), native builds ship `claude.exe`.
/// `.ps1` is deliberately absent — CreateProcess can't launch a PowerShell
/// script, whereas `std::process` routes `.cmd`/`.bat` through cmd.exe for us.
#[cfg(windows)]
const EXE_EXTS: &[&str] = &["exe", "cmd", "bat"];
#[cfg(not(windows))]
const EXE_EXTS: &[&str] = &[];

/// Accept `p` itself, else `p` with each executable extension appended.
/// Fallback entries name a file (`~/.claude/local/claude`), and on Windows the
/// file actually on disk is that name plus `.cmd` / `.exe`.
fn resolve_exe_file(p: &Path) -> Option<PathBuf> {
    if is_executable(p) {
        return Some(p.to_path_buf());
    }
    for ext in EXE_EXTS {
        // Append, don't set_extension: `trae-cli` must not become `trae.exe`.
        let mut name = p.file_name()?.to_os_string();
        name.push(format!(".{}", ext));
        let candidate = p.with_file_name(name);
        if is_executable(&candidate) {
            return Some(candidate);
        }
    }
    None
}

/// The first executable named `binary` (with any platform extension) in `dir`.
fn find_exe_in(dir: &Path, binary: &str) -> Option<PathBuf> {
    resolve_exe_file(&dir.join(binary))
}

/// Manual `which`: walk the current process PATH for the first matching
/// executable. Avoids pulling in the `which` crate for ~10 lines.
fn find_in_path(binary: &str) -> Option<PathBuf> {
    let path = std::env::var("PATH").ok()?;
    for dir in path.split(crate::path_env::PATH_SEP) {
        let dir = dir.trim();
        if dir.is_empty() {
            continue;
        }
        if let Some(hit) = find_exe_in(Path::new(dir), binary) {
            return Some(hit);
        }
    }
    None
}

/// npm global install prefix's bin dir, cached after first resolution.
/// `npm config get prefix` can be slow / hit the network, so memoize per
/// process. Only consulted as a last-resort fallback for Npm-installed agents.
static NPM_GLOBAL_BIN: OnceLock<Option<PathBuf>> = OnceLock::new();
fn npm_global_bin() -> Option<PathBuf> {
    NPM_GLOBAL_BIN
        .get_or_init(|| {
            let child = crate::proc::command("npm")
                .args(["config", "get", "prefix"])
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
                .ok()?;
            let out = crate::path_env::wait_with_timeout(child, Duration::from_secs(5))?;
            if !out.status.success() {
                return None;
            }
            let prefix = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if prefix.is_empty() || prefix == "undefined" {
                return None;
            }
            let prefix = PathBuf::from(prefix);
            // Unix lays shims down in <prefix>/bin; Windows puts them directly
            // in <prefix> (%APPDATA%\npm), where a "bin" join finds nothing.
            Some(if cfg!(windows) { prefix } else { prefix.join("bin") })
        })
        .clone()
}

/// nvm-managed global bin dir: newest version under ~/.nvm/versions/node/*/bin.
/// `npm config get prefix` is itself unreachable when the GUI PATH lacks nvm
/// (npm lives inside the very bin dir we're looking for), so scan the
/// filesystem instead. Honors ~/.nvm/alias/default when present, else picks
/// the lexicographically greatest version dir (node versions sort lexically).
fn nvm_global_bin() -> Option<PathBuf> {
    if cfg!(windows) {
        return None; // nvm-windows uses a different layout; out of scope
    }
    let nvm = dirs::home_dir()?.join(".nvm").join("versions").join("node");
    let versions: Vec<PathBuf> = std::fs::read_dir(&nvm)
        .ok()?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    if versions.is_empty() {
        return None;
    }
    // Prefer the version ~/.nvm/alias/default points at ("22", "v22.23.1",
    // "lts/..."): try the two spellings; bare "lts/*" we skip — too fiddly.
    let alias = std::fs::read_to_string(nvm.join("..").join("..").join("alias").join("default"))
        .ok()
        .map(|s| s.trim().trim_start_matches('v').to_string())
        .filter(|a| !a.is_empty() && !a.starts_with("lts"));
    if let Some(a) = alias {
        for v in &versions {
            if let Some(name) = v.file_name() {
                let nv = name.to_string_lossy().trim_start_matches('v').to_string();
                if nv == a || nv.starts_with(&format!("{}.", a)) {
                    return Some(v.join("bin"));
                }
            }
        }
    }
    versions
        .into_iter()
        .max()
        .map(|v| v.join("bin"))
}

impl AgentDef {
    /// Resolve the absolute binary path, trying in order:
    ///   1. the process PATH (the common, fast case)
    ///   2. declared per-agent fallback paths (~/.claude/local/claude, ...)
    ///   3. the generic ~/.local/bin/<binary> (native installers, e.g. Claude
    ///      Code's claude.ai/install.sh)
    ///   4. the npm global prefix's bin dir (Npm-installed agents)
    ///
    /// This exists because GUI-launched ClawBox inherits a minimal PATH; when
    /// path_env::init() can't recover the user's interactive shell PATH
    /// (heavy .zshrc timing out), a bare PATH probe false-negatives agents the
    /// user has clearly installed. See GH#3.
    pub fn resolve(&self) -> Option<PathBuf> {
        if let Some(abs) = find_in_path(self.binary) {
            return Some(abs);
        }
        for fb in self.fallback_paths {
            if let Some(p) = expand_home(fb) {
                if let Some(hit) = resolve_exe_file(&p) {
                    return Some(hit);
                }
            }
        }
        if let Some(home) = dirs::home_dir() {
            if let Some(hit) = find_exe_in(&home.join(".local").join("bin"), self.binary) {
                return Some(hit);
            }
        }
        if matches!(self.install, InstallMethod::Npm { .. }) {
            if let Some(bin_dir) = npm_global_bin() {
                if let Some(hit) = find_exe_in(&bin_dir, self.binary) {
                    return Some(hit);
                }
            }
            // nvm layouts: `npm` itself is unreachable when the GUI PATH lacks
            // the very bin dir we're hunting for — scan ~/.nvm directly.
            if let Some(bin_dir) = nvm_global_bin() {
                if let Some(hit) = find_exe_in(&bin_dir, self.binary) {
                    return Some(hit);
                }
            }
        }
        None
    }

    /// Probe the binary for its version string.
    /// Bounded by a 10s watchdog: a hung `--version` (interactive prompt,
    /// self-update check) must not stall list_agent_status' rayon collect
    /// and freeze the frontend spinner forever.
    pub fn version(&self) -> Option<String> {
        // Resolve up front so detection survives a minimal GUI PATH — we may
        // have to run an absolute fallback path instead of relying on PATH
        // lookup inside Command::new.
        let resolved = self.resolve()?;
        let child = crate::proc::command(&resolved)
            .args(self.check_probe)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;
        let out = crate::path_env::wait_with_timeout(child, Duration::from_secs(10))?;
        if !out.status.success() {
            return None;
        }
        let s = String::from_utf8_lossy(&out.stdout).trim().lines().next()?.to_string();
        if s.is_empty() { None } else { Some(s) }
    }

    pub fn is_installed(&self) -> bool {
        self.version().is_some()
    }
}

/// Explicit depends_on plus the implicit node dependency for Npm installs.
pub fn effective_deps(def: &AgentDef) -> Vec<&'static str> {
    let mut deps: Vec<&'static str> = def.depends_on.to_vec();
    if matches!(def.install, InstallMethod::Npm { .. }) && !deps.contains(&"node") {
        deps.push("node");
    }
    deps
}

/// Human-readable install command (shown in UI before the user confirms).
/// None for DetectOnly.
pub fn install_command_display(def: &AgentDef) -> Option<String> {
    match def.install {
        InstallMethod::Npm { package, force } => Some(if force {
            format!("npm install -g --force {}", package)
        } else {
            format!("npm install -g {}", package)
        }),
        InstallMethod::Pipx { package } => Some(format!("pipx install {}", package)),
        InstallMethod::Script { unix, windows } => Some(if cfg!(windows) {
            format!("irm {} | iex", windows)
        } else {
            format!("curl -fsSL {} | bash", unix)
        }),
        InstallMethod::PlatformPkg => Some(match std::env::consts::OS {
            "windows" => "winget install --id OpenJS.NodeJS.LTS --silent".to_string(),
            _ => "brew install node".to_string(),
        }),
        InstallMethod::DetectOnly => None,
    }
}

#[derive(Serialize)]
pub struct AgentStatus {
    pub id: String,
    pub label: String,
    pub kind: AgentKind,
    pub installed: bool,
    pub version: Option<String>,
    pub deps_satisfied: bool,
    pub missing_deps: Vec<String>,
    pub install_command: Option<String>,
    pub docs_url: Option<String>,
}

/// Probe every agent in parallel (each probe shells out, ~0.1-1s serial cost).
pub fn list_agent_status() -> Vec<AgentStatus> {
    use rayon::prelude::*;
    // Probe installed-state for all 10 first so dep checks reuse results.
    let installed: std::collections::HashMap<&str, Option<String>> = AGENTS
        .par_iter()
        .map(|a| (a.id, a.version()))
        .collect();
    AGENTS
        .iter()
        .map(|a| {
            let version = installed.get(a.id).cloned().flatten();
            let missing: Vec<String> = effective_deps(a)
                .into_iter()
                .filter(|dep| installed.get(*dep).map(|v| v.is_none()).unwrap_or(true))
                .map(|s| s.to_string())
                .collect();
            AgentStatus {
                id: a.id.to_string(),
                label: a.label.to_string(),
                kind: a.kind,
                installed: version.is_some(),
                version,
                deps_satisfied: missing.is_empty(),
                missing_deps: missing,
                install_command: install_command_display(a),
                docs_url: a.docs_url.map(|s| s.to_string()),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_has_exactly_15_unique_entries() {
        let ids: Vec<_> = agents().iter().map(|a| a.id).collect();
        assert_eq!(ids.len(), 17);
        let mut dedup = ids.clone();
        dedup.sort();
        dedup.dedup();
        assert_eq!(dedup.len(), 17, "duplicate agent ids");
    }

    #[test]
    fn registry_covers_spec_table() {
        for id in [
            "node", "claude-code", "codex", "openclaw", "opencode", "codebuddy",
            "cursor-agent", "kimi", "qodercli", "hermes",
            "gemini", "cline", "pi", "qwen-code", "trae-agent",
        ] {
            assert!(find_agent(id).is_some(), "missing agent: {}", id);
        }
    }

    #[test]
    fn depends_on_references_are_valid_ids() {
        for a in agents() {
            for dep in a.depends_on {
                assert!(find_agent(dep).is_some(), "{} depends on unknown {}", a.id, dep);
            }
        }
    }

    #[test]
    fn hermes_uses_cross_platform_script_install() {
        // Hermes 由脚本安装 (unix install.sh / windows install.ps1),不再 DetectOnly。
        let h = find_agent("hermes").unwrap();
        assert!(matches!(h.install, InstallMethod::Script { .. }));
        assert!(install_command_display(h).is_some());
    }

    #[test]
    fn install_command_display_formats_each_method() {
        assert_eq!(
            install_command_display(find_agent("claude-code").unwrap()).as_deref(),
            Some("npm install -g @anthropic-ai/claude-code")
        );
        assert_eq!(
            install_command_display(find_agent("cursor-agent").unwrap()).as_deref(),
            Some("curl -fsSL https://cursor.com/install | bash")
        );
        // node = platform package manager
        let node_cmd = install_command_display(find_agent("node").unwrap()).unwrap();
        assert!(node_cmd.contains("brew") || node_cmd.contains("winget"));
    }

    #[test]
    fn npm_agents_implicitly_require_node_in_status() {
        // Registry-level check: effective_deps injects "node" for Npm installs.
        let deps = effective_deps(find_agent("claude-code").unwrap());
        assert!(deps.contains(&"node"));
        // ...but node itself must not depend on node.
        assert!(effective_deps(find_agent("node").unwrap()).is_empty());
    }

    #[test]
    fn expand_home_resolves_tilde_prefix() {
        let home = dirs::home_dir().expect("home dir available in test env");
        assert_eq!(expand_home("~/foo/bar"), Some(home.join("foo/bar")));
        assert_eq!(expand_home("~"), Some(home));
        // absolute / non-tilde paths are not expanded (return None here).
        assert_eq!(expand_home("/usr/bin"), None);
        assert_eq!(expand_home("claude"), None);
    }

    #[test]
    fn find_in_path_locates_a_known_binary() {
        // `sh` ships on every unix; `where` exists on windows. Either way a
        // binary present on the process PATH must resolve to an absolute path.
        let needle = if cfg!(windows) { "where" } else { "sh" };
        let resolved = find_in_path(needle);
        assert!(resolved.is_some(), "expected {} on PATH", needle);
        let p = resolved.unwrap();
        assert!(p.is_absolute() || p.exists());
    }

    #[test]
    fn find_in_path_returns_none_for_missing_binary() {
        assert!(find_in_path("definitely-not-a-real-binary-xyz").is_none());
    }

    /// A scratch dir holding one file, cleaned up on drop.
    fn scratch_with(file: &str, tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("clawbox-exe-{}-{}", tag, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scratch dir");
        let p = dir.join(file);
        std::fs::write(&p, "").expect("scratch file");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        dir
    }

    #[test]
    #[cfg(windows)]
    fn find_exe_in_matches_npm_cmd_shim() {
        // The Windows detection bug: `npm i -g @anthropic-ai/claude-code`
        // writes claude.cmd, and an extensionless probe never sees it.
        let dir = scratch_with("claude.cmd", "cmd");
        let hit = find_exe_in(&dir, "claude").expect("claude.cmd must resolve from bare name");
        assert_eq!(hit.file_name().unwrap(), "claude.cmd");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    #[cfg(windows)]
    fn resolve_exe_file_appends_rather_than_replaces_extension() {
        // `trae-cli` has a dot-free name but a hyphen; set_extension() on a
        // name like `foo.bar` would clobber the suffix, so we append.
        let dir = scratch_with("trae-cli.exe", "append");
        let hit = resolve_exe_file(&dir.join("trae-cli")).expect("trae-cli.exe must resolve");
        assert_eq!(hit.file_name().unwrap(), "trae-cli.exe");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn find_exe_in_finds_a_plain_executable() {
        let dir = scratch_with(if cfg!(windows) { "probe.exe" } else { "probe" }, "plain");
        assert!(find_exe_in(&dir, "probe").is_some());
        assert!(find_exe_in(&dir, "absent").is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn claude_code_declares_native_installer_fallback() {
        // GH#3: Claude Code installed via claude.ai/install.sh lands at
        // ~/.local/bin/claude (generic, in resolve()) and ~/.claude/local/claude
        // (Claude-specific, declared here). The declared entry must exist and
        // be home-relative so detection survives a broken/minimal PATH.
        let cc = find_agent("claude-code").unwrap();
        assert!(
            cc.fallback_paths.iter().any(|p| *p == "~/.claude/local/claude"),
            "claude-code must declare ~/.claude/local/claude fallback"
        );
    }

    #[test]
    fn every_agent_declares_fallback_paths_field() {
        // Guard against a future AgentDef literal forgetting the field — Rust
        // already enforces this at compile time, but this makes intent explicit.
        for a in agents() {
            // field exists and is a slice; nothing to assert about contents,
            // just that construction didn't omit it (compile-time check).
            let _ = a.fallback_paths;
        }
    }

    #[test]
    fn nvm_global_bin_finds_default_alias_dir() {
        // 本机 ~/.nvm 布局(nvm for unix);无 nvm 的环境跳过
        let Some(bin) = nvm_global_bin() else { return };
        assert!(bin.to_string_lossy().contains(".nvm"), "{:?}", bin);
        assert!(bin.join("node").exists(), "node missing in {:?}", bin);
    }

    #[test]
    fn dsh_resolves_via_nvm_even_with_minimal_path() {
        // GUI 最小 PATH 复现:剥掉 nvm 段后 dsh 仍应经 nvm 兜底解析到
        let saved = std::env::var("PATH").unwrap();
        let stripped = saved
            .split(':')
            .filter(|d| !d.contains(".nvm"))
            .collect::<Vec<_>>()
            .join(":");
        std::env::set_var("PATH", &stripped);
        let hit = find_agent("dsh").unwrap().resolve();
        std::env::set_var("PATH", &saved);
        if hit.is_some() {
            assert!(hit.unwrap().to_string_lossy().contains(".nvm"));
        } // 无 nvm/未装 dsh 的环境:resolve 为 None 也合法
    }
}
