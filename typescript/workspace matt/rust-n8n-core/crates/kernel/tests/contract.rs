//! In-memory `SpillStore` plus the round-trip property test that proves
//! the A-03 resolution: spilling must be invisible to node semantics.

use kernel::error::KernelError;
use kernel::id::SpillPath;
use kernel::item::{Item, SpillCodec, SpillStore, SpillWriter, SpilledList};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// A `SpillStore` backed by RAM, for tests.
///
/// The real implementation in `crates/data-plane` writes to disk with an offset
/// index. This one deliberately keeps identical observable behaviour so the
/// contract can be tested without touching the filesystem.
/// Shared state so a `SpillWriter` can register its file on `finish()` without
/// borrowing the store across a `self: Box<Self>` call. No `unsafe` needed.
#[derive(Default)]
struct MemState {
    files: Mutex<HashMap<String, Vec<Vec<u8>>>>,
    next_id: Mutex<u64>,
}

#[derive(Default)]
struct MemSpillStore {
    state: Arc<MemState>,
}

#[async_trait]
impl SpillStore for MemSpillStore {
    async fn write(&self, items: &[Item]) -> Result<SpilledList, KernelError> {
        let mut next = self.state.next_id.lock().unwrap();
        *next += 1;
        let path = format!("spill/{}", *next);

        // Serialize each item independently, mirroring how the disk store
        // records one offset per item.
        let mut blobs = Vec::with_capacity(items.len());
        let mut total = 0u64;
        for it in items {
            let bytes = serde_json::to_vec(it).map_err(|e| KernelError::Codec {
                codec: "json",
                reason: e.to_string(),
            })?;
            total += bytes.len() as u64;
            blobs.push(bytes);
        }

        self.state.files.lock().unwrap().insert(path.clone(), blobs);

        Ok(SpilledList {
            path: SpillPath::new(path),
            len: items.len() as u32,
            total_bytes: total,
            codec: SpillCodec::Json,
        })
    }

    async fn read_at(&self, handle: &SpilledList, index: usize) -> Result<Item, KernelError> {
        let files = self.state.files.lock().unwrap();
        let blobs = files
            .get(handle.path.as_str())
            .ok_or_else(|| KernelError::SpillIo {
                message: format!("no such spill file: {}", handle.path),
            })?;
        let bytes = blobs.get(index).ok_or(KernelError::IndexOutOfBounds {
            index,
            len: blobs.len(),
        })?;
        serde_json::from_slice(bytes).map_err(|e| KernelError::Codec {
            codec: "json",
            reason: e.to_string(),
        })
    }

    async fn read_range(
        &self,
        handle: &SpilledList,
        start: usize,
        len: usize,
    ) -> Result<Vec<Item>, KernelError> {
        let mut out = Vec::with_capacity(len);
        for i in start..start + len {
            out.push(self.read_at(handle, i).await?);
        }
        Ok(out)
    }

    async fn read_all(&self, handle: &SpilledList) -> Result<Vec<Item>, KernelError> {
        self.read_range(handle, 0, handle.len as usize).await
    }

    async fn delete(&self, handle: &SpilledList) -> Result<(), KernelError> {
        self.state.files.lock().unwrap().remove(handle.path.as_str());
        Ok(())
    }

    fn disk_footprint(&self, handle: &SpilledList) -> u64 {
        handle.total_bytes
    }

    async fn writer(&self) -> Result<Box<dyn SpillWriter>, KernelError> {
        let mut next = self.state.next_id.lock().unwrap();
        *next += 1;
        let path = format!("spill/{}", *next);
        drop(next);
        Ok(Box::new(MemWriter {
            state: Arc::clone(&self.state),
            path,
            blobs: Vec::new(),
            finished: false,
        }))
    }
}

/// Streaming writer for the in-memory store.
struct MemWriter {
    state: Arc<MemState>,
    path: String,
    blobs: Vec<Vec<u8>>,
    finished: bool,
}

