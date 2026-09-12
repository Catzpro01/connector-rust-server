//! The `Node` contract and everything the scheduler needs to know about one.

use crate::error::NodeError;
use crate::id::NodeKind;
use crate::item::ItemList;
use crate::params::{CredentialSpec, ParameterSchema};
use crate::context::NodeContext;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// A node implementation.
///
/// Core never inspects a node's internals — it reads [`NodeDescriptor`] for
/// scheduling decisions and calls [`Node::execute`] for work. That is the whole
/// plugin boundary.
#[async_trait]
pub trait Node: Send + Sync {
    /// Static metadata. Called once at registration, never per-execution.
    fn descriptor(&self) -> &NodeDescriptor;

    /// Run the node.
    ///
    /// All input access goes through `ctx`, so the engine controls what is
    /// materialized into RAM and when. A node must **not** capture raw payload
    /// references beyond this call.
    async fn execute(&self, ctx: &mut NodeContext<'_>) -> Result<NodeOutput, NodeError>;
}

/// Everything the scheduler, the validator, and the UI need about a node type.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NodeDescriptor {
    /// Stable type id: `http.request`, `if`, `merge`, `generated.stripe.charges`.
    pub kind: NodeKind,

    /// D89 — explicit node versions. Parameters evolve; workflows pin a version.
    pub version: u16,

    pub display_name: String,

    #[serde(default)]
    pub group: NodeGroup,

    /// What the scheduler needs to place this node's work.
    pub hints: ResourceHint,

    /// ONE source for both validation and UI form rendering (audit A-30).
    pub params: ParameterSchema,

    /// Credentials this node is allowed to read (D92 least privilege).
    #[serde(default)]
    pub credentials: Vec<CredentialSpec>,

    /// n8n input/output arity. `IF` has 2 outputs, `Merge` has 2 inputs, etc.
    #[serde(default = "default_one")]
    pub inputs: u8,
    #[serde(default = "default_one")]
    pub outputs: u8,

    /// n8n "Execute Once" vs per-item execution.
    #[serde(default)]
    pub execute_once: bool,

    /// Human-facing description shown in the editor.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Icon reference for the editor.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
}

fn default_one() -> u8 {
    1
}

