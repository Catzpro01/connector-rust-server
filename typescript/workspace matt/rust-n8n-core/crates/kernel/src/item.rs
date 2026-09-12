//! The data model that flows between nodes.
//!
//! # Why this module is the centre of the whole design
//!
//! Two CRITICAL audit findings both reduce to one question — *what shape is the
//! data a node receives?*
//!
//! * **A-01** — n8n expressions need random access to *any* previous node's
//!   output (`$('HTTP Request').item.json`). So prior outputs must stay
//!   **addressable** for the whole execution.
//! * **A-03** — n8n passes **arrays of items**. Nodes such as `Merge`, `Sort`,
//!   `Aggregate` and `RemoveDuplicates` genuinely need the whole array, and
//!   `pairedItem` lineage needs it too. Making streaming the default (old D11)
//!   silently breaks all of them.
//!
//! # Resolution
//!
//! > **Addressable ≠ resident in RAM.**
//!
//! A list stays a fully indexable array as far as any node can observe. Its
//! *backing store* may be on disk. A node asks for item 4 000 and gets it;
//! whether that came from a `Vec` or from a `pread` is invisible to it.
//!
//! ```text
//!                     n8n (Node.js)              this engine (Rust)
//!                     -------------              ------------------
//!  1 000 000 items -> every item a live JS   ->  index in RAM (~12 MB)
//!                     object in the heap         items in a spill file
//!                     ~ several GB               deserialized one at a time
//!                     -> OOM on a 2 GB VPS       -> RAM stays < 100 MB
//! ```
//!
//! That is the entire efficiency thesis of this project, and it does not
//! require changing a single observable n8n semantic.

use crate::error::KernelError;
use crate::id::{ContentId, SpillPath};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeMap;

// ─────────────────────────────────────────────────────────────────────────────
// Item
// ─────────────────────────────────────────────────────────────────────────────

/// One n8n-compatible item.
///
/// Field shape intentionally mirrors `INodeExecutionData` so that n8n workflow
/// JSON and execution dumps deserialize without a translation layer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Item {
    /// The payload proper.
    ///
    /// This is `serde_json::Value` and **not** a generic parameter, on purpose:
    /// expressions like `$json.user.email` must work uniformly across every
    /// node, and monomorphizing per node type would explode both compile time
    /// and binary size.
    pub json: Value,

    /// n8n's `binary` property (decision D105).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub binary: Option<BinaryContainer>,

    /// Lineage tracking.
    ///
    /// Do **not** remove this in the name of efficiency. Without it the editor
    /// cannot highlight which output item came from which input item, and
    /// `pairedItem`-dependent nodes (Merge in "combine by position" mode,
    /// Compare Datasets) break. It costs ~8 bytes per item.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paired_item: Option<PairedItem>,
}

impl Item {
    pub fn new(json: Value) -> Self {
        Self {
            json,
            binary: None,
            paired_item: None,
        }
    }

    pub fn from_obj(map: Map<String, Value>) -> Self {
        Self::new(Value::Object(map))
    }

    /// Cheap upper-bound estimate of the heap this item occupies.
    ///
    /// Used by the Resource Governor (D72) to decide whether a list should be
    /// spilled. It is an *estimate* — deliberately so: an exact figure would
    /// require serializing, which costs more than the decision it informs.
    pub fn estimated_bytes(&self) -> usize {
        estimate_value(&self.json)
            + self
                .binary
                .as_ref()
                .map_or(0, |b| b.values().map(binary_data_ram).sum::<usize>())
            + if self.paired_item.is_some() { 12 } else { 0 }
    }
}

fn binary_data_ram(b: &BinaryData) -> usize {
    match &b.location {
        // base64 is inline in RAM
        BinaryLocation::Inline { data, .. } => data.len() + 64,
        // only the reference is in RAM; bytes live in the blob store
        BinaryLocation::Ref { .. } => 48,
    }
}

fn estimate_value(v: &Value) -> usize {
    match v {
        Value::Null => 8,
        Value::Bool(_) => 8,
        Value::Number(n) => n.to_string().len().max(8),
        Value::String(s) => s.len() + 24,
        Value::Array(a) => 24 + a.iter().map(estimate_value).sum::<usize>(),
        Value::Object(m) => {
            48 + m
                .iter()
                .map(|(k, val)| k.len() + 24 + estimate_value(val))
                .sum::<usize>()
        }
    }
}

