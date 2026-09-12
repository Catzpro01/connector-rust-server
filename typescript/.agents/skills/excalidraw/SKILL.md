---
name: excalidraw
description: Tier A - Tool terbaik untuk sketsa diagram arsitektur sistem, alur node/workflow, wireframe cepat gaya hand-drawn profesional. Aktivasi saat butuh diagram workflow, arsitektur, atau wireframe n8n canvas.
---

# excalidraw — Tier A Diagram

## Fungsi
Untuk diagram workflow n8n: node sebagai rectangle, edge sebagai arrow hand-drawn.

## Implementasi di n8n-rust
- Canvas edges menggunakan bezier curve seperti excalidraw arrow
- Path: M x1 y1 C x1+dx y1, x2-dx y2, x2 y2
- Arrow marker dengan SVG marker
- Minimap sebagai excalidraw-like overview

## Checklist
- [ ] Edge path bezier smooth, dx = max(80, |x2-x1|/2)
- [ ] Arrow marker #c8c8c8, selected #ff6d5a
- [ ] Minimap overview dengan scale dan viewport rect
