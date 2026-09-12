---
name: axe-core-a11y
description: Tier C - Validator kepatuhan aksesibilitas WCAG 2.2 AA. Aktivasi saat butuh a11y check, keyboard nav, atau screen reader support untuk n8n clone.
---

# axe-core-a11y — Tier C

## Checks
- Keyboard: Tab navigation, Enter to select, Delete to delete, Esc to clear, ⌘K palette
- Contrast: text #2d2d2d on #fff = 15:1 PASS, #666 on #fff = 5.7:1 PASS
- Focus: visible ring
- Labels: buttons punya aria-label

## Untuk n8n-rust
- [ ] All buttons keyboard accessible
- [ ] Focus ring visible
- [ ] Contrast AA
