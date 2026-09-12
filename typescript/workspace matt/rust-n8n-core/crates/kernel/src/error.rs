//! Error types shared across the engine.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Errors produced by kernel-level operations (item access, spill I/O,
/// expression evaluation, credential lookup).
#[derive(Debug, thiserror::Error)]
pub enum KernelError {
    #[error("index {index} out of bounds (len {len})")]
    IndexOutOfBounds { index: usize, len: usize },

    #[error("spill store i/o failed: {message}")]
    SpillIo { message: String },

    #[error("codec `{codec}` failed: {reason}")]
    Codec {
        codec: &'static str,
        reason: String,
    },

    #[error("node `{node}` not found in prior outputs")]
    NodeNotFound { node: String },

    #[error("expression `{expr}` failed: {reason}")]
    Expression { expr: String, reason: String },

    #[error("invalid data: {message}")]
    Invalid { message: String },
}

/// Why a node failed.
///
/// The distinction between variants is not cosmetic — it drives retry policy
/// (D14/D57), quarantine (D69), and crash reconciliation (audit A-10).
#[derive(Debug, thiserror::Error)]
pub enum NodeError {
    /// Retryable: network blip, HTTP 429/5xx, transient lock.
    #[error("transient failure (retry_after={retry_after:?}): {message}")]
    Transient {
        message: String,
        retry_after: Option<std::time::Duration>,
    },

    /// Not retryable: HTTP 4xx, validation failure, auth failure.
    #[error("permanent failure [{code}]: {message}")]
    Permanent { message: String, code: ErrorCode },

    /// D58: the per-node deadline elapsed.
    #[error("timeout after {elapsed:?}")]
    Timeout { elapsed: std::time::Duration },

    /// D20: cooperative cancellation was observed.
    #[error("cancelled")]
    Cancelled,

    /// D72: the Memory/CPU Governor refused the allocation.
    ///
    /// IMPORTANT (audit A-07): this is a *normal, expected* error, not a bug.
    /// Nodes must return it rather than attempt the allocation and OOM.
    /// It is the only mechanism by which a "hard RAM budget" can actually be
    /// enforced from inside the process.
    #[error("resource exhausted: {resource} (requested {requested} bytes, {available} available)")]
    ResourceExhausted {
        resource: Resource,
        requested: u64,
        available: u64,
    },

    /// Expression evaluation failed.
    #[error("expression `{expr}` failed: {reason}")]
    Expression { expr: String, reason: String },

    /// A bug in the node implementation. Every occurrence must become a test.
    #[error("internal node error: {message}")]
    Internal { message: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Resource {
    Memory,
    Cpu,
    Disk,
    FileHandles,
    Network,
}

impl fmt::Display for Resource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Resource::Memory => "memory",
            Resource::Cpu => "cpu",
            Resource::Disk => "disk",
            Resource::FileHandles => "file handles",
            Resource::Network => "network",
        };
        f.write_str(s)
    }
}

/// Stable, machine-readable error classification.
///
/// Kept as a string (not an int) so it survives serialization into the event
/// log and is greppable in structured logs (D63).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ErrorCode(pub String);

impl ErrorCode {
    pub const fn new() -> Self {
        Self(String::new())
    }
    pub fn of(s: impl Into<String>) -> Self {
        Self(s.into())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for ErrorCode {
    fn default() -> Self {
        Self::of("unknown")
    }
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// True when a [`NodeError`] should be retried under D14/D57.
///
/// NOTE (audit A-10): this deliberately does NOT consult `SideEffect`.
/// A `Transient` error from a `SideEffect::NonIdempotent` node must be routed
/// to quarantine by the *scheduler*, not retried here. The scheduler owns that
/// decision because it is the only component that sees both facts.
impl NodeError {
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            NodeError::Transient { .. } | NodeError::ResourceExhausted { .. }
        )
    }

    pub fn is_terminal(&self) -> bool {
        matches!(self, NodeError::Cancelled)
    }
}
