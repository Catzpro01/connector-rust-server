# n8n-rust v1.1 — Premium Quality

> Port Rust dari [n8n](https://n8n.io): sefleksibel mungkin, ringan/cepat/efisien, tampilan hampir sama seperti n8n asli — sekarang dengan kualitas premium.

[![Rust](https://img.shields.io/badge/rust-1.75%2B-orange?style=flat-square)](https://www.rust-lang.org)
[![Version](https://img.shields.io/badge/version-v1.1.0-blue?style=flat-square)](n8n-rust/Cargo.toml)
[![License](https://img.shields.io/badge/license-MIT-green?style=flat-square)](LICENSE)
[![Nodes](https://img.shields.io/badge/nodes-19%20types-purple?style=flat-square)](#node-yang-didukung)

- **Kode v1.1**: [`n8n-rust/`](n8n-rust/) (workspace Cargo: core/engine/nodes/cli/server + UI premium di `crates/n8n-server/ui/`)
- **UI Premium**: Single-file modern (Geist font, dark/light, canvas infinite, minimap, command palette ⌘K)
- **Preview**: `node preview-server.js` → http://localhost:3000
- **Skills agent**: `.claude/skills/` + `.agents/skills/` (73 + 73)
- **Riwayat**: tag [`pre-v1-wipe`](https://github.com/Catzpro01/n8n-rust-v1.0/tree/pre-v1-wipe)

## 🎨 Preview Kualitas Baru

UI v1.1 upgrade total:
- **Design**: Dark mode premium, Geist font, shadow, blur, gradient, badge, toast, statusbar.
- **Canvas**: Infinite pan/zoom, grid dot, node card 260px rounded 14px, edge bezier + arrow + dash animation, minimap.
- **Palette**: Search + tabs (All/Trigger/Logic/Transform), 19 node dengan icon warna.
- **Inspector**: Edit name, disable, duplicate, delete, parameters JSON, output per cabang, validate, explain, run history.
- **UX**: Command palette ⌘K, shortcuts ⌘⏎ run, ⌘Z undo, Del delete, Esc clear, localStorage auto-save.
- **Backend**: Health `/health`, metrics `/api/metrics`, openapi `/api/openapi.json`, CORS, security headers, request-id, graceful shutdown, structured logging.

Lihat langsung:
```bash
cd n8n-rust-v1.0
node preview-server.js
# buka http://localhost:3000 — LIVE PREVIEW
```

## 🚀 Mulai Cepat

```bash
# Rust (jika ada toolchain)
cargo build --release
./target/release/n8n-cli validate n8n-rust/fixtures/manual-to-set.json
./target/release/n8n-cli run n8n-rust/fixtures/manual-to-set.json --save report.json -v
./target/release/n8n-server   # http://127.0.0.1:3000

# Tanpa Rust — preview Node.js (mock API)
node preview-server.js
```

## 📦 Struktur

```text
n8n-rust/
  crates/
    n8n-core/    Workflow model + expr {{ }} + topo + validation (premium docs)
    n8n-engine/  Executor sinkron + planner + linter + metrics total_ms
    n8n-nodes/   19 node: set/noOp/filter/sort/limit/if/http/code/schedule/webhook/switch/merge/dateTime/respond/wait/stop/exec
    n8n-cli/     CLI premium: colored output, version, help, verbose
    n8n-server/  Axum server premium: health/metrics/openapi + UI embedded + hooks
  fixtures/manual-to-set.json
  web/README.md  Dokumentasi UI premium
preview-server.js  Node.js preview server (mock API) untuk demo tanpa cargo
Dockerfile + docker-compose.yml
```

## 🧩 Node yang didukung (19)

| Tipe | Deskripsi |
|------|-----------|
| manualTrigger | emit [{}] |
| scheduleTrigger | emit field UTC + echo rule |
| webhook | payload server, httpMethod/responseMode |
| set | manual/raw, include all/none/selected/except, dotNotation |
| noOp | passthrough |
| filter | conditions object + legacy string |
| sort | simple/random/code, multi-field case-insensitive |
| limit | maxItems + keep first/last |
| if | conditions → [true,false] |
| httpRequest | fan-out per item, timeout, responseFormat |
| code/function | rhai, runOnceForAllItems/runOnceForEachItem |
| switch | N cabang + else |
| merge | append/choose/combine |
| dateTime | 7 operasi |
| respondToWebhook | passthrough, server baca params |
| wait | sleep amount×unit |
| stopAndError | selalu gagal |
| executeCommand | sh -c shell |

## 🔌 API Server v1.1

| Method | Path | Fungsi |
|--------|------|--------|
| GET / | UI premium |
| GET /health | health check |
| GET /api/nodes | list node types |
| POST /api/validate | lint workflow |
| POST /api/explain | topo order |
| POST /api/run | run workflow |
| GET /api/runs?limit=50 | run history |
| GET /api/metrics | metrics |
| GET /api/openapi.json | OpenAPI |
| POST /api/hooks | register hook |
| GET /api/hooks | list hooks |
| DELETE /api/hooks/:path | delete hook |
| GET/POST/PUT/PATCH/DELETE /hook/:path | fire hook |

## 🛠 Quality Metrics

- **Tests**: 84+ (core 3, engine 7, nodes 41+filter 8+merge 8+datetime 7)
- **Lint**: `cargo test` hijau, `cargo clippy` (future)
- **UI**: Single-file 1500+ lines premium, no build, no CDN, Geist font, dark/light, command palette, minimap, toast, undo/redo.
- **Server**: Graceful shutdown, security headers, CORS, request-id, structured log, config via env PORT/HOST.

## 📸 Screenshots (deskripsi)

- Canvas infinite dengan node card warna-warni, edge bezier, badge urutan.
- Command palette ⌘K dengan search node + actions.
- Inspector kanan dengan parameters JSON + output cabang.
- Statusbar dengan nodes/edges/zoom + toast.

## 📄 License

MIT — personal project, dipakai sendiri, tanpa telemetri, tanpa DB, tanpa auth multi-user.

## 🙏 Credits

- Original n8n: https://n8n.io
- Rust crates: axum, tokio, serde, rhai, reqwest, chrono
- Font: Geist by Vercel
