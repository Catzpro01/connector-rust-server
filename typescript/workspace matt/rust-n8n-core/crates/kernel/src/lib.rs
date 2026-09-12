//! # kernel
//!
//! Shared contracts for the workflow engine: pure types and traits, no policy.
//!
//! ## The freeze rule (D115 / audit A-05)
//!
//! Every other crate depends on `kernel`. `kernel` depends on **no internal
//! crate**. That single constraint is what makes the crate graph a DAG instead
//! of the cycle the original blueprint described (`node-sdk` needing `Item`
//! from `data-plane`, `data-plane` needing `Node` from `node-sdk`).
//!
//! Allowed external dependencies: `serde`, `serde_json`, `async-trait`,
//! `thiserror`. Adding anything else — a runtime, an HTTP client, a database
//! driver, a JS engine — requires a written ADR, because every one of those
//! would drag policy into the contract layer.
//!
//! ## What lives here vs elsewhere
//!
//! | Here (contract)              | Elsewhere (policy / implementation)   |
//! |------------------------------|---------------------------------------|
//! | `Item`, `ItemList`           | spill file format, codecs → `data-plane` |
//! | `SpillStore` trait           | disk-backed store, mmap → `data-plane` |
//! | `ExpressionEngine` trait     | QuickJS runtime → `expr-quickjs`      |
//! | `Node`, `NodeDescriptor`     | node implementations → `nodes/`       |
//! | `Checkpoint`, policy struct  | SQLite schema → `storage`             |
//! | `TaskStatus`, recovery enum  | scheduler decisions → `scheduler`     |
//!
//! ## Resolved audit findings
//!
//! * **A-01** — n8n expressions are JavaScript and need random access to prior
//!   node output. Resolved by `PriorOutputs` + `ExpressionEngine` (trait only;
//!   QuickJS lives outside the kernel).
//! * **A-03** — streaming-first inverted n8n's array semantics. Resolved by
//!   `ItemList`: observable semantics stay array-based, `BufferingMode::Batch`
//!   is the default, and efficiency comes from spilling rather than streaming.
//! * **A-05** — undefined core types and a cyclic crate graph. This crate *is*
//!   the resolution.
//! * **A-10** — crash reconciliation could not distinguish "side effect
//!   happened" from "it didn't". Resolved by `SideEffect` + `TaskStatus::InDoubt`
//!   + `Task::recovery_action()`.
//! * **A-16** — persisted events had no version. Resolved by `SCHEMA_VERSION`.
//! * **A-30** — node parameter UI had no schema. Resolved by `ParameterSchema`,
//!   which drives validation and form rendering from one definition.
//! * **D101/D102/D104/D105/D106/D107** — `StaticData`, `ExecutionMeta::timezone`,
//!   `ExecutionOrder`, `BinaryData`, `ExecutionMode`, `SideEffect`.

#![forbid(unsafe_code)]
// NOTE: `missing_docs` is intentionally NOT enforced crate-wide.
//
// Every public type, trait and function carries a doc comment explaining the
// decision it encodes and the audit finding it resolves — that is the contract
// and it is what a reader of `kernel` actually needs.
//
// The remaining undocumented items are struct *fields* on types that mirror the
// n8n JSON schema verbatim (`Item`, `BinaryData`, `EventRecord`, ...). Their
// names are the wire format, so a doc comment would restate the identifier.
// Field-level docs are tracked as a task, not gated here, because a wall of 300
// warnings hides the ones that matter.
#![warn(missing_copy_implementations, missing_debug_implementations)]

pub mod checkpoint;
pub mod context;
pub mod error;
pub mod event;
pub mod id;
pub mod item;
pub mod node;
pub mod params;
pub mod task;

// Re-export the types that make up the "integration contract" the blueprint
// named but never defined. If it is in this list, changing it is a breaking
// change for the whole engine and requires an ADR.
pub use checkpoint::{Checkpoint, CheckpointPolicy, WaitingTask, CHECKPOINT_SCHEMA_VERSION};
pub use context::{
    BlobStore, CredentialProvider, CredentialValue, EnvAccess, ExecutionMeta, ExecutionMode,
    ExecutionOrder, ExpressionEngine, ExpressionScope, HttpClient, Logger, LogLevel, NoCancel,
    NodeContext, NodeMeta, NoopLogger, PriorNodeRef, PriorOutputs, RequestBody, HttpRequest,
    ResponseBody, HttpResponse, StaticData, WorkflowMeta,
};
pub use error::{ErrorCode, KernelError, NodeError, Resource};
pub use event::{
    execution_status, EventRecord, ExecutionEvent, ExecutionStatus, GovernorLevel, WaitReason,
    SCHEMA_VERSION,
};
pub use id::{
    CheckpointId, ContentId, ExecutionId, NodeId, NodeKind, SpillPath, TaskId, WorkerId, WorkflowId,
};
pub use item::{
    spill_from_iter, BinaryContainer, BinaryData, BinaryLocation, Item, ItemCursor, ItemList,
    ItemView, PairedItem, SpillCodec, SpillPolicy, SpillStore, SpillWriter, SpilledList,
};
pub use node::{
    BufferingMode, Node, NodeDescriptor, NodeGroup, NodeOutput, ResourceHint, SideEffect, Weight,
};
pub use params::{
    CredentialSpec, DisplayCondition, ParameterField, ParameterKind, ParameterOption,
    ParameterSchema, ValidationError,
};
pub use task::{Lease, RecoveryAction, Task, TaskError, TaskStatus};

/// Version of this crate's public contract. Bump on any breaking change to the
/// re-exports above.
pub const CONTRACT_VERSION: u16 = 1;