/// n8n names binary attachments per key: `data`, `data1`, `thumbnail`, ...
pub type BinaryContainer = BTreeMap<String, BinaryData>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BinaryData {
    #[serde(rename = "mimeType")]
    pub mime_type: String,

    #[serde(rename = "fileName", default, skip_serializing_if = "Option::is_none")]
    pub file_name: Option<String>,

    /// Where the bytes actually are. Never both variants at once.
    #[serde(flatten)]
    pub location: BinaryLocation,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BinaryLocation {
    /// Small payload, base64 inline. Matches n8n's default
    /// `N8N_DEFAULT_BINARY_DATA_MODE=database`.
    Inline {
        /// base64-encoded bytes
        data: String,
        #[serde(default)]
        id: Option<String>,
    },
    /// Large payload: a reference into the blob store (D30).
    ///
    /// This is where the RAM saving comes from. The threshold that decides
    /// Inline vs Ref is configuration, not a constant — see `SpillPolicy`.
    Ref {
        content_id: ContentId,
        size_bytes: u64,
    },
}

impl BinaryLocation {
    /// Bytes held in RAM by this location.
    pub fn ram_footprint(&self) -> usize {
        match self {
            BinaryLocation::Inline { data, .. } => data.len(),
            BinaryLocation::Ref { .. } => 0,
        }
    }

    pub fn is_inline(&self) -> bool {
        matches!(self, BinaryLocation::Inline { .. })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairedItem {
    #[serde(rename = "itemId")]
    pub item_id: u32,

    #[serde(
        rename = "inputNodeIndex",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub input_node_index: Option<u8>,
}

// ─────────────────────────────────────────────────────────────────────────────
// ItemList
// ─────────────────────────────────────────────────────────────────────────────

/// The list of items flowing between two nodes.
///
/// **Resolution of audit A-03:** the observable semantic is an *array*, exactly
/// like n8n. Streaming is an internal optimisation hidden behind this enum, not
/// a change to what a node sees.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ItemList {
    /// Small enough to live entirely in RAM. Zero indirection cost.
    Inline(Vec<Item>),

    /// Too large: items are in a spill file, only metadata is here.
    Spilled(SpilledList),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpilledList {
    pub path: SpillPath,
    /// Number of items. Kept here so `len()` never touches the store.
    pub len: u32,
    /// Total serialized size, for Governor accounting (D72) and metrics (D64).
    pub total_bytes: u64,
    pub codec: SpillCodec,
}

/// Serialization format used for spilled items.
///
/// JSON is the fallback and the only variant the kernel can itself produce, so
/// that `kernel` stays dependency-light. `data-plane` is expected to add a
/// compact binary codec (~3x smaller, ~5x faster to decode) behind this enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SpillCodec {
    Json,
    /// Reserved for `data-plane`. Kernel never constructs this.
    Postcard,
}

impl SpillCodec {
    pub fn name(self) -> &'static str {
        match self {
            SpillCodec::Json => "json",
            SpillCodec::Postcard => "postcard",
        }
    }
}

/// When to spill. Engine decides; nodes never do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpillPolicy {
    /// Spill once the list holds more items than this.
    pub max_inline_items: u32,
    /// Spill once the estimated in-RAM size exceeds this.
    pub max_inline_bytes: u64,
    /// Binary payloads larger than this become `BinaryLocation::Ref`.
    pub max_inline_binary_bytes: u64,
}

impl Default for SpillPolicy {
    fn default() -> Self {
        Self {
            max_inline_items: 1_000,
            max_inline_bytes: 4 * 1024 * 1024, // 4 MB
            max_inline_binary_bytes: 256 * 1024, // 256 KB
        }
    }
}

impl ItemList {
    pub fn empty() -> Self {
        ItemList::Inline(Vec::new())
    }

    pub fn single(item: Item) -> Self {
        ItemList::Inline(vec![item])
    }

