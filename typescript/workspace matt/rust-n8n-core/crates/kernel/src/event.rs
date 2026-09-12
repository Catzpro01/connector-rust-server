//! Execution event log (decision D6, resolution of audit A-16).
//!
//! Events are the append-only record of what happened. Checkpoints are periodic
//! recovery points derived from them. Together they let a crashed execution
//! resume without repeating work that already succeeded.
//!
//! # The versioning rule
//!
//! Every record carries `schema_version`. Without it, upgrading the engine
//! makes old event logs unreplayable — which silently breaks crash recovery for
//! any execution that was in flight during the upgrade, and makes D99
//! (backward-compatible upgrade + rollback) impossible.
//!
//! Variants may be **added**. They may not be renamed, reordered, or removed
//! without bumping `SCHEMA_VERSION` and writing a replay migrator.

use crate::error::ErrorCode;
use crate::id::{CheckpointId, ExecutionId, NodeId, TaskId};
use crate::task::TaskStatus;
use serde::{Deserialize, Serialize};

/// Current on-disk event format version.
pub const SCHEMA_VERSION: u16 = 1;

/// One entry in the execution event log.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EventRecord {
    /// MUST be persisted. Read it before interpreting `event`.
    pub schema_version: u16,

    /// Monotonic within an execution. Ordering is by this, not by timestamp,
    /// because the clock can jump.
    pub seq: u64,

    /// Unix ms.
    pub ts: i64,

    #[serde(flatten)]
    pub event: ExecutionEvent,
}

impl EventRecord {
    pub fn new(seq: u64, ts: i64, event: ExecutionEvent) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            seq,
            ts,
            event,
        }
    }

    /// True if a reader built against `SCHEMA_VERSION` can interpret this.
    pub fn is_readable(&self) -> bool {
        self.schema_version <= SCHEMA_VERSION
    }
}

/// What can happen during an execution.
///
/// Kept deliberately coarse: this log is for recovery and debugging, not for
/// fine-grained tracing. High-frequency detail belongs in metrics (D64) and
/// spans (D65), which are sampled and buffered separately.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ExecutionEvent {
    ExecutionCreated {
        execution_id: ExecutionId,
        workflow_version: u32,
        mode: String,
    },

    TaskCreated {
        task_id: TaskId,
        node_id: NodeId,
    },

    TaskReady {
        task_id: TaskId,
    },

    TaskStarted {
        task_id: TaskId,
        attempt: u16,
    },

    TaskCompleted {
        task_id: TaskId,
        /// Items produced per output branch — metadata only, never payloads.
        branch_lengths: Vec<u32>,
        duration_ms: u64,
        /// Peak RAM attributed to this node, for the governor to learn from.
        peak_ram_bytes: u64,
    },

    TaskFailed {
        task_id: TaskId,
        code: ErrorCode,
        retryable: bool,
        attempt: u16,
    },

    /// Audit A-10: the process died while a non-idempotent node was running.
    /// The outcome is unknowable; a human must decide.
    TaskInDoubt {
        task_id: TaskId,
        node_id: NodeId,
        reason: String,
    },

    TaskRetrying {
        task_id: TaskId,
        attempt: u16,
        backoff_ms: u32,
    },

    TaskWaiting {
        task_id: TaskId,
        /// Unix ms. Persisted so a restart can restore the wait (D17).
        wake_at: Option<i64>,
        reason: WaitReason,
    },

    TaskResumed {
        task_id: TaskId,
    },

    TaskQuarantined {
        task_id: TaskId,
        code: ErrorCode,
    },

    TaskCancelled {
        task_id: TaskId,
    },

    /// D6 — a recovery point was written.
    CheckpointWritten {
        checkpoint_id: CheckpointId,
        completed_tasks: u32,
    },

    /// D5/D72 — memory governor changed level.
    ResourcePressure {
        level: GovernorLevel,
        ram_used_bytes: u64,
        ram_budget_bytes: u64,
        /// Bytes pushed to disk to relieve pressure.
        spilled_bytes: u64,
    },

    ExecutionCompleted {
        execution_id: ExecutionId,
        duration_ms: u64,
        status: ExecutionStatus,
    },

    ExecutionFailed {
        execution_id: ExecutionId,
        code: ErrorCode,
    },

    ExecutionCancelled {
        execution_id: ExecutionId,
    },

    /// Recovery ran after a crash (D40).
    RecoveryPerformed {
        execution_id: ExecutionId,
        requeued: u32,
        marked_in_doubt: u32,
        restored_waits: u32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WaitReason {
    /// Wait/Delay node.
    Timer,
    /// Waiting for an external webhook to resume.
    Webhook,
    /// D97 — waiting for a human decision.
    HumanApproval,
    /// Waiting on a subworkflow execution.
    Subworkflow,
    /// Rate-limited by the governor (D59).
    Throttled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum GovernorLevel {
    Normal,
    Pressure,
    Critical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionStatus {
    Success,
    Failed,
    Cancelled,
    /// Contains `InDoubt` or `Quarantined` tasks. Needs human review.
    NeedsReview,
    StillRunning,
    Waiting,
}

impl ExecutionStatus {
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            ExecutionStatus::Success | ExecutionStatus::Failed | ExecutionStatus::Cancelled
        )
    }
}

/// Derive overall execution status from task states.
///
/// `InDoubt` and `Quarantined` dominate: an execution containing either is
/// `NeedsReview`, never plain `Success`. Surfacing a partial failure as success
/// is the worst possible outcome for a data pipeline.
pub fn execution_status(tasks: impl IntoIterator<Item = TaskStatus>) -> ExecutionStatus {
    let mut any_failed = false;
    let mut any_review = false;
    let mut any_waiting = false;
    let mut any_running = false;
    let mut count = 0u32;

    for s in tasks {
        count += 1;
        match s {
            TaskStatus::Failed => any_failed = true,
            TaskStatus::InDoubt | TaskStatus::Quarantined => any_review = true,
            TaskStatus::Waiting => any_waiting = true,
            TaskStatus::Running | TaskStatus::Ready | TaskStatus::Pending => any_running = true,
            TaskStatus::Success | TaskStatus::Cancelled => {}
        }
    }

    if count == 0 {
        return ExecutionStatus::Success;
    }
    if any_review {
        ExecutionStatus::NeedsReview
    } else if any_failed {
        ExecutionStatus::Failed
    } else if any_running {
        ExecutionStatus::StillRunning
    } else if any_waiting {
        ExecutionStatus::Waiting
    } else {
        ExecutionStatus::Success
    }
}
