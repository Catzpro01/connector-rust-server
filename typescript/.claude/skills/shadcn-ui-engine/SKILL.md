---
name: shadcn-ui-engine
description: Tier S - Wajib Pakai - Standar industri modern shadcn-ui. Memberikan kode komponen React/Tailwind copy-pasteable tanpa dependensi berat (headless primitives Radix). 100% kode milik Anda di repo. Aktivasi saat butuh komponen UI nyata, design system, atau build interface mirip n8n asli tanpa AI slop.
---

# shadcn-ui-engine — Tier S

## Filosofi
shadcn-ui adalah standar industri 2024-2026: bukan library npm berat, tapi koleksi komponen copy-paste yang 100% milik Anda. Menggunakan Radix UI primitives (headless, accessible) + Tailwind CSS.

## Prinsip Anti AI Slop
- JANGAN pakai gradient ungu berlebihan, rounded 3xl aneh, shadow berlebihan
- Pakai whitespace rhythm elegan: 4px grid, spacing konsisten
- Warna netral + accent terbatas (n8n: #ff6d5a primary, #2d2d2d text, #e8e8e8 border)
- Komponen harus fungsional, bukan dekoratif

## Komponen Wajib untuk n8n clone
- Button: height 32-36px, radius 6px, font 500 13px, border 1px solid #e8e8e8
- Card/Node: bg #fff, border 1px solid #e8e8e8, radius 8px, shadow 0 2px 6px rgba(0,0,0,0.08)
- Input: height 36px, radius 6px, border 1px solid #e8e8e8, focus ring #ff6d5a
- Sidebar: bg #fff, border-right 1px solid #e8e8e8, width 320px, collapsible
- Canvas: bg #fbfbfb, dot grid 20px, radial-gradient dot #e2e2e2

## Implementasi untuk n8n-rust
1. Jangan pakai React di file single-file? Simulasikan shadcn styling dengan CSS variables mirip shadcn
2. Gunakan Tailwind-like utility via CSS custom
3. Pastikan komponen copy-pasteable, bukan generated AI slop

## Checklist
- [ ] Button primary #ff6d5a, hover #ff8f7f, radius 6px
- [ ] Node card 240px, radius 8px, shadow subtle, selected ring 2px #ff6d5a
- [ ] Input 36px, focus ring 3px rgba(255,109,90,0.2)
- [ ] Whitespace 8px grid, no excessive gradients