#[async_trait]
impl SpillWriter for MemWriter {
    async fn push(&mut self, items: &[Item]) -> Result<(), KernelError> {
        for it in items {
            let bytes = serde_json::to_vec(it).map_err(|e| KernelError::Codec {
                codec: "json",
                reason: e.to_string(),
            })?;
            self.blobs.push(bytes);
        }
        Ok(())
    }

    async fn finish(mut self: Box<Self>) -> Result<SpilledList, KernelError> {
        let total: u64 = self.blobs.iter().map(|b| b.len() as u64).sum();
        let len = self.blobs.len() as u32;
        self.state
            .files
            .lock()
            .unwrap()
            .insert(self.path.clone(), std::mem::take(&mut self.blobs));
        self.finished = true;
        Ok(SpilledList {
            path: SpillPath::new(self.path.clone()),
            len,
            total_bytes: total,
            codec: SpillCodec::Json,
        })
    }

    fn pushed(&self) -> u32 {
        self.blobs.len() as u32
    }
}

/// An unfinished writer must not leave a partial file behind.
impl Drop for MemWriter {
    fn drop(&mut self) {
        if !self.finished {
            self.state.files.lock().unwrap().remove(&self.path);
        }
    }
}

// Minimal async executor so the test crate needs no tokio dependency
// (kernel itself is runtime-agnostic).
fn block_on<F: std::future::Future>(fut: F) -> F::Output {
    use std::task::{Context, Poll, Wake};
    use std::sync::atomic::{AtomicBool, Ordering};

    struct SpinWaker(AtomicBool);
    impl Wake for SpinWaker {
        fn wake(self: Arc<Self>) {
            self.0.store(true, Ordering::SeqCst);
        }
    }

    let spin = Arc::new(SpinWaker(AtomicBool::new(true)));
    let waker: std::task::Waker = spin.clone().into();
    let mut cx = Context::from_waker(&waker);
    let mut fut = Box::pin(fut);
    loop {
        match fut.as_mut().poll(&mut cx) {
            Poll::Ready(v) => return v,
            Poll::Pending => {
                spin.0.store(false, Ordering::SeqCst);
                std::hint::spin_loop();
            }
        }
    }
}

fn sample_items(n: usize) -> Vec<Item> {
    (0..n)
        .map(|i| {
            Item::new(json!({
                "id": i,
                "name": format!("item-{}", i),
                "nested": { "values": [i, i * 2, i * 3] },
                "flag": i % 2 == 0,
            }))
        })
        .collect()
}

/// THE property that makes the whole spill design safe:
/// a spilled list must be observationally identical to an inline list.
///
/// If this ever fails, `Merge`, `Sort`, `pairedItem`, `$items()` and
/// `$('Node').item` all break in ways that only show up on large workflows —
/// exactly the bug class audit A-03 predicted.
#[test]
fn spilled_and_inline_are_observationally_identical() {
    let store = MemSpillStore::default();
    let items = sample_items(250);

    // Force spilling with an aggressive policy.
    let spill_policy = kernel::item::SpillPolicy {
        max_inline_items: 10,
        max_inline_bytes: 64,
        max_inline_binary_bytes: 256 * 1024,
    };

    let spilled = block_on(build_list(&items, &spill_policy, &store));
    assert!(spilled.is_spilled(), "policy should have forced a spill");
    assert_eq!(spilled.len(), items.len());

    let inline = kernel::ItemList::Inline(items.clone());
    assert!(!inline.is_spilled());
    assert_eq!(inline.len(), items.len());

    // Same length.
    assert_eq!(spilled.len(), inline.len());

    // Same content, item by item, in both directions.
    let sview = spilled.view(&store);
    let iview = inline.view(&store);
    for (i, expected) in items.iter().enumerate() {
        let a = block_on(sview.get(i)).expect("spilled get");
        let b = block_on(iview.get(i)).expect("inline get");
        assert_eq!(a, b, "mismatch at index {}", i);
        assert_eq!(a.json, expected.json);
    }

    // Same content via materialize_all.
    assert_eq!(block_on(sview.materialize_all()).unwrap(), items);
    assert_eq!(block_on(iview.materialize_all()).unwrap(), items);

    // Same content via chunked cursor, for several chunk sizes.
    for chunk in [1usize, 7, 64, 249, 250, 1000] {
        let mut cur_s = sview.cursor(chunk);
        let mut cur_i = iview.cursor(chunk);
        let mut flat_s = Vec::new();
        let mut flat_i = Vec::new();
        while let Some(c) = block_on(cur_s.next_chunk()).unwrap() {
            flat_s.extend(c);
        }
        while let Some(c) = block_on(cur_i.next_chunk()).unwrap() {
            flat_i.extend(c);
        }
        assert_eq!(flat_s, items, "cursor chunk={} diverged (spilled)", chunk);
        assert_eq!(flat_i, items, "cursor chunk={} diverged (inline)", chunk);
        assert!(cur_s.is_finished() && cur_i.is_finished());
    }

    // Random access still works after everything else — this is the
    // $('Node').item guarantee (audit A-01).
    let last = block_on(sview.get(items.len() - 1)).unwrap();
    assert_eq!(last.json["id"], json!(249));

    // Out-of-bounds is an error, not a panic, on both variants.
    assert!(block_on(sview.get(items.len())).is_err());
    assert!(block_on(iview.get(items.len())).is_err());
}

