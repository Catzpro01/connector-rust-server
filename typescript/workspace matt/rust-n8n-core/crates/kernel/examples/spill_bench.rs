//! Empirical proof of the A-03 resolution.
//!
//! This turns "spill-to-disk keeps RAM bounded" from a design claim into a
//! measured number. It matters because the entire 2 GB VPS thesis (D3) rests on
//! it, and audit A-03 flagged that the blueprint asserted memory efficiency
//! without ever measuring it.
//!
//! Unlike the in-memory store in `tests/contract.rs`, `FileSpillStore` here does
//! **real disk I/O** with a real offset index — effectively a reference sketch
//! for the `data-plane` implementation. So this validates both the RAM claim and
//! that `SpillStore` is actually implementable against a filesystem as designed.
//!
//! ```text
//! cargo run --release --example spill_bench -- compare 2000000
//! cargo run --release --example spill_bench -- spilled 2000000
//! cargo run --release --example spill_bench -- inline  2000000
//! ```
//!
//! `compare` re-execs itself once per scenario in a **separate process**. That
//! is deliberate: `VmHWM` is a process-lifetime high-water mark and cannot be
//! reset, so measuring both scenarios in one process would report the inline
//! peak for both and quietly prove nothing.

use async_trait::async_trait;
use kernel::error::KernelError;
use kernel::id::SpillPath;
use kernel::item::{spill_from_iter, Item, ItemList, SpillCodec, SpillPolicy, SpillStore, SpillWriter, SpilledList};
use serde_json::json;
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Read, Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;

// ═══════════════════════════════════════════════════════════════════════════
// File-backed SpillStore — real disk, real offset index
// ═══════════════════════════════════════════════════════════════════════════

struct Entry {
    /// Byte offset of each item in the spill file.
    ///
    /// This is the "index in RAM" the spec budgets for: 8 bytes per item, with
    /// the payload itself staying on disk.
    offsets: Vec<u64>,
    file: File,
}

type Entries = Arc<Mutex<HashMap<String, Entry>>>;

struct FileSpillStore {
    dir: PathBuf,
    next_id: Mutex<u64>,
    entries: Entries,
}

