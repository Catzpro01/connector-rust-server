//! What a node sees while executing.

use crate::error::KernelError;
use crate::id::{ExecutionId, NodeId, WorkflowId};
use crate::item::{Item, ItemList, ItemView};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

/// Everything a node may touch during one execution attempt.
///
/// Deliberately a struct of trait objects rather than a god-object: the engine
/// assembles it, the node borrows it, and no node can reach outside these
/// handles. That is how credential least-privilege (D92) and the RAM budget
/// (D72) are actually enforced instead of merely documented.
pub struct NodeContext<'a> {
    // ── identity & environment ────────────────────────────────────────────
    pub execution: &'a ExecutionMeta,
    pub node: &'a NodeMeta,

    /// Input lists, one per input index. `Merge` sees 2.
    pub input: &'a [ItemList],

    /// RESOLUTION of audit A-01.
    ///
    /// Outputs of every node that already completed in this execution, kept
    /// addressable because n8n expressions require it:
    /// `$('HTTP Request').item.json`, `$items('Node')`, `$node["X"].json`.
    ///
    /// Memory-safe because the values are `ItemList`s, which may be spilled:
    /// addressable does not mean resident.
    pub prior: &'a dyn PriorOutputs,

    // ── services ──────────────────────────────────────────────────────────
    /// Only credentials declared in `NodeDescriptor::credentials` resolve.
    pub credentials: &'a dyn CredentialProvider,

    /// D101 — per-(workflow, node) state persisting across executions.
    /// Flushed only on `ExecutionMode::Production` (D106), matching n8n.
    pub static_data: &'a mut StaticData,

    /// D109 — the kernel defines the contract; QuickJS lives in `expr-quickjs`.
    pub expr: &'a dyn ExpressionEngine,

    pub http: &'a dyn HttpClient,
    pub blobs: &'a dyn BlobStore,
    pub spill: &'a dyn crate::item::SpillStore,
    pub log: &'a dyn Logger,

    // ── control ───────────────────────────────────────────────────────────
    /// D20 — cooperative cancellation. Nodes MUST poll this.
    pub cancel: &'a dyn CancellationToken,

    /// D58 — per-node deadline. `Instant`, not a duration: absolute and
    /// unaffected by clock changes.
    pub deadline: Instant,

    /// Attempt number, 1-based. Lets a node vary behaviour on retry.
    pub attempt: u16,
}

/// Manual: `&mut StaticData` and the `dyn` service handles are not `Debug`,
/// and a derived impl would be impossible to satisfy.
///
/// This prints identity, shape and control state — which is what you actually
/// need when a node fails in production — and never item payloads or
/// credentials. `finish_non_exhaustive` keeps the output honest about the
/// service handles it is omitting.
impl std::fmt::Debug for NodeContext<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NodeContext")
            .field("execution", &self.execution.id)
            .field("node", &self.node.id)
            .field("mode", &self.execution.mode)
            .field("attempt", &self.attempt)
            .field("inputs", &self.input.len())
            .field(
                "input_lens",
                &self.input.iter().map(|l| l.len()).collect::<Vec<_>>(),
            )
            .field("cancelled", &self.cancel.is_cancelled())
            .field("static_data_dirty", &self.static_data.is_dirty())
            .finish_non_exhaustive()
    }
}

impl<'a> NodeContext<'a> {
    /// View of one input, with the spill store already bound.
    pub fn input(&self, index: usize) -> Result<ItemView<'_>, KernelError> {
        self.input
            .get(index)
            .map(|l| l.view(self.spill))
            .ok_or(KernelError::IndexOutOfBounds {
                index,
                len: self.input.len(),
            })
    }

    /// Milliseconds remaining before the deadline (0 if already passed).
    pub fn remaining_ms(&self) -> u64 {
        self.deadline
            .checked_duration_since(Instant::now())
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancel.is_cancelled()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Execution / node metadata
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExecutionMeta {
    pub id: ExecutionId,
    pub workflow_id: WorkflowId,
    /// Immutable workflow snapshot version (D48).
    pub workflow_version: u32,

    /// D106 — decides whether static data flushes and whether data is retained.
    pub mode: ExecutionMode,

    /// D102 — cron, `$now` and `$today` all depend on this.
    ///
    /// Stored as an IANA name (`"Asia/Jakarta"`) rather than a fixed offset, so
    /// DST is handled correctly. Parsing lives outside the kernel.
    pub timezone: String,

    /// D104 — n8n changed branch ordering in 1.0. Both must be honoured or
    /// imported workflows silently produce different results.
    pub execution_order: ExecutionOrder,

    /// Unix milliseconds.
    pub started_at: i64,
}

/// n8n distinguishes these and the distinction changes behaviour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ExecutionMode {
    /// Live run. Static data flushes; retry and error workflows apply.
    Production,
    /// Run from the editor. Static data does NOT flush (n8n behaviour).
    Manual,
    /// Editor test with `pinData`; no real side effects.
    Test,
    /// Triggered by an incoming webhook.
    Webhook,
}