/// The RAM claim itself: a spilled list must hold almost nothing in memory.
#[test]
fn spilled_list_holds_no_payload_in_ram() {
    let store = MemSpillStore::default();
    let items = sample_items(5_000);

    let policy = kernel::item::SpillPolicy {
        max_inline_items: 100,
        max_inline_bytes: 1024,
        max_inline_binary_bytes: 256 * 1024,
    };
    let spilled = block_on(build_list(&items, &policy, &store));

    let inline = kernel::ItemList::Inline(items.clone());

    let inline_ram = inline.ram_footprint();
    let spilled_ram = spilled.ram_footprint();

    assert!(spilled_ram < 512, "spilled metadata was {} bytes", spilled_ram);
    assert!(
        inline_ram > spilled_ram * 50,
        "expected a large gap: inline={} spilled={}",
        inline_ram,
        spilled_ram
    );

    // But the data is all still there on "disk".
    assert_eq!(spilled.len(), 5_000);
    let view = spilled.view(&store);
    assert_eq!(block_on(view.get(4_999)).unwrap().json["id"], json!(4999));
}

#[test]
fn empty_and_single_item_lists_behave() {
    let store = MemSpillStore::default();
    let empty = kernel::ItemList::empty();
    assert!(empty.is_empty());
    assert_eq!(empty.len(), 0);
    let ev = empty.view(&store);
    assert_eq!(block_on(ev.first()).unwrap(), None);
    assert!(block_on(ev.get(0)).is_err());

    let one = kernel::ItemList::single(Item::new(json!({"a": 1})));
    assert_eq!(one.len(), 1);
    let ov = one.view(&store);
    assert_eq!(block_on(ov.first()).unwrap().unwrap().json, json!({"a": 1}));
}

#[test]
fn binary_location_ram_accounting() {
    use kernel::item::{BinaryData, BinaryLocation};
    use kernel::id::ContentId;

    let inline = BinaryData {
        mime_type: "image/png".into(),
        file_name: Some("a.png".into()),
        location: BinaryLocation::Inline {
            data: "x".repeat(100_000),
            id: None,
        },
    };
    let reffed = BinaryData {
        mime_type: "image/png".into(),
        file_name: Some("a.png".into()),
        location: BinaryLocation::Ref {
            content_id: ContentId::new(7),
            size_bytes: 100_000,
        },
    };

    // Inline holds the bytes; a reference does not.
    assert!(inline.location.ram_footprint() >= 100_000);
    assert_eq!(reffed.location.ram_footprint(), 0);

    let mut it = Item::new(json!({}));
    it.binary = Some([("data".to_string(), reffed)].into_iter().collect());
    assert!(it.estimated_bytes() < 1000, "reference must stay cheap");
}

