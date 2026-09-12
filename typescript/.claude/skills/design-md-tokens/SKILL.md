---
name: design-md-tokens
description: Tier A - Mengelola token desain (warna, tipografi, radius, spacing) dalam file Markdown yang mudah dibaca AI dan diaplikasikan ke Tailwind/CSS variable. Aktivasi saat butuh design tokens konsisten mirip n8n asli.
---

# design-md-tokens — Tier A

## Format
```md
## Colors
- --n8n-bg: #ffffff
- --n8n-primary: #ff6d5a
- --n8n-line: #e8e8e8

## Typography
- font: Inter, 400/500/600
- size: 11px uppercase, 12px, 13px, 15px

## Spacing
- 4,8,12,16,24,32

## Radius
- sm: 6px, md: 8px, lg: 12px
```

## Untuk n8n-rust
Buat file DESIGN_TOKENS.md di root web/ dengan tokens n8n asli.

## Checklist
- [ ] Tokens di :root CSS
- [ ] Tidak ada warna random, hanya palette n8n
- [ ] Spacing konsisten 8px grid