impl ExecutionMode {
    /// D106/D101: only production runs persist static data.
    pub fn flushes_static_data(self) -> bool {
        matches!(self, ExecutionMode::Production)
    }

    pub fn is_production(self) -> bool {
        matches!(self, ExecutionMode::Production)
    }
}

/// D104 — n8n 1.0 changed multi-branch ordering.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExecutionOrder {
    /// Legacy: first node of every branch, then second node of every branch.
    ///
    /// Removed in n8n 1.0. Still honoured on import so old workflows keep
    /// producing identical results (D99 — zero behaviour change on upgrade).
    #[serde(rename = "v0")]
    V0Legacy,
    /// Modern (n8n >= 1.0 default): complete one branch before starting the
    /// next, ordered by canvas position (top-to-bottom, then left-to-right).
    ///
    /// The default because it is what any workflow created on a current n8n
    /// already expects.
    #[default]
    #[serde(rename = "v1")]
    V1,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NodeMeta {
    pub id: NodeId,
    /// Position on the canvas. Required by `ExecutionOrder::V1`.
    pub position: (i32, i32),
    /// Which input index this attempt is processing, for multi-input nodes.
    pub input_index: u8,
}

// ─────────────────────────────────────────────────────────────────────────────
// PriorOutputs — the A-01 resolution
// ─────────────────────────────────────────────────────────────────────────────

/// Read access to earlier nodes' outputs within the current execution.
///
/// Backing implementations hold `ItemList`s, which may be spilled — so this
/// stays cheap even for executions that produced gigabytes of intermediate data.
#[async_trait]
pub trait PriorOutputs: Send + Sync {
    /// n8n addresses nodes by display name.
    fn by_name(&self, name: &str) -> Option<PriorNodeRef>;

    fn by_id(&self, id: &NodeId) -> Option<PriorNodeRef>;

    /// One item — backs `$('Node').item` and `$json`.
    async fn item(&self, node: &str, input_index: u8, i: usize) -> Result<Item, KernelError>;

    /// Whole list handle — backs `$items()` and `$node["X"].json`.
    fn items(&self, node: &str, input_index: u8) -> Result<ItemList, KernelError>;

    /// Nodes completed so far. Used to validate expressions before running.
    fn available_nodes(&self) -> Vec<String>;
}

/// Lightweight pointer to a prior node's output.
#[derive(Debug, Clone)]
pub struct PriorNodeRef {
    pub id: NodeId,
    pub name: String,
    /// Per output branch.
    pub branch_lengths: Vec<usize>,
}

// ─────────────────────────────────────────────────────────────────────────────
// StaticData — D101
// ─────────────────────────────────────────────────────────────────────────────

/// n8n's `getWorkflowStaticData()`.
///
/// Lets a polling trigger remember "last seen id" between executions. Without
/// it almost no incremental poll trigger can be written, which is why audit
/// listed it as a blocking gap.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct StaticData {
    /// `"global"` scope plus per-node scopes.
    scopes: BTreeMap<String, Value>,
    dirty: bool,
}

impl StaticData {
    pub fn new() -> Self {
        Self::default()
    }

    /// Get a scope, creating an empty object if absent.
    pub fn scope(&mut self, name: &str) -> &mut Value {
        self.scopes
            .entry(name.to_string())
            .or_insert_with(|| Value::Object(Default::default()))
    }

    pub fn get(&self, scope: &str, key: &str) -> Option<&Value> {
        self.scopes.get(scope)?.get(key)
    }