#[test]
fn parameter_schema_validation_and_visibility() {
    use kernel::params::{DisplayCondition, ParameterField, ParameterKind, ParameterSchema};

    let schema = ParameterSchema::empty()
        .field(
            ParameterField::new("method", "Method", ParameterKind::Options)
                .required()
                .with_default(json!("GET")),
        )
        .field(
            ParameterField::new("url", "URL", ParameterKind::String)
                .required()
                .expression(),
        )
        .field(
            ParameterField::new("body", "Body", ParameterKind::Json)
                .shown_when(DisplayCondition::show("method", json!("POST"))),
        );

    // Valid GET: body absent and hidden, so not required.
    assert!(schema.validate(&json!({"method": "GET", "url": "https://x"})).is_empty());

    // Missing required field.
    let errs = schema.validate(&json!({"method": "GET"}));
    assert_eq!(errs.len(), 1);
    assert_eq!(errs[0].field, "url");

    // Wrong type.
    let errs = schema.validate(&json!({"method": "GET", "url": 42}));
    assert_eq!(errs.len(), 1);
    assert!(errs[0].message.contains("expected string"));

    // POST makes body visible; visibility is driven by the same schema the
    // editor renders from (audit A-30).
    let body_field = schema.lookup("body").unwrap();
    assert!(!body_field.is_visible(&json!({"method": "GET"})));
    assert!(body_field.is_visible(&json!({"method": "POST"})));
}

#[test]
fn execution_status_prefers_review_over_success() {
    use kernel::event::execution_status;
    use kernel::task::TaskStatus;

    assert_eq!(
        execution_status([TaskStatus::Success, TaskStatus::Success]),
        kernel::ExecutionStatus::Success
    );
    assert_eq!(
        execution_status([TaskStatus::Success, TaskStatus::Failed]),
        kernel::ExecutionStatus::Failed
    );
    // An InDoubt task must never be reported as plain success (audit A-10).
    assert_eq!(
        execution_status([TaskStatus::Success, TaskStatus::InDoubt]),
        kernel::ExecutionStatus::NeedsReview
    );
    assert_eq!(
        execution_status([TaskStatus::Success, TaskStatus::Quarantined]),
        kernel::ExecutionStatus::NeedsReview
    );
    assert_eq!(
        execution_status([TaskStatus::Running, TaskStatus::Success]),
        kernel::ExecutionStatus::StillRunning
    );
    assert_eq!(
        execution_status([TaskStatus::Waiting, TaskStatus::Success]),
        kernel::ExecutionStatus::Waiting
    );
}

#[test]
fn recovery_action_respects_side_effects() {
    use kernel::node::{ResourceHint, SideEffect};
    use kernel::task::{RecoveryAction, Task, TaskStatus};
    use kernel::id::{ExecutionId, NodeId, TaskId};

    let base = Task {
        id: TaskId::new(1),
        execution_id: ExecutionId::new(1),
        node_id: NodeId::new("http"),
        input_index: 0,
        attempt: 1,
        priority: 0,
        status: TaskStatus::Running,
        hints: ResourceHint::default(),
        lease: None,
        created_at: 0,
        started_at: None,
        finished_at: None,
        last_error: None,
        wake_at: None,
    };

    // Pure node crashed mid-run: safe to just requeue.
    let mut t = base.clone();
    t.hints.side_effect = SideEffect::None;
    assert_eq!(t.recovery_action(), RecoveryAction::Requeue);
    assert!(t.safe_to_auto_retry());

    // Idempotent node: requeue, but with an idempotency key (D60).
    let mut t = base.clone();
    t.hints.side_effect = SideEffect::Idempotent;
    assert_eq!(t.recovery_action(), RecoveryAction::RequeueWithIdempotencyKey);

    // Non-idempotent node: the outcome is unknowable. Never guess.
    let mut t = base.clone();
    t.hints.side_effect = SideEffect::NonIdempotent;
    assert_eq!(t.recovery_action(), RecoveryAction::MarkInDoubt);
    assert!(!t.safe_to_auto_retry());

    // Waiting tasks restore their wait instead of consuming a worker slot
    // (this is what prevents the audit A-09 starvation deadlock).
    let mut t = base.clone();
    t.status = TaskStatus::Waiting;
    assert_eq!(t.recovery_action(), RecoveryAction::RestoreWait);
    assert!(!TaskStatus::Waiting.holds_worker_slot());
    assert!(TaskStatus::Running.holds_worker_slot());

    // Terminal states are left alone.
    let mut t = base.clone();
    t.status = TaskStatus::Success;
    assert_eq!(t.recovery_action(), RecoveryAction::Noop);
}

