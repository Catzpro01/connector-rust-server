//! Checkpoints — recovery points (decision D6).
//!
//! A checkpoint records *which tasks had safely completed* at a moment in time.
//! On recovery the engine loads the newest checkpoint, resets in-flight tasks,
//! and continues. Work before the checkpoint is never repeated.
//!
//! # The rule that keeps this cheap
//!
//! **A checkpoint never contains payload data.** Items live in spill files and
//! blobs, referenced by `ContentId` from the `ItemList`s the engine already
//! persists. If checkpoints stored payloads, a 10 000-node execution would
//! write gigabytes of duplicated state and defeat the entire design.
//!
//! # When to write one (audit A-06)
//!
//! Not after every node. SQLite in WAL mode allows only one writer, so
//! checkpoint frequency is the dominant write cost in the whole engine. Write
//! checkpoints only at *recovery points*:
//!
//! * before/after a node with `SideEffect::NonIdempotent`
//! * at branch and merge boundaries
//! * every N completed tasks (configurable, default 50)
//! * on `TaskWaiting` (a wait can last weeks — it must survive a restart)

use crate::id::{CheckpointId, ExecutionId, TaskId};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// Current checkpoint format version. See `event::SCHEMA_VERSION` for why.
pub const CHECKPOINT_SCHEMA_VERSION: u16 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Checkpoint {
    pub id: CheckpointId,
    pub execution_id: ExecutionId,

    /// Persisted. Required for replay after an upgrade (D99).
    pub schema_version: u16,

    /// Tasks known-complete at this point. Never re-executed on recovery.
    ///
    /// `BTreeSet` rather than a bitmap crate: the kernel stays dependency-light,
    /// and `storage` can switch the on-disk encoding to a roaring bitmap later
    /// without touching this type.
    pub completed: BTreeSet<TaskId>,

    /// Tasks that were in flight. Recovery decides what to do with each via
    /// `Task::recovery_action()` — requeue, requeue-with-key, or mark InDoubt.
    pub in_flight: Vec<TaskId>,

    /// Tasks parked in `Waiting`, with their wake times, so long waits survive
    /// a restart (D17/D97).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub waiting: Vec<WaitingTask>,

    /// Monotonic event sequence covered by this checkpoint. Recovery replays
    /// only events after this point.
    pub event_seq: u64,

    /// Unix ms.
    pub created_at: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WaitingTask {
    pub task_id: TaskId,
    /// Unix ms, or `None` for event-driven waits (webhook / human approval).
    pub wake_at: Option<i64>,
}

impl Checkpoint {
    pub fn new(id: CheckpointId, execution_id: ExecutionId, event_seq: u64, created_at: i64) -> Self {
        Self {
            id,
            execution_id,
            schema_version: CHECKPOINT_SCHEMA_VERSION,
            completed: BTreeSet::new(),
            in_flight: Vec::new(),
            waiting: Vec::new(),
            event_seq,
            created_at,
        }
    }

    pub fn is_readable(&self) -> bool {
        self.schema_version <= CHECKPOINT_SCHEMA_VERSION
    }

    pub fn contains(&self, task: TaskId) -> bool {
        self.completed.contains(&task)
    }

    /// Cheap size estimate, for Governor accounting.
    pub fn estimated_bytes(&self) -> usize {
        64 + self.completed.len() * 8 + self.in_flight.len() * 8 + self.waiting.len() * 16
    }
}

/// Policy deciding when a checkpoint is worth its write cost.
///
/// Tunable, because the right answer depends on workload: side-effect-heavy
/// workflows want frequent checkpoints, pure-transform workflows want rare ones.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CheckpointPolicy {
    /// Write one every N completed tasks.
    pub every_n_tasks: u32,
    /// Always checkpoint around non-idempotent side effects.
    pub on_side_effect: bool,
    /// Always checkpoint at branch/merge boundaries.
    pub on_branch_boundary: bool,
    /// Always checkpoint when a task enters `Waiting`.
    pub on_wait: bool,
}

impl Default for CheckpointPolicy {
    fn default() -> Self {
        Self {
            every_n_tasks: 50,
            on_side_effect: true,
            on_branch_boundary: true,
            on_wait: true,
        }
    }
}
