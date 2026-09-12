//! Strongly-typed identifiers.
//!
//! Newtypes instead of bare `u64` so the compiler catches
//! `execution_id` being passed where `workflow_id` is expected.

use serde::{Deserialize, Serialize};
use std::fmt;

macro_rules! numeric_id {
    ($name:ident) => {
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(pub u64);

        impl $name {
            pub const fn new(v: u64) -> Self {
                Self(v)
            }
            pub const fn get(self) -> u64 {
                self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}#{}", stringify!($name), self.0)
            }
        }
    };
}

numeric_id!(WorkflowId);
numeric_id!(ExecutionId);
numeric_id!(TaskId);
numeric_id!(CheckpointId);
numeric_id!(WorkerId);

// Content-addressed handle into the blob store (D30).
numeric_id!(ContentId);

/// Identifier of a node *instance* inside a workflow.
///
/// n8n addresses nodes by their user-visible **name** (`$('HTTP Request')`),
/// so this is a string, not a number. Both name and a stable id are kept:
/// names can be edited by the user, ids cannot.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct NodeId(pub String);

impl NodeId {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Identifier of a node *type*, e.g. `http.request`, `if`, `merge`.
///
/// Dotted namespace so the OpenAPI codegen (KERNEL-SPEC §10) can emit
/// `generated.stripe.charges.create` without colliding with hand-written nodes.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct NodeKind(pub String);

impl NodeKind {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
    /// True for nodes produced by the OpenAPI generator rather than hand-written.
    pub fn is_generated(&self) -> bool {
        self.0.starts_with("generated.")
    }
}

impl fmt::Display for NodeKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Opaque location of a spill file inside a [`SpillStore`].
///
/// Deliberately a string rather than a `PathBuf`: the store may be backed by
/// a directory, an object store, or a single packed file with offsets.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SpillPath(pub String);

impl SpillPath {
    /// Create a path from anything string-convertible.
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }
    /// Borrow the underlying string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SpillPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
