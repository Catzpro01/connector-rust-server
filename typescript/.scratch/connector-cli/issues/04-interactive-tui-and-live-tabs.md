# 04: Interactive TUI Dashboard & Unified Live Tabs

**What to build:**
Menu terminal interaktif (1-7) dengan layout rapi ala Claude Code UX untuk berpindah antar project, membuat project baru, memantau live tabs (VPS + Local Tasks), dan mengedit pengaturan server.

**Blocked by:** 02-local-tasks-manager, 03-vps-integration-and-inheritance

**Status:** ready-for-agent

- [x] interactiveMenu dengan header agent name, menu 1-7, dan loop interaktif
- [x] cmdTabs menampilkan VPS remote tasks dan laptop local tasks secara berdampingan
- [x] cmdNewTab memungkinkan memilih start background task di VPS atau Local Laptop
- [x] cmdSetting memperbarui URL server, API key, dan agent login secara persisten