impl NodeDescriptor {
    pub fn new(kind: NodeKind, version: u16, display_name: impl Into<String>) -> Self {
        Self {
            kind,
            version,
            display_name: display_name.into(),
            group: NodeGroup::Action,
            hints: ResourceHint::default(),
            params: ParameterSchema::empty(),
            credentials: Vec::new(),
            inputs: 1,
            outputs: 1,
            execute_once: false,
            description: None,
            icon: None,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NodeGroup {
    /// Starts an execution (Manual, Schedule, Webhook).
    Trigger,
    /// Controls flow (IF, Switch, Merge, Loop, Wait).
    Flow,
    /// Transforms data (Set, Edit Fields, Code, SplitOut).
    Transform,
    /// Talks to the outside world (HTTP, Database, Slack).
    ///
    /// The default: most nodes in any real workflow are actions, and a node
    /// that forgets to declare its group should be treated as the most
    /// conservative one (actions get rate-limited and credential-gated).
    #[default]
    Action,
    /// Generated from an OpenAPI spec.
    Generated,
}

/// Hints the scheduler uses to decide concurrency and admission.
///
/// These are *hints*, not guarantees. The Resource Governor may still refuse a
/// node that under-declares itself — see `NodeError::ResourceExhausted`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceHint {
    pub cpu: Weight,
    pub io: Weight,
    pub memory: Weight,

    /// RESOLUTION of audit A-03.
    ///
    /// The default is `Batch`, because that is n8n's actual semantic. Streaming
    /// is an opt-in optimisation for sources, sinks and 1:1 transforms.
    pub buffering: BufferingMode,

    /// RESOLUTION of audit A-10 / decision D107.
    ///
    /// Without this the crash reconciler cannot possibly be correct: it cannot
    /// tell "the side effect did not happen" from "it happened and we lost the
    /// acknowledgement".
    pub side_effect: SideEffect,

    /// May identical inputs produce a cache hit? (e.g. idempotent GET)
    #[serde(default)]
    pub cacheable: bool,

    /// May this node be re-run safely during recovery at all?
    /// Convenience derived from `side_effect`, kept explicit for clarity.
    #[serde(default = "default_true")]
    pub resumable: bool,
}

fn default_true() -> bool {
    true
}

impl Default for ResourceHint {
    fn default() -> Self {
        Self {
            cpu: Weight::Low,
            io: Weight::Medium,
            memory: Weight::Low,
            // Safe defaults: n8n semantics, no assumed side effects.
            buffering: BufferingMode::Batch,
            side_effect: SideEffect::None,
            cacheable: false,
            resumable: true,
        }
    }
}

impl ResourceHint {
    /// True when the node may be fed items incrementally.
    pub fn can_stream(&self) -> bool {
        self.buffering == BufferingMode::Stream
    }

    /// True when automatic retry after a crash is safe.
    pub fn auto_retry_safe(&self) -> bool {
        matches!(self.side_effect, SideEffect::None | SideEffect::Idempotent)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Weight {
    None,
    Low,
    Medium,
    High,
}

impl Weight {
    /// Rough relative cost, used by the adaptive scheduler (D7/D37).
    pub fn cost(self) -> u8 {
        match self {
            Weight::None => 0,
            Weight::Low => 1,
            Weight::Medium => 2,
            Weight::High => 4,
        }
    }
}

/// How much of its input a node needs before it can emit output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BufferingMode {
    /// Needs the whole input list. **This is the n8n default.**
    ///
    /// Sort, Merge, Aggregate, Limit, RemoveDuplicates, Summarize,
    /// Compare Datasets, SplitInBatches.
    Batch,

    /// Can emit output per input item. Engine may feed incrementally.
    ///
    /// Set, Edit Fields, HTTP download-to-file, most 1:1 transforms.
    Stream,

    /// Needs the whole input AND emits several independent outputs at once.
    ///
    /// SplitOut, Compare Datasets, Switch with many branches.
    BatchInOut,
}

/// Whether running this node mutates the world outside the engine.
///
/// RESOLUTION of audit A-10 / decision D107.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SideEffect {
    /// Pure computation. Always safe to retry.
    ///
    /// The default. Note this is the *optimistic* choice and is only safe
    /// because getting it wrong is loud: a node that silently performs a
    /// non-idempotent side effect while declaring `None` will duplicate that
    /// effect on retry, which surfaces immediately in testing. Declaring
    /// `NonIdempotent` when unsure costs only a retry decision.
    #[default]
    None,

    /// Safe to retry **provided** an idempotency key is supplied (D60).
    ///
    /// e.g. HTTP PUT, Stripe calls with `Idempotency-Key`.
    Idempotent,

    /// MUST NOT be auto-retried.
    ///
    /// If the process crashes while such a node is running, the task goes to
    /// `TaskStatus::InDoubt` and the execution to `NEEDS_REVIEW`. A human
    /// decides. e.g. `POST /transfer`, sending email or SMS.
    NonIdempotent,
}

/// What a node returns.
#[derive(Debug, Clone)]
pub struct NodeOutput {
    /// One entry per output branch. `IF` returns 2, `Merge` returns 1.
    ///
    /// An empty branch is `ItemList::empty()`, not a missing entry — position
    /// matters and must be stable.
    pub branches: Vec<ItemList>,
}

impl NodeOutput {
    pub fn single(list: ItemList) -> Self {
        Self {
            branches: vec![list],
        }
    }

    pub fn none() -> Self {
        Self {
            branches: vec![ItemList::empty()],
        }
    }

    /// For 2-output nodes like IF/Switch.
    pub fn two(true_branch: ItemList, false_branch: ItemList) -> Self {
        Self {
            branches: vec![true_branch, false_branch],
        }
    }

    pub fn branch_count(&self) -> usize {
        self.branches.len()
    }
}
