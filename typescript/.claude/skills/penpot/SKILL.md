---
name: penpot
description: Tier S - Alternatif open-source Figma. Berbasis web native (SVG, CSS Flexbox, Grid), design tokens, self-hostable. Aktivasi saat butuh design system berbasis SVG, tokens, atau layout flex/grid mirip n8n asli.
---

# penpot — Tier S Open Design

## Prinsip
Penpot menggunakan standar web native: SVG, Flexbox, Grid. Tidak ada vendor lock-in.

## Design Tokens untuk n8n-rust
Gunakan format penpot tokens:
- Colors: --n8n-bg, --n8n-primary, --n8n-line
- Typography: Inter 400/500/600/700, 12-15px
- Spacing: 4,8,12,16,24,32
- Radius: 6px (button/input), 8px (card/node)
- Shadow: 0 2px 6px rgba(0,0,0,0.08)

## Layout
- Header: flex row, height 60px, border-bottom
- Layout: flex row, sidebar 320px, canvas flex:1, right 420px
- Canvas: absolute viewport with transform translate+scale
- Nodes: absolute positioned, width 240px

## Checklist
- [ ] Semua layout pakai flexbox, bukan absolute hack
- [ ] SVG untuk edges dengan marker arrow
- [ ] Design tokens di :root