    /// Number of items. O(1) for both variants — never touches the store.
    pub fn len(&self) -> usize {
        match self {
            ItemList::Inline(v) => v.len(),
            ItemList::Spilled(s) => s.len as usize,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn is_spilled(&self) -> bool {
        matches!(self, ItemList::Spilled(_))
    }

    /// RAM actually held by this list right now.
    ///
    /// For `Spilled` this is metadata only (~64 bytes) — the items are on disk.
    /// This is the number the Memory Governor (D72) budgets against.
    pub fn ram_footprint(&self) -> usize {
        match self {
            ItemList::Inline(v) => v.iter().map(Item::estimated_bytes).sum(),
            ItemList::Spilled(s) => 64 + s.path.0.len(),
        }
    }

    /// Build a list from a `Vec`, spilling if the policy demands it.
    pub async fn from_vec(
        items: Vec<Item>,
        policy: &SpillPolicy,
        store: &dyn SpillStore,
    ) -> Result<Self, KernelError> {
        let bytes: usize = items.iter().map(Item::estimated_bytes).sum();
        let should_spill = items.len() as u32 > policy.max_inline_items || bytes as u64 > policy.max_inline_bytes;

        if !should_spill {
            return Ok(ItemList::Inline(items));
        }
        let handle = store.write(&items).await?;
        Ok(ItemList::Spilled(handle))
    }

    /// Borrow this list together with a store, giving ergonomic data access.
    ///
    /// Metadata methods above never need a store; anything that touches actual
    /// items does. Bundling both avoids threading `store` through every call.
    pub fn view<'a>(&'a self, store: &'a dyn SpillStore) -> ItemView<'a> {
        ItemView { list: self, store }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// ItemView — data access
// ─────────────────────────────────────────────────────────────────────────────

/// `ItemList` + the store needed to read it.
///
/// This is what node implementations actually receive (via `NodeContext`).
///
/// `Copy` because it is only two thin references; the cursor below holds one by
/// value and this keeps the ergonomics identical to passing a slice.
#[derive(Clone, Copy)]
pub struct ItemView<'a> {
    list: &'a ItemList,
    store: &'a dyn SpillStore,
}

/// Manual because `dyn SpillStore` is not `Debug`.
///
/// Prints the shape (length, spilled or not) rather than contents: dumping
/// items in a Debug impl is how large payloads end up in log files.
impl std::fmt::Debug for ItemView<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ItemView")
            .field("len", &self.list.len())
            .field("spilled", &self.list.is_spilled())
            .finish_non_exhaustive()
    }
}

impl<'a> ItemView<'a> {
    pub fn len(&self) -> usize {
        self.list.len()
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    pub fn is_spilled(&self) -> bool {
        self.list.is_spilled()
    }

    /// Materialize exactly one item into RAM.
    ///
    /// **This is what makes `$('Node').item` work on a spilled list** (A-01):
    /// the expression engine asks for item *i*, we read just that one.
    pub async fn get(&self, index: usize) -> Result<Item, KernelError> {
        match self.list {
            ItemList::Inline(v) => v
                .get(index)
                .cloned()
                .ok_or(KernelError::IndexOutOfBounds {
                    index,
                    len: v.len(),
                }),
            ItemList::Spilled(s) => {
                if index >= s.len as usize {
                    return Err(KernelError::IndexOutOfBounds {
                        index,
                        len: s.len as usize,
                    });
                }
                self.store.read_at(s, index).await
            }
        }
    }

    /// First item, matching n8n's `.item` shorthand.
    pub async fn first(&self) -> Result<Option<Item>, KernelError> {
        if self.is_empty() {
            Ok(None)
        } else {
            Ok(Some(self.get(0).await?))
        }
    }

    /// Load every item into RAM at once.
    ///
    /// Required by `BufferingMode::Batch` nodes (Sort, Merge, Aggregate...).
    /// The engine — not the node — must check with the Governor before calling
    /// this, and must handle `ResourceExhausted` (audit A-07).
    pub async fn materialize_all(&self) -> Result<Vec<Item>, KernelError> {
        match self.list {
            ItemList::Inline(v) => Ok(v.clone()),
            ItemList::Spilled(s) => self.store.read_all(s).await,
        }
    }

    /// Chunked cursor. The RAM-bounded way to walk a huge list.
    ///
    /// Rust has no stable async generators, so this is an explicit cursor
    /// rather than a `Stream`. It also avoids adding `futures` to the kernel.
    pub fn cursor(&self, chunk_size: usize) -> ItemCursor<'a> {
        ItemCursor {
            view: *self,
            pos: 0,
            chunk_size: chunk_size.max(1),
        }
    }
}

/// Walks an [`ItemView`] in bounded chunks.
///
/// Peak RAM is `chunk_size` items, regardless of list length. This is how a
/// 10-million-item list gets processed on a 2 GB VPS.
///
/// Deliberately **not** `Copy` even though every field is: `pos` advances as
/// the cursor is consumed, and a silently copyable cursor would let two loops
/// iterate the same list from diverging positions. Copying it must be explicit
/// (`cursor.clone()` is not derived either — construct a new cursor instead).
#[allow(missing_copy_implementations)]
#[derive(Debug)]
pub struct ItemCursor<'a> {
    view: ItemView<'a>,
    pos: usize,
    chunk_size: usize,
}

impl<'a> ItemCursor<'a> {
    /// Next chunk, or `None` when exhausted.
    pub async fn next_chunk(&mut self) -> Result<Option<Vec<Item>>, KernelError> {
        let total = self.view.len();
        if self.pos >= total {
            return Ok(None);
        }
        let end = (self.pos + self.chunk_size).min(total);
        let out = match self.view.list {
            ItemList::Inline(v) => v[self.pos..end].to_vec(),
            ItemList::Spilled(s) => {
                self.view
                    .store
                    .read_range(s, self.pos, end - self.pos)
                    .await?
            }
        };
        self.pos = end;
        Ok(Some(out))
    }