    /// Mark a value. Sets `dirty` so the engine knows to flush.
    pub fn set(&mut self, scope: &str, key: impl Into<String>, value: Value) {
        self.scope(scope)
            .as_object_mut()
            .expect("scope is always an object")
            .insert(key.into(), value);
        self.dirty = true;
    }

    pub fn remove(&mut self, scope: &str, key: &str) -> Option<Value> {
        let out = self
            .scopes
            .get_mut(scope)
            .and_then(|v| v.as_object_mut())
            .and_then(|m| m.remove(key));
        if out.is_some() {
            self.dirty = true;
        }
        out
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Engine calls this after a successful flush.
    pub fn mark_flushed(&mut self) {
        self.dirty = false;
    }

    /// Whether a flush should happen for this execution mode (D106).
    pub fn should_flush(&self, mode: ExecutionMode) -> bool {
        self.dirty && mode.flushes_static_data()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// ExpressionEngine — D109 / A-01
// ─────────────────────────────────────────────────────────────────────────────

/// n8n expressions **are JavaScript**. The kernel defines the contract only;
/// the implementation (QuickJS via `rquickjs`) lives in `crates/expr-quickjs`.
///
/// This split is what keeps `kernel` free of any JS dependency, and is the
/// concrete resolution of audit A-01.
#[async_trait]
pub trait ExpressionEngine: Send + Sync {
    /// Evaluate one expression, returning JSON.
    async fn eval(&self, expr: &str, scope: &ExpressionScope<'_>) -> Result<Value, KernelError>;

    /// Evaluate many expressions reusing one warm JS context.
    ///
    /// Matters for performance: QuickJS context setup costs hundreds of
    /// microseconds, which must not be paid per expression per item.
    async fn eval_batch(
        &self,
        exprs: &[&str],
        scope: &ExpressionScope<'_>,
    ) -> Result<Vec<Value>, KernelError>;

    /// Validate without executing (used by the editor as the user types).
    async fn validate(&self, expr: &str) -> Result<(), KernelError>;
}

/// The surface expressions can see. Anything missing here fails D32
/// compatibility tests, so this list is the contract with n8n.
pub struct ExpressionScope<'a> {
    /// `$json` — current item payload.
    pub json: &'a Value,
    /// `$input` — current input list.
    pub input: &'a ItemList,
    /// `$binary`
    pub binary: Option<&'a Value>,
    /// `$('Node')`, `$items()`, `$node["X"]`
    pub prior: &'a dyn PriorOutputs,
    /// `$execution`
    pub execution: &'a ExecutionMeta,
    /// `$workflow`
    pub workflow: &'a WorkflowMeta,
    /// `$now`, `$today` — already resolved to the workflow timezone (D102).
    pub now_unix_ms: i64,
    /// `$vars`
    pub vars: &'a Value,
    /// `$env` — MUST be whitelisted; never expose the whole environment.
    pub env: &'a dyn EnvAccess,
    /// Index of the item currently being evaluated.
    pub item_index: usize,
}

/// Manual: `dyn PriorOutputs` and `dyn EnvAccess` are not `Debug`.
///
/// Prints `$json` because that is usually what you are actually debugging, but
/// deliberately omits `env` — its values are secrets by construction (D94) and
/// an expression error that dumps the scope must not dump credentials into logs.
impl std::fmt::Debug for ExpressionScope<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExpressionScope")
            .field("json", self.json)
            .field("input_len", &self.input.len())
            .field("item_index", &self.item_index)
            .field("now_unix_ms", &self.now_unix_ms)
            .field("execution", &self.execution.id)
            .field("workflow", &self.workflow.id)
            .field("env", &"<redacted>")
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkflowMeta {
    pub id: WorkflowId,
    pub name: String,
    pub version: u32,
    pub active: bool,
}

/// Whitelisted environment access for `$env`.
pub trait EnvAccess: Send + Sync {
    fn get(&self, key: &str) -> Option<String>;
}

// ─────────────────────────────────────────────────────────────────────────────
// Service contracts
// ─────────────────────────────────────────────────────────────────────────────

/// D92 — least-privilege credential access.
#[async_trait]
pub trait CredentialProvider: Send + Sync {
    /// Returns `Err` if the node did not declare this credential kind.
    async fn get(&self, kind: &str) -> Result<CredentialValue, KernelError>;
}

/// A resolved credential. Values are wrapped so that `Debug`/`Display` and the
/// logger cannot leak them (D93 automatic redaction).
#[derive(Clone)]
pub struct CredentialValue {
    inner: Arc<Value>,
}

impl CredentialValue {
    pub fn new(v: Value) -> Self {
        Self { inner: Arc::new(v) }
    }
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.inner.get(key)
    }
    pub fn as_value(&self) -> &Value {
        &self.inner
    }
}

/// Deliberate: printing a credential must never reveal it.
impl std::fmt::Debug for CredentialValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("CredentialValue(<redacted>)")
    }
}

