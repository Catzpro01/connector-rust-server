---
name: hallmark
description: Tier B - Linter kualitas visual untuk konsistensi layout dan struktur styling. Aktivasi saat butuh cek konsistensi, layout berantakan, atau quality gate sebelum ship.
---

# hallmark — Tier B Visual Linter

## Checks
- Layout: semua pakai 8px grid? spacing konsisten?
- Colors: hanya palette n8n? tidak ada random hex?
- Typography: hanya Inter? weight 400/500/600?
- Radius: hanya 6px/8px?
- Shadow: hanya 2 level (subtle, hover)?
- No AI slop patterns?

## Untuk n8n-rust
Jalankan checklist sebelum commit:
- [ ] No gradient ungu
- [ ] Radius konsisten
- [ ] Shadow konsisten
- [ ] Colors dari tokens
- [ ] Spacing 8px grid
