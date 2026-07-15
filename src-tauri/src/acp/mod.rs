//! ACP (Agent Client Protocol) subsystem — spawns ACP-compatible agent
//! bridges over stdio and drives sessions. Independent from the `Backend`
//! trait (which manages long-running gateway/cron runtimes).

pub mod adapters;
pub mod jsonrpc;
pub mod permission;
pub mod review;
pub mod session;

/// Adapter identifier, e.g. "claude-agent-acp".
pub type AdapterId = String;
