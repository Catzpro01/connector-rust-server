# UI web n8n-rust v1.1 — Premium Edition

Editor workflow premium single-file (`crates/n8n-server/ui/app.html`) disajikan embedded oleh binary server di `GET /`. Tanpa bundler, tanpa CDN — buka browser langsung pakai, tapi rasa aplikasi modern kelas dunia.

## ✨ Quality Upgrade v1.1

### Design System
- **Theme**: Dark (default) + Light, toggle persist, CSS variables, Geist font, shadow & blur.
- **Layout**: Header 52px, left sidebar 300px (node palette), center canvas infinite, right inspector 380px, statusbar.
- **Visual**: Gradient logo, badge, dot pulse, glassmorphism, animations pop/slide/dash.

### Canvas Premium
- Infinite canvas dengan pan (drag background / middle mouse) + zoom (wheel / buttons) + fit view / center.
- Grid radial dot, viewport transform `translate + scale`, minimap 160×110 dengan viewport rect.
- Node sebagai div (bukan SVG) — rounded 14px, shadow, hover lift, selected border acc, running blue, success/error.
- Edge SVG bezier, marker arrow, hover highlight, true/false style, running dash animation, click to delete.
- Drag node dengan pointer capture, auto history (undo/redo), clipboard duplicate.

### Node Palette
- Search + tabs (Semua/Trigger/Logic/Transform) + kategori.
- 19 node dengan icon, warna, deskripsi, draggable + click to add.
- Auto naming unik, random jitter posisi.

### Inspector
- Panel kanan: Node detail (editable name, type, version, position, disable/duplicate/delete).
- Parameters editor JSON textarea dengan save + format.
- Output per cabang dengan label true/false untuk If.
- Workflow info, shortcuts, Validate diags, Explain plan, Last run.

### UX Premium
- Command palette (⌘K) — fuzzy search node + actions (Run, Validate, Explain, Export, Fit, Contoh).
- Toast system, status bar dengan stats nodes/edges/zoom.
- LocalStorage auto-save workflow, theme persist, history 50 steps.
- Keyboard: ⌘K palette, ⌘⏎ run, ⌘Z/⇧⌘Z undo/redo, Del hapus, Esc clear.
- Mock API fallback — jika backend Rust tidak ada, UI tetap jalan dengan mock topo/validate/run.

### API yang dipakai
`POST /api/validate`, `/api/explain`, `/api/run`, `GET /api/nodes`, `/api/runs`, `/api/metrics`, `/health`, `/api/openapi.json`, `/api/hooks`, `/hook/:path`.

### Cara lihat hasil
```bash
PORT=3000 node preview-server.js
# buka http://localhost:3000
```

Atau build Rust:
```bash
cargo run -p n8n-server
```

### Batasan (disengaja)
- Canvas tetap DOM + SVG, tanpa WebGL — ringan.
- Editor parameters masih JSON raw (form per node di roadmap v1.2).
- Tanpa collaboration / undo visual graph (history linear).
