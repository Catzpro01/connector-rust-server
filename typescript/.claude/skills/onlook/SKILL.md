---
name: onlook
description: Tier S - Menjembatani desainer dan coder. Edit UI langsung visual di browser, perubahan otomatis ditulis ke file kode React/Next.js. Aktivasi saat butuh visual editing, WYSIWYG, atau iterasi cepat UI n8n clone.
---

# onlook — Tier S Visual Editor

## Konsep
Onlook memungkinkan edit UI langsung di browser seperti Figma, tapi perubahan langsung menjadi kode.

## Untuk n8n-rust single-file
Simulasikan prinsip onlook:
- Setiap elemen harus punya data-onlook-id
- Edit visual harus tercermin di DOM + localStorage
- Gunakan contenteditable untuk teks, drag untuk posisi

## Implementasi
- Tambahkan mode visual edit toggle
- Saat mode edit aktif, klik elemen untuk edit props
- Simpan perubahan ke workflow JSON

## Checklist n8n clone
- [ ] Node bisa di-drag visual dengan pointer capture
- [ ] Text editable inline (workflow name, node name)
- [ ] Perubahan langsung save ke localStorage + history