    pub fn position(&self) -> usize {
        self.pos
    }

    pub fn is_finished(&self) -> bool {
        self.pos >= self.view.len()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// SpillStore — the contract
// ─────────────────────────────────────────────────────────────────────────────

/// Backing store for spilled item lists.
///
/// Implemented by `crates/data-plane` (disk-backed, with the offset index and
/// optional mmap). The kernel only defines the contract, which keeps `kernel`
/// free of filesystem and mmap dependencies.
#[async_trait]
pub trait SpillStore: Send + Sync {
    /// Persist items and return a handle describing them.
    async fn write(&self, items: &[Item]) -> Result<SpilledList, KernelError>;

    /// Read one item by index. Must be O(1)-ish (indexed offset lookup).
    async fn read_at(&self, handle: &SpilledList, index: usize) -> Result<Item, KernelError>;

    /// Read `len` items starting at `start`.
    async fn read_range(
        &self,
        handle: &SpilledList,
        start: usize,
        len: usize,
    ) -> Result<Vec<Item>, KernelError>;

    /// Read everything. Callers must have Governor approval first.
    async fn read_all(&self, handle: &SpilledList) -> Result<Vec<Item>, KernelError>;

    /// Delete the backing file (D39 lifecycle / GC).
    ///
    /// IMPORTANT (audit A-21): the GC must never call this on a handle still
    /// referenced by a non-terminal execution (RUNNING / WAITING / PAUSED).
    async fn delete(&self, handle: &SpilledList) -> Result<(), KernelError>;

    /// Bytes this handle occupies on disk.
    fn disk_footprint(&self, handle: &SpilledList) -> u64;

    /// Open an incremental writer.
    ///
    /// # Why this exists
    ///
    /// `write()` takes a fully materialized `&[Item]`, which means the caller
    /// held every item in RAM at once — so spilling saved nothing at the moment
    /// of peak pressure. That is exactly backwards.
    ///
    /// Real triggers emit items over time: a Postgres node streams rows from a
    /// cursor, a webhook receives a large body in chunks, `SplitInBatches`
    /// produces output incrementally. A writer lets those spill **as they
    /// arrive**, so peak RAM is one batch rather than the whole result set.
    ///
    /// This is what makes the 2 GB VPS claim (D3) actually true under load
    /// rather than only at rest.
    async fn writer(&self) -> Result<Box<dyn SpillWriter>, KernelError>;
}

/// Incremental spill writer. See [`SpillStore::writer`].
///
/// # Contract
///
/// * `push` may be called any number of times, including zero.
/// * `finish` consumes the writer and returns the handle. Calling `push` after
///   `finish` is impossible by construction (the writer is consumed).
/// * Dropping a writer without calling `finish` MUST release its partial file —
///   an aborted execution should not leak spill files (audit A-21).
/// * The resulting handle must satisfy the same invariants as `write()`:
///   `read_at(i)` returns the i-th pushed item, in push order, across all
///   batches. Indexing is global, not per-batch.
#[async_trait]
pub trait SpillWriter: Send {
    /// Append a batch. Batches are an I/O efficiency hint, not a semantic
    /// boundary — readers see one flat list.
    async fn push(&mut self, items: &[Item]) -> Result<(), KernelError>;

    /// Flush and produce the handle.
    async fn finish(self: Box<Self>) -> Result<SpilledList, KernelError>;

    /// Items pushed so far. Lets the Governor observe growth mid-write (D72).
    fn pushed(&self) -> u32;
}

/// Build an [`ItemList`] from any iterator, spilling incrementally.
///
/// Peak RAM is `batch` items regardless of how many the iterator yields, so a
/// lazy iterator — `(0..10_000_000).map(make_row)`, a database row cursor —
/// never fully materializes. This is the path real triggers should use.
///
/// An empty iterator yields [`ItemList::empty`] and leaves no file behind.
pub async fn spill_from_iter(
    batch: usize,
    store: &dyn SpillStore,
    items: impl IntoIterator<Item = Item>,
) -> Result<ItemList, KernelError> {
    let batch = batch.max(1);
    let mut w = store.writer().await?;
    let mut buf: Vec<Item> = Vec::with_capacity(batch.min(4096));

    for item in items {
        buf.push(item);
        if buf.len() >= batch {
            w.push(&buf).await?;
            buf.clear();
        }
    }
    if !buf.is_empty() {
        w.push(&buf).await?;
    }

    let handle = w.finish().await?;
    if handle.len == 0 {
        // Nothing was written; do not leak an empty spill file (audit A-21).
        store.delete(&handle).await?;
        return Ok(ItemList::empty());
    }
    Ok(ItemList::Spilled(handle))
}