impl std::fmt::Display for CredentialValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("<redacted>")
    }
}

/// D28 — pooled, streaming, size-gated HTTP.
///
/// The size gating is what makes audit A-07 survivable: the client refuses or
/// spills oversized bodies *before* they reach RAM, rather than letting
/// `serde_json::from_slice` buffer 1.5 GB and get OOM-killed.
#[async_trait]
pub trait HttpClient: Send + Sync {
    async fn send(&self, req: HttpRequest) -> Result<HttpResponse, KernelError>;
}

#[derive(Debug, Clone)]
pub struct HttpRequest {
    pub method: String,
    pub url: String,
    pub headers: BTreeMap<String, String>,
    pub query: BTreeMap<String, String>,
    pub body: Option<RequestBody>,
    /// Hard ceiling in bytes. Exceeding it is an error, not an OOM.
    pub max_response_bytes: u64,
    pub timeout_ms: u64,
}

#[derive(Debug, Clone)]
pub enum RequestBody {
    Json(Value),
    Bytes(Vec<u8>),
    /// Streamed from the blob store — never fully in RAM.
    Blob { content_id: crate::id::ContentId },
    Form(BTreeMap<String, String>),
}

#[derive(Debug, Clone)]
pub struct HttpResponse {
    pub status: u16,
    pub headers: BTreeMap<String, String>,
    /// Small bodies inline; large bodies already written to the blob store.
    pub body: ResponseBody,
    pub duration_ms: u64,
}

#[derive(Debug, Clone)]
pub enum ResponseBody {
    Empty,
    Json(Value),
    Bytes(Vec<u8>),
    Blob {
        content_id: crate::id::ContentId,
        size_bytes: u64,
    },
    /// Body exceeded `max_response_bytes` and was rejected.
    TooLarge { size_bytes: u64 },
}

/// D30 — streaming blob store for binary data, spill files and artifacts.
#[async_trait]
pub trait BlobStore: Send + Sync {
    async fn put(&self, bytes: &[u8], mime: &str) -> Result<(crate::id::ContentId, u64), KernelError>;
    async fn get(&self, id: crate::id::ContentId) -> Result<Vec<u8>, KernelError>;
    async fn delete(&self, id: crate::id::ContentId) -> Result<(), KernelError>;
    fn size(&self, id: crate::id::ContentId) -> Option<u64>;
}

/// D20 — cooperative cancellation.
///
/// Cooperative cancellation cannot stop a CPU-bound loop that never polls it
/// (audit A-11), so the Node SDK must also enforce a watchdog at the scheduler
/// layer. This token is the *polite* half of that mechanism.
pub trait CancellationToken: Send + Sync {
    fn is_cancelled(&self) -> bool;
    /// Optional reason, surfaced in the event log.
    fn reason(&self) -> Option<&str> {
        None
    }
}

/// Never-cancelled token, for tests and manual runs.
#[derive(Debug, Clone, Copy)]
pub struct NoCancel;
impl CancellationToken for NoCancel {
    fn is_cancelled(&self) -> bool {
        false
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Logging
// ─────────────────────────────────────────────────────────────────────────────

/// D63 structured logging. Redaction (D93) is the implementation's job and must
/// be unconditional — a node cannot opt out of it.
pub trait Logger: Send + Sync {
    fn log(&self, level: LogLevel, message: &str, fields: &Value);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

/// A logger that discards everything. Useful in tests and for `NoopNode`s.
#[derive(Debug, Clone, Copy)]
pub struct NoopLogger;
impl Logger for NoopLogger {
    fn log(&self, _level: LogLevel, _message: &str, _fields: &Value) {}
}
