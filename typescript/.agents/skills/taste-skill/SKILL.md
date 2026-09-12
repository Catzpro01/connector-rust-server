---
name: taste-skill
description: Tier B - Membuang pola UI buatan AI yang norak/generik (gradient ungu berlebihan, rounded-corner aneh) dan menegakkan ritme visual whitespace elegan. Aktivasi saat UI terlihat AI slop, kaku, atau norak.
---

# taste-skill — Tier B Anti AI Slop

## Pola AI Slop yang HARUS dibuang
- Gradient ungu-pink berlebihan (#8b5cf6 → #ec4899)
- Rounded 2xl/3xl aneh (radius 20px+)
- Shadow berlebihan dengan blur besar
- Glassmorphism berlebihan
- Emoji berlebihan di UI profesional
- Dark mode default untuk tool produktivitas (n8n asli light)

## Ritme Visual Elegan (n8n asli)
- Whitespace besar, breathing room
- Warna netral: #fff bg, #2d2d2d text, #e8e8e8 border, #ff6d5a accent SATU
- Radius konsisten: 6px (kecil), 8px (card), bukan 20px
- Shadow subtle: 0 2px 6px rgba(0,0,0,0.08), bukan 0 20px 60px
- Font Inter, weight 400/500/600, size 12-14px
- No excessive animations, hanya 0.15s ease

## Checklist n8n clone
- [ ] Hapus gradient ungu, ganti #ff6d5a
- [ ] Radius max 8px, bukan 20px
- [ ] Shadow subtle, bukan dramatic
- [ ] Light theme default (n8n asli light)
- [ ] Whitespace 16-24px, bukan cramped