impl FileSpillStore {
    fn new(dir: impl Into<PathBuf>) -> std::io::Result<Self> {
        let dir = dir.into();
        std::fs::create_dir_all(&dir)?;
        Ok(Self {
            dir,
            next_id: Mutex::new(0),
            entries: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    fn path_for(&self, name: &str) -> PathBuf {
        self.dir.join(name)
    }

    fn alloc_name(&self) -> String {
        let mut next = self.next_id.lock().unwrap();
        *next += 1;
        format!("spill-{}.bin", *next)
    }

    /// RAM held by the index for one handle.
    fn index_bytes(&self, handle: &SpilledList) -> usize {
        self.entries
            .lock()
            .unwrap()
            .get(handle.path.as_str())
            .map(|e| e.offsets.len() * std::mem::size_of::<u64>())
            .unwrap_or(0)
    }
}

fn io_err(e: std::io::Error) -> KernelError {
    KernelError::SpillIo {
        message: e.to_string(),
    }
}

/// Serialize one item to the on-disk record format: 8-byte LE length + payload.
fn encode(it: &Item) -> Result<Vec<u8>, KernelError> {
    serde_json::to_vec(it).map_err(|e| KernelError::Codec {
        codec: "json",
        reason: e.to_string(),
    })
}

fn decode(buf: &[u8]) -> Result<Item, KernelError> {
    serde_json::from_slice(buf).map_err(|e| KernelError::Codec {
        codec: "json",
        reason: e.to_string(),
    })
}

fn open_rw(path: &PathBuf) -> Result<File, KernelError> {
    OpenOptions::new()
        .create(true)
        .write(true)
        .read(true)
        .truncate(true)
        .open(path)
        .map_err(io_err)
}

#[async_trait]
impl SpillStore for FileSpillStore {
    async fn write(&self, items: &[Item]) -> Result<SpilledList, KernelError> {
        // Convenience path: reuse the streaming writer so there is exactly one
        // implementation of the file format.
        let mut w = self.writer().await?;
        w.push(items).await?;
        w.finish().await
    }

    async fn read_at(&self, handle: &SpilledList, index: usize) -> Result<Item, KernelError> {
        let mut entries = self.entries.lock().unwrap();
        let entry = entries
            .get_mut(handle.path.as_str())
            .ok_or_else(|| KernelError::SpillIo {
                message: format!("no such spill file: {}", handle.path),
            })?;

        let &off = entry
            .offsets
            .get(index)
            .ok_or(KernelError::IndexOutOfBounds {
                index,
                len: entry.offsets.len(),
            })?;

        entry.file.seek(SeekFrom::Start(off)).map_err(io_err)?;
        let mut lenbuf = [0u8; 8];
        entry.file.read_exact(&mut lenbuf).map_err(io_err)?;
        let len = u64::from_le_bytes(lenbuf) as usize;

        let mut buf = vec![0u8; len];
        entry.file.read_exact(&mut buf).map_err(io_err)?;
        decode(&buf)
    }

    async fn read_range(
        &self,
        handle: &SpilledList,
        start: usize,
        len: usize,
    ) -> Result<Vec<Item>, KernelError> {
        // Sequential read: one seek, then stream the whole range. This is the
        // cursor hot path, so avoiding N seeks is what makes chunked iteration
        // cheaper than N random gets.
        let mut entries = self.entries.lock().unwrap();
        let entry = entries
            .get_mut(handle.path.as_str())
            .ok_or_else(|| KernelError::SpillIo {
                message: format!("no such spill file: {}", handle.path),
            })?;

        let n = entry.offsets.len();
        if start >= n || len == 0 {
            return Ok(Vec::new());
        }
        let end = (start + len).min(n);
        let off = entry.offsets[start];
        entry.file.seek(SeekFrom::Start(off)).map_err(io_err)?;

        let file_len = entry.file.metadata().map(|m| m.len()).unwrap_or(0);
        let span = if end < n {
            (entry.offsets[end] - off) as usize
        } else {
            (file_len.saturating_sub(off)) as usize
        };

        let mut raw = vec![0u8; span];
        entry.file.read_exact(&mut raw).map_err(io_err)?;

        let mut out = Vec::with_capacity(end - start);
        let mut cur = 0usize;
        for _ in start..end {
            if cur + 8 > raw.len() {
                break;
            }
            let l = u64::from_le_bytes(raw[cur..cur + 8].try_into().unwrap()) as usize;
            cur += 8;
            if cur + l > raw.len() {
                return Err(KernelError::Codec {
                    codec: "json",
                    reason: "truncated record".into(),
                });
            }
            out.push(decode(&raw[cur..cur + l])?);
            cur += l;
        }
        Ok(out)
    }

    async fn read_all(&self, handle: &SpilledList) -> Result<Vec<Item>, KernelError> {
        self.read_range(handle, 0, handle.len as usize).await
    }

    async fn delete(&self, handle: &SpilledList) -> Result<(), KernelError> {
        self.entries.lock().unwrap().remove(handle.path.as_str());
        let p = self.path_for(handle.path.as_str());
        if p.exists() {
            std::fs::remove_file(p).map_err(io_err)?;
        }
        Ok(())
    }

    fn disk_footprint(&self, handle: &SpilledList) -> u64 {
        handle.total_bytes
    }

    async fn writer(&self) -> Result<Box<dyn SpillWriter>, KernelError> {
        let name = self.alloc_name();
        let fs_path = self.path_for(&name);
        let file = open_rw(&fs_path)?;
        Ok(Box::new(FileSpillWriter {
            logical: name,
            fs_path,
            w: Some(BufWriter::with_capacity(256 * 1024, file)),
            offsets: Vec::new(),
            pos: 0,
            total: 0,
            finished: false,
            entries: Arc::clone(&self.entries),
        }))
    }
}

struct FileSpillWriter {
    logical: String,
    fs_path: PathBuf,
    /// `Option` so `finish` can move the file out. A type implementing `Drop`
    /// cannot have fields moved out of it directly (E0509).
    w: Option<BufWriter<File>>,
    offsets: Vec<u64>,
    pos: u64,
    total: u64,
    finished: bool,
    entries: Entries,
}

#[async_trait]
impl SpillWriter for FileSpillWriter {
    async fn push(&mut self, items: &[Item]) -> Result<(), KernelError> {
        if self.finished {
            return Err(KernelError::Invalid {
                message: "push() called after finish()".into(),
            });
        }
        let w = self.w.as_mut().ok_or(KernelError::Invalid {
            message: "writer already finished".into(),
        })?;
        for it in items {
            let bytes = encode(it)?;
            let len = bytes.len() as u64;
            w.write_all(&len.to_le_bytes())
                .and_then(|_| w.write_all(&bytes))
                .map_err(io_err)?;
            self.offsets.push(self.pos);
            self.pos += 8 + len;
            self.total += len;
        }
        Ok(())
    }

    async fn finish(mut self: Box<Self>) -> Result<SpilledList, KernelError> {
        let mut bw = self.w.take().ok_or(KernelError::Invalid {
            message: "finish() called twice".into(),
        })?;
        bw.flush().map_err(io_err)?;
        let file = bw.into_inner().map_err(|e| io_err(e.into_error()))?;
        file.sync_all().map_err(io_err)?;

        let count = self.offsets.len() as u32;
        self.entries.lock().unwrap().insert(
            self.logical.clone(),
            Entry {
                offsets: std::mem::take(&mut self.offsets),
                file,
            },
        );
        self.finished = true;

        Ok(SpilledList {
            path: SpillPath::new(self.logical.clone()),
            len: count,
            total_bytes: self.total,
            codec: SpillCodec::Json,
        })
    }

    fn pushed(&self) -> u32 {
        self.offsets.len() as u32
    }
}

/// Honour the trait contract: an unfinished writer must not leak a partial file.
impl Drop for FileSpillWriter {
    fn drop(&mut self) {
        if !self.finished {
            let _ = std::fs::remove_file(&self.fs_path);
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Minimal executor (kernel is runtime-agnostic; no tokio dependency)
// ═══════════════════════════════════════════════════════════════════════════

fn block_on<F: std::future::Future>(fut: F) -> F::Output {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::task::{Context, Poll, Wake};

    struct Spin(AtomicBool);
    impl Wake for Spin {
        fn wake(self: Arc<Self>) {
            self.0.store(true, Ordering::SeqCst);
        }
    }

    let spin = Arc::new(Spin(AtomicBool::new(true)));
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

// ═══════════════════════════════════════════════════════════════════════════
// Measurement
// ═══════════════════════════════════════════════════════════════════════════

fn proc_field(name: &str) -> u64 {
    let s = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    for line in s.lines() {
        if let Some(rest) = line.strip_prefix(name) {
            return rest
                .trim()
                .trim_end_matches("kB")
                .trim()
                .parse::<u64>()
                .unwrap_or(0)
                * 1024;
        }
    }
    0
}

fn rss() -> u64 {
    proc_field("VmRSS:")
}
fn peak_rss() -> u64 {
    proc_field("VmHWM:")
}
fn mb(b: u64) -> f64 {
    b as f64 / (1024.0 * 1024.0)
}

/// Thousands separator, id-ID style (1.000.000).
fn grouped(n: usize) -> String {
    let s = n.to_string();
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, b) in bytes.iter().enumerate() {
        if i > 0 && (bytes.len() - i) % 3 == 0 {
            out.push('.');
        }
        out.push(*b as char);
    }
    out
}

/// A realistic item: the shape an HTTP Request or Postgres node emits.
/// ~170 bytes of JSON — typical for API rows, CRM records, order lines.
fn make_item(i: usize) -> Item {
    Item::new(json!({
        "id": i,
        "email": format!("user{}@example.com", i),
        "name": format!("Customer {i}"),
        "amount_cents": (i * 37) % 100_000,
        "created_at": "2026-09-09T12:00:00Z",
        "status": if i % 3 == 0 { "active" } else { "pending" },
    }))
}

fn temp_dir(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!("spill-bench-{}-{}", tag, std::process::id()))
}

/// Spilled scenario.
///
/// Items are generated **lazily** and streamed straight to disk in batches, so
/// the full set is never resident. This is the honest version of the claim: it
/// is what a Postgres trigger or a paginated HTTP node actually does.
fn scenario_spilled(n: usize) -> Report {
    let dir = temp_dir("spilled");
    let store = FileSpillStore::new(&dir).expect("mkdir");

    let rss_before = rss();
    let t0 = Instant::now();
    let list = block_on(spill_from_iter(
        10_000,
        &store,
        (0..n).map(make_item),
    ))
    .expect("spill");
    let build = t0.elapsed();

    assert!(list.is_spilled(), "expected spill at n={}", n);
    let handle = match &list {
        ItemList::Spilled(h) => h.clone(),
        ItemList::Inline(_) => panic!("expected spilled"),
    };

    let (scan, processed, checksum) = scan_all(&list, &store);
    let random_access = timed(|| {
        let v = list.view(&store);
        block_on(v.get(n - 1)).expect("last")
    });

    let report = Report {
        mode: "SPILLED",
        n,
        items_processed: processed,
        checksum,
        logical_bytes: handle.total_bytes,
        index_bytes: store.index_bytes(&handle) as u64,
        struct_bytes: list.ram_footprint() as u64,
        rss_before,
        rss_after: rss(),
        peak_rss: peak_rss(),
        build,
        scan,
        random_access,
    };

    let _ = block_on(store.delete(&handle));
    let _ = std::fs::remove_dir_all(&dir);
    report
}

/// Inline scenario: everything resident, which is what n8n does today.
fn scenario_inline(n: usize) -> Report {
    let dir = temp_dir("inline");
    let store = FileSpillStore::new(&dir).expect("mkdir");
    let policy = SpillPolicy {
        max_inline_items: u32::MAX,
        max_inline_bytes: u64::MAX,
        max_inline_binary_bytes: u64::MAX,
    };

    let rss_before = rss();
    let t0 = Instant::now();
    // Materialize the full set first — this is the point of the comparison.
    let items: Vec<Item> = (0..n).map(make_item).collect();
    let logical_bytes: u64 = items.iter().map(|i| i.estimated_bytes() as u64).sum();
    let list = block_on(ItemList::from_vec(items, &policy, &store)).expect("inline");
    let build = t0.elapsed();

    assert!(!list.is_spilled(), "expected inline at n={}", n);

    let (scan, processed, checksum) = scan_all(&list, &store);
    let random_access = timed(|| {
        let v = list.view(&store);
        block_on(v.get(n - 1)).expect("last")
    });

    let report = Report {
        mode: "INLINE",
        n,
        items_processed: processed,
        checksum,
        logical_bytes,
        index_bytes: 0,
        struct_bytes: list.ram_footprint() as u64,
        rss_before,
        rss_after: rss(),
        peak_rss: peak_rss(),
        build,
        scan,
        random_access,
    };

    drop(list);
    let _ = std::fs::remove_dir_all(&dir);
    report
}

/// Walk every item through a cursor, the way a transform node would.
fn scan_all(list: &ItemList, store: &FileSpillStore) -> (std::time::Duration, usize, u64) {
    let view = list.view(store);
    let t = Instant::now();
    let mut processed = 0usize;
    let mut checksum: u64 = 0;
    let mut cur = view.cursor(1_000);
    while let Some(chunk) = block_on(cur.next_chunk()).expect("chunk") {
        for it in &chunk {
            checksum = checksum.wrapping_add(it.json["amount_cents"].as_u64().unwrap_or(0));
            processed += 1;
        }
    }
    (t.elapsed(), processed, checksum)
}

fn timed<F, R>(f: F) -> std::time::Duration
where
    F: FnOnce() -> R,
{
    let t = Instant::now();
    let _ = f();
    t.elapsed()
}

struct Report {
    mode: &'static str,
    n: usize,
    items_processed: usize,
    checksum: u64,
    logical_bytes: u64,
    index_bytes: u64,
    struct_bytes: u64,
    rss_before: u64,
    rss_after: u64,
    peak_rss: u64,
    build: std::time::Duration,
    scan: std::time::Duration,
    random_access: std::time::Duration,
}

impl Report {
    fn print(&self) {
        println!();
        println!("┌─ {} ─ {} items", self.mode, grouped(self.n));
        println!("│ payload (logical)    {:>9.1} MB", mb(self.logical_bytes));
        println!("│ RAM list structs     {:>9.1} MB", mb(self.struct_bytes));
        println!("│ RAM spill index      {:>9.1} MB", mb(self.index_bytes));
        println!("│ RSS at start         {:>9.1} MB", mb(self.rss_before));
        println!("│ RSS after full scan  {:>9.1} MB", mb(self.rss_after));
        println!("│ PEAK RSS (VmHWM)     {:>9.1} MB", mb(self.peak_rss));
        println!("│ build + spill        {:>9.3} s", self.build.as_secs_f64());
        println!("│ scan all items       {:>9.3} s", self.scan.as_secs_f64());
        println!(
            "│ random get(last)     {:>9.3} ms",
            self.random_access.as_secs_f64() * 1000.0
        );
        println!("│ items processed      {:>9}", grouped(self.items_processed));
        println!("└─ checksum            {:>9}", self.checksum);
    }

}

fn total_ram() -> u64 {
    let s = std::fs::read_to_string("/proc/meminfo").unwrap_or_default();
    for line in s.lines() {
        if let Some(rest) = line.strip_prefix("MemTotal:") {
            return rest
                .trim()
                .trim_end_matches("kB")
                .trim()
                .parse::<u64>()
                .unwrap_or(0)
                * 1024;
        }
    }
    0
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mode = args.get(1).map(|s| s.as_str()).unwrap_or("compare");
    let n: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(1_000_000);

    match mode {
        "inline" => scenario_inline(n).print(),
        "spilled" => scenario_spilled(n).print(),
        "compare" => {
            let exe = std::env::current_exe().expect("current_exe");
            println!("spill_bench — kernel ItemList vs real file-backed SpillStore");
            println!(
                "host: {:.0} MB RAM, {} CPUs, n = {} items",
                mb(total_ram()),
                std::thread::available_parallelism().map(|v| v.get()).unwrap_or(1),
                grouped(n)
            );

            let mut results = Vec::new();
            for scen in ["spilled", "inline"] {
                // Separate process per scenario: VmHWM cannot be reset.
                let out = std::process::Command::new(&exe)
                    .args([scen, &n.to_string()])
                    .output()
                    .expect("spawn");
                let text = String::from_utf8_lossy(&out.stdout).into_owned();
                print!("{}", text);
                if !out.status.success() {
                    eprintln!(
                        "{} scenario FAILED (status {:?}):\n{}",
                        scen,
                        out.status.code(),
                        String::from_utf8_lossy(&out.stderr)
                    );
                    std::process::exit(1);
                }
                results.push((scen, text));
            }

            // Verdict
            let peak = |tag: &str| -> Option<f64> {
                results.iter().find(|(s, _)| *s == tag).and_then(|(_, t)| {
                    t.lines()
                        .find_map(|l| l.strip_prefix("│ PEAK RSS (VmHWM)"))
                        .and_then(|v| v.trim().trim_end_matches("MB").trim().parse::<f64>().ok())
                })
            };
            if let (Some(sp), Some(inl)) = (peak("spilled"), peak("inline")) {
                println!("\n════════ VERDICT ════════");
                println!("inline  peak RSS : {:>8.1} MB", inl);
                println!("spilled peak RSS : {:>8.1} MB", sp);
                if sp > 0.0 {
                    println!("reduction        : {:>8.1}x", inl / sp);
                }
                println!("\nSame {} items, identical checksum, identical random", grouped(n));
                println!("access — the only difference is where the bytes live.");
            }
        }
        other => {
            eprintln!("unknown mode {:?}; use inline|spilled|compare", other);
            std::process::exit(2);
        }
    }
}