#[test]
fn static_data_flushes_only_in_production() {
    use kernel::context::{ExecutionMode, StaticData};

    let mut sd = StaticData::new();
    assert!(!sd.is_dirty());

    sd.set("global", "lastId", json!(42));
    assert!(sd.is_dirty());
    assert_eq!(sd.get("global", "lastId"), Some(&json!(42)));

    // D106: n8n only persists static data on production executions.
    assert!(sd.should_flush(ExecutionMode::Production));
    assert!(!sd.should_flush(ExecutionMode::Manual));
    assert!(!sd.should_flush(ExecutionMode::Test));

    sd.mark_flushed();
    assert!(!sd.is_dirty());
    assert!(!sd.should_flush(ExecutionMode::Production));

    // Node-scoped data is separate from global.
    sd.set("node:abc", "cursor", json!("2026-01-01"));
    assert_eq!(sd.get("global", "cursor"), None);
    assert_eq!(sd.get("node:abc", "cursor"), Some(&json!("2026-01-01")));

    sd.remove("node:abc", "cursor");
    assert_eq!(sd.get("node:abc", "cursor"), None);
}

#[test]
fn event_records_are_versioned() {
    use kernel::event::{EventRecord, ExecutionEvent, SCHEMA_VERSION};
    use kernel::id::ExecutionId;

    let rec = EventRecord::new(
        1,
        1_700_000_000_000,
        ExecutionEvent::ExecutionCreated {
            execution_id: ExecutionId::new(9),
            workflow_version: 3,
            mode: "production".into(),
        },
    );
    assert_eq!(rec.schema_version, SCHEMA_VERSION);
    assert!(rec.is_readable());

    // Round-trips through JSON, which is what SQLite storage does.
    let s = serde_json::to_string(&rec).unwrap();
    let back: EventRecord = serde_json::from_str(&s).unwrap();
    assert_eq!(back, rec);

    // A record from a future version is flagged, not silently misread (A-16).
    let mut future = rec.clone();
    future.schema_version = SCHEMA_VERSION + 1;
    assert!(!future.is_readable());
}

#[test]
fn credential_value_never_leaks_in_debug() {
    use kernel::context::CredentialValue;

    let c = CredentialValue::new(json!({"apiKey": "sk-secret-123", "user": "bob"}));
    let dbg = format!("{:?}", c);
    let disp = format!("{}", c);

    assert!(!dbg.contains("sk-secret-123"), "Debug leaked: {}", dbg);
    assert!(!disp.contains("sk-secret-123"), "Display leaked: {}", disp);
    // But the value is still usable by the node.
    assert_eq!(c.get("apiKey"), Some(&json!("sk-secret-123")));
}

#[test]
fn resource_hints_default_to_safe_values() {
    use kernel::node::{BufferingMode, ResourceHint, SideEffect};

    let h = ResourceHint::default();
    // A-03: n8n semantics are batch by default, streaming is opt-in.
    assert_eq!(h.buffering, BufferingMode::Batch);
    assert!(!h.can_stream());
    // A-10: assume no side effect unless the node says otherwise.
    assert_eq!(h.side_effect, SideEffect::None);
    assert!(h.auto_retry_safe());
}

/// Build an `ItemList` from a slice, applying the spill policy.
///
/// A free function rather than an inherent impl: `Item` is foreign to this
/// crate, and Rust forbids inherent impls on foreign types.
async fn build_list(
    items: &[Item],
    policy: &kernel::item::SpillPolicy,
    store: &MemSpillStore,
) -> kernel::ItemList {
    kernel::ItemList::from_vec(items.to_vec(), policy, store)
        .await
        .expect("spill write")
}

// Silence unused-import warnings in configurations where Value is only used in macros.
#[allow(dead_code)]
fn _assert_value_in_scope(_: Value) {}

