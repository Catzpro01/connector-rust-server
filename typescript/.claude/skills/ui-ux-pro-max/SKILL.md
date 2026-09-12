---
name: ui-ux-pro-max
description: Tier B - Ensiklopedia prinsip desain (kontras rasio, micro-interactions, states hover/active/disabled). Aktivasi saat butuh polish UX, micro-interactions, atau states mirip n8n asli.
---

# ui-ux-pro-max — Tier B

## Prinsip
- Kontras rasio WCAG AA 4.5:1
- Micro-interactions: hover 0.15s, active scale 0.98, focus ring
- States: default, hover, active, disabled, focus, selected, executing, success, error

## Untuk n8n-rust
- Button: hover bg #f8f8f8, active scale 0.98, focus ring #ff6d5a 3px
- Node: hover shadow larger, selected ring 2px #ff6d5a, executing pulse
- Handle: hover scale 1.3, connected bg #ff6d5a
- Edge: hover stroke #999 width 3, selected #ff6d5a

## Checklist
- [ ] Semua interactive punya hover/active/focus
- [ ] Transition 0.15s ease, bukan instant
- [ ] Focus ring visible untuk accessibility
