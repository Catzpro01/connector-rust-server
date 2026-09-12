# Map: n8n-rust-engine

## Destination
Core engine workflow n8n versi Rust (eksekusi node, parser trigger, evaluasi workflow JSON, graph execution) yang terintegrasi penuh dengan hybrid build pipeline VPS (`connector-cli`) hingga dapat menjalankan workflow nyata n8n secara lokal maupun remote.

## Notes
- **Domain**: Rust workflow orchestration, DAG graph execution, n8n JSON compatibility.
- **Skills**: `grilling`, `domain-modeling`, `tdd`, `executing-plans`.
- **Standing Preferences**:
  - Auto-Hybrid execution via `connector-cli tab`.
  - Zero blocking wait; gunakan passive sentinel check.
  - Strict Rust type safety, idiomatic `Result<T, E>`, zero runtime `unwrap()` di jalur eksekusi production.

## Decisions so far
<!-- the index: one line per closed ticket, enough to judge relevance -->

## Not yet specified
<!-- Fog of war: in-scope areas to graduate later as the frontier advances -->
- **Expression Engine Binding**: Keputusan final runtime evaluasi sintaks `{{ $json.field }}` (QuickJS C-binding vs pure Rust interpreter).
- **Trigger Webhook Server & Async Polling**: Arsitektur listener webhook Axum dan scheduler polling cron untuk node trigger.
- **State Snapshot Persistence**: Database layer (SQLite / Postgres) untuk menyimpan log riwayat eksekusi (`execution_entity`).
- **Dynamic Node Plugin System**: Sandboxing dan sistem dynamic loading untuk node pihak ketiga (WASM component model).

## Out of scope
- **Full Canvas Frontend Replication**: Kode editor visual n8n (Vue/React web canvas) tidak dikerjakan di sini; fokus 100% pada backend execution engine, runtime daemon, dan CLI.