#[allow(dead_code)]
fn _arc_is_send_sync<T: Send + Sync>(_: Arc<T>) {}

// ═══════════════════════════════════════════════════════════════════════════
// Streaming write path (SpillWriter / spill_from_iter)
//
// This is the path real triggers use, and the one that makes peak RAM bounded
// during *production* of items rather than only after it. It gets its own
// tests because a bug here is invisible at small N: it only shows up when a
// batch boundary lands wrong, i.e. exactly on large workflows.
// ═══════════════════════════════════════════════════════════════════════════

impl MemSpillStore {
    /// Number of live spill files. Used to assert nothing leaks.
    fn file_count(&self) -> usize {
        self.state.files.lock().unwrap().len()
    }
}

/// The streaming path must produce exactly what the batch path produces.
#[test]
fn spill_from_iter_matches_from_vec() {
    let store = MemSpillStore::default();
    let items: Vec<Item> = sample_items(1_000);

    let streamed = block_on(kernel::spill_from_iter(
        64,
        &store,
        items.iter().cloned(),
    ))
    .expect("stream");
    assert!(streamed.is_spilled());

    let policy = kernel::item::SpillPolicy {
        max_inline_items: 1,
        max_inline_bytes: 1,
        max_inline_binary_bytes: 1,
    };
    let batched = block_on(kernel::ItemList::from_vec(items.clone(), &policy, &store))
        .expect("batch");
    assert!(batched.is_spilled());

    // Same length, same content at every index, same full materialization.
    assert_eq!(streamed.len(), batched.len());
    let sv = streamed.view(&store);
    let bv = batched.view(&store);
    for (i, expected) in items.iter().enumerate() {
        assert_eq!(&block_on(sv.get(i)).unwrap(), expected, "stream idx {}", i);
        assert_eq!(&block_on(bv.get(i)).unwrap(), expected, "batch idx {}", i);
    }
    assert_eq!(block_on(sv.materialize_all()).unwrap(), items);
}

/// Batch boundaries are where off-by-one bugs live. Cover every alignment.
#[test]
fn spill_from_iter_handles_all_batch_boundaries() {
    let store = MemSpillStore::default();
    let items: Vec<Item> = sample_items(100);

    // batch = 1 (every item its own push), 3 (non-divisor), 50 (exact half),
    // 100 (exact), 101 (larger than the whole set), and a huge value.
    for batch in [1usize, 3, 7, 50, 99, 100, 101, 10_000] {
        let list = block_on(kernel::spill_from_iter(
            batch,
            &store,
            items.iter().cloned(),
        ))
        .unwrap_or_else(|e| panic!("batch={}: {}", batch, e));

        assert_eq!(list.len(), items.len(), "batch={}", batch);
        let v = list.view(&store);
        for (i, expected) in items.iter().enumerate() {
            assert_eq!(
                &block_on(v.get(i)).unwrap(),
                expected,
                "batch={} idx={}",
                batch,
                i
            );
        }
        // Cursor must also survive every boundary.
        let mut cur = v.cursor(batch.max(1));
        let mut flat = Vec::new();
        while let Some(c) = block_on(cur.next_chunk()).unwrap() {
            flat.extend(c);
        }
        assert_eq!(flat, items, "cursor diverged at batch={}", batch);
    }
}

/// An empty iterator must yield an empty list and leave no file behind.
#[test]
fn spill_from_iter_empty_leaves_no_file() {
    let store = MemSpillStore::default();
    let before = store.file_count();

    let list = block_on(kernel::spill_from_iter(
        64,
        &store,
        std::iter::empty::<Item>(),
    ))
    .expect("empty");

    assert!(list.is_empty());
    assert!(!list.is_spilled(), "empty should not be a spilled handle");
    assert_eq!(
        store.file_count(),
        before,
        "empty spill must not leak a file (audit A-21)"
    );
}

/// A single item still round-trips through the streaming path.
#[test]
fn spill_from_iter_single_item() {
    let store = MemSpillStore::default();
    let list = block_on(kernel::spill_from_iter(
        1_000,
        &store,
        std::iter::once(Item::new(json!({"only": true}))),
    ))
    .expect("single");

    assert_eq!(list.len(), 1);
    let v = list.view(&store);
    assert_eq!(block_on(v.first()).unwrap().unwrap().json, json!({"only": true}));
}

