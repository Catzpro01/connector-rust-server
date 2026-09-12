//! Task — the scheduler's atomic unit of work (decision D4).

use crate::error::ErrorCode;
use crate::id::{ExecutionId, NodeId, TaskId, WorkerId};
use crate::node::ResourceHint;
use serde::{Deserialize, Serialize};

/// Three layers, per decision D4:
///
/// ```text
/// WorkflowExecution   "workflow #100 is running"     ← root container
///       └── NodeExecution  "HTTP node #5 is running"  ← domain record
///              └── Task      "attempt 1 of node #5"    ← scheduling primitive
/// ```
///
/// Task is what the scheduler queues, leases and retries. Keeping it separate
/// from NodeExecution is what lets the engine move from 1 process on a 2-core
/// VPS to 100 distributed workers without changing the execution model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Task {
    pub id: TaskId,
    pub execution_id: ExecutionId,
    pub node_id: NodeId,

    /// Which input index this task serves (multi-input nodes like Merge).
    #[serde(default)]
    pub input_index: u8,

    /// 1-based. Incremented by the retry policy (D14/D57).
    pub attempt: u16,

    /// D56 — adaptive priority. Higher runs first.
    pub priority: i8,

    pub status: TaskStatus,

    /// Copied from the node descriptor at planning time, so the scheduler never
    /// has to look the node up to make an admission decision.
    pub hints: ResourceHint,

    /// Set while `Running`. Enables crash recovery: an expired lease means the
    /// worker died and the task can be reclaimed (D8).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lease: Option<Lease>,

    /// Unix ms.
    pub created_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<i64>,

    /// Populated on failure. Drives retry vs quarantine decisions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_error: Option<TaskError>,

    /// For `Waiting`: when to wake up. Persisted, so a restart survives (D17).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wake_at: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lease {
    pub holder: WorkerId,
    /// Unix ms. A lease past this timestamp with status still `Running` means
    /// the holder crashed — the task is reclaimable.
    pub expires_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskError {
    pub code: ErrorCode,
    pub message: String,
    /// Whether the error itself was retryable — distinct from whether retrying
    /// is *safe*, which depends on `ResourceHint::side_effect` (audit A-10).
    pub retryable: bool,
}

/// Task lifecycle.
///
/// `InDoubt` is the state most engines get wrong. It exists because a crash
/// during a non-idempotent side effect has an unknowable outcome: the transfer
/// either happened or it did not, and no amount of reconciliation can tell.
/// Guessing in either direction is a correctness bug (audit A-10).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TaskStatus {
    /// Dependencies not yet satisfied.
    Pending,
    /// Eligible; waiting for a worker slot.
    Ready,
    /// Leased by a worker.
    Running,
    /// D17 — persisted wait. Holds **no** worker slot and no RAM.
    /// Used by Wait, Schedule resume, webhook resume and human approval (D97).
    Waiting,
    Success,
    Failed,
    /// Crashed mid-side-effect. Never auto-retried. Requires human review.
    InDoubt,
    /// D20 — cancelled cooperatively.
    Cancelled,
    /// D69 — retry limit exceeded, parked for inspection.
    Quarantined,
}

impl TaskStatus {
    /// Terminal states never transition again.
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            TaskStatus::Success
                | TaskStatus::Failed
                | TaskStatus::Cancelled
                | TaskStatus::Quarantined
        )
    }

    /// Occupies a worker slot. Note `Waiting` does NOT — that is the whole
    /// point of D17 and what prevents worker-starvation deadlock (audit A-09).
    pub fn holds_worker_slot(self) -> bool {
        matches!(self, TaskStatus::Running)
    }

    /// Holds resources that must not be GC'd (audit A-21).
    pub fn is_non_terminal(self) -> bool {
        !self.is_terminal()
    }

    /// Reclaimable after a crash without human intervention.
    pub fn auto_recoverable(self) -> bool {
        matches!(self, TaskStatus::Ready | TaskStatus::Running)
    }

    /// Requires a human decision before proceeding.
    pub fn needs_review(self) -> bool {
        matches!(self, TaskStatus::InDoubt | TaskStatus::Quarantined)
    }
}

impl Task {
    /// Whether crash recovery may simply re-queue this task.
    ///
    /// Combines the task's state with the node's declared side effects. This is
    /// the single place where audit A-10 gets resolved at runtime.
    pub fn safe_to_auto_retry(&self) -> bool {
        self.status.auto_recoverable() && self.hints.auto_retry_safe()
    }

    /// What recovery should do with this task after a crash.
    pub fn recovery_action(&self) -> RecoveryAction {
        match self.status {
            TaskStatus::Pending | TaskStatus::Ready => RecoveryAction::Requeue,

            TaskStatus::Running => match self.hints.side_effect {
                crate::node::SideEffect::None => RecoveryAction::Requeue,
                crate::node::SideEffect::Idempotent => RecoveryAction::RequeueWithIdempotencyKey,
                // Unknown outcome. Do not guess.
                crate::node::SideEffect::NonIdempotent => RecoveryAction::MarkInDoubt,
            },

            TaskStatus::Waiting => RecoveryAction::RestoreWait,

            TaskStatus::Success | TaskStatus::Failed | TaskStatus::Cancelled => {
                RecoveryAction::Noop
            }

            TaskStatus::InDoubt | TaskStatus::Quarantined => RecoveryAction::NeedsReview,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RecoveryAction {
    Noop,
    Requeue,
    RequeueWithIdempotencyKey,
    /// Reset to `Waiting` using the persisted `wake_at`.
    RestoreWait,
    /// Set `InDoubt`, mark the execution `NEEDS_REVIEW`, alert (D68).
    MarkInDoubt,
    /// Already flagged; keep flagged and surface to the user.
    NeedsReview,
}