/// Dropping a writer mid-flight must not leave a partial file registered.
///
/// This is the abort path: an execution cancelled while a trigger is still
/// producing items. Leaking here is how a VPS fills its disk over months.
#[test]
fn unfinished_writer_leaves_nothing_behind() {
    let store = MemSpillStore::default();
    let before = store.file_count();

    block_on(async {
        let mut w = store.writer().await.expect("writer");
        w.push(&sample_items(10)).await.expect("push");
        assert_eq!(w.pushed(), 10);
        w.push(&sample_items(5)).await.expect("push2");
        assert_eq!(w.pushed(), 15);
        // Deliberately NOT calling finish() — just drop it.
        drop(w);
    });

    assert_eq!(
        store.file_count(),
        before,
        "unfinished writer leaked a partial file"
    );
}

/// `pushed()` must track growth so the Governor can observe mid-write (D72).
#[test]
fn writer_reports_progress() {
    let store = MemSpillStore::default();
    block_on(async {
        let mut w = store.writer().await.expect("writer");
        assert_eq!(w.pushed(), 0);
        w.push(&sample_items(7)).await.unwrap();
        assert_eq!(w.pushed(), 7);
        w.push(&sample_items(3)).await.unwrap();
        assert_eq!(w.pushed(), 10);
        let handle = w.finish().await.expect("finish");
        assert_eq!(handle.len, 10);
        assert!(handle.total_bytes > 0);
    });
}

/// After `finish`, the handle must be readable through the normal API —
/// the writer path and the read path have to agree on the format.
#[test]
fn writer_output_readable_through_store_api() {
    let store = MemSpillStore::default();
    let items = sample_items(300);

    let handle = block_on(async {
        let mut w = store.writer().await.expect("writer");
        for chunk in items.chunks(37) {
            w.push(chunk).await.expect("push");
        }
        w.finish().await.expect("finish")
    });

    assert_eq!(handle.len as usize, items.len());
    assert_eq!(handle.codec, kernel::item::SpillCodec::Json);

    // read_at
    assert_eq!(block_on(store.read_at(&handle, 0)).unwrap(), items[0]);
    assert_eq!(block_on(store.read_at(&handle, 299)).unwrap(), items[299]);

    // read_range spanning several push boundaries
    let r = block_on(store.read_range(&handle, 30, 50)).unwrap();
    assert_eq!(r, items[30..80]);

    // read_all
    assert_eq!(block_on(store.read_all(&handle)).unwrap(), items);

    // out of bounds is an error, not a panic
    assert!(block_on(store.read_at(&handle, 300)).is_err());

    // delete removes it
    block_on(store.delete(&handle)).unwrap();
    assert!(block_on(store.read_at(&handle, 0)).is_err());
}

/// Peak RAM during streaming must stay bounded by the batch, not by N.
///
/// This is the actual claim behind D3, asserted structurally: the list built
/// from 50k items via streaming holds no more RAM than one built from 500.
#[test]
fn streaming_ram_is_independent_of_n() {
    let store = MemSpillStore::default();

    let small = block_on(kernel::spill_from_iter(
        100,
        &store,
        sample_items(500),
    ))
    .unwrap();
    let large = block_on(kernel::spill_from_iter(
        100,
        &store,
        sample_items(50_000),
    ))
    .unwrap();

    let rs = small.ram_footprint();
    let rl = large.ram_footprint();

    // Both are metadata-only, so both are tiny and nearly equal — the list
    // struct itself does not grow with item count.
    assert!(rs < 512, "small list held {} bytes", rs);
    assert!(rl < 512, "large list held {} bytes", rl);
    assert_eq!(large.len(), 50_000);

    // Sanity: the equivalent inline list would be orders of magnitude bigger.
    let inline = kernel::ItemList::Inline(sample_items(50_000));
    assert!(
        inline.ram_footprint() > rl * 100,
        "inline={} spilled={}",
        inline.ram_footprint(),
        rl
    );
}
