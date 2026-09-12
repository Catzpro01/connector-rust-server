# Spec: Connector MCP System (CLI & Dual-Layer Multitasking)

## Problem Statement

AI Agent yang beroperasi dalam sesi terisolasi (misalnya Claude Code, Gemini CLI, Cursor, atau web UI) hanya memiliki akses lokal ke disk sesi sementara (*Session Disk*) atau mesin lokalnya sendiri. Masalah utama yang dihadapi:
1. **Context Window Exhaustion**: Sesi yang panjang mengalami degradasi memori (*context rot*) dan terputus tanpa ada cara otomatis untuk mewariskan progress ke sesi berikutnya.
2. **Keterbatasan Eksekusi**: Agen tidak dapat langsung berinteraksi dengan sumber daya server/VPS yang persisten dan berkinerja tinggi tanpa setup SSH manual yang rumit dan rentan bocor kredensial.
3. **Ketiadaan Multitasking**: Agen tidak memiliki mekanisme "tab" latar belakang (*background tasks*) yang persisten baik di laptop lokalnya maupun di VPS.

## Solution

Sistem Connector MCP Server & CLI yang menghubungkan sesi agen terisolasi ke remote workspace di VPS dan mesin lokal melalui:
1. **Pewarisan Progress Otomatis (*Progress Inheritance*)**: Ledger generasi stateful di VPS (`MEMORY.md`, milestone, file changes) yang secara otomatis dibaca oleh sesi agen berikutnya saat login.
2. **Dual-Layer Multitasking**: Kemampuan menjalankan, memantau, dan menghentikan background task baik di VPS remote maupun di Local Disk laptop (`lt-*`).
3. **UX Bersih & Terisolasi**: TUI interaktif dengan sistem login agen tanpa password (API key sebagai rahasia), auto-download skill manifest, dan auto-injection `.mcp.json`.

---

## User Stories

1. As an autonomous agent, I want to authenticate by name without password, so that my work and milestones are attributed to me in the shared project ledger.
2. As an autonomous agent, I want to enter an active project session, so that the remote server increments the project generation and creates a clean isolated Session Disk.
3. As an autonomous agent, I want to receive inherited memory (past milestones, file changes, open tasks) when entering a session, so that I do not lose progress when a chat context resets.
4. As an autonomous agent, I want to download declared skills from GitHub into my Session Disk automatically, so that I have the exact toolset required for the project.
5. As an autonomous agent, I want a valid `.mcp.json` automatically generated in the working directory, so that standard agent harnesses can immediately discover all remote MCP tools.
6. As a developer, I want an interactive terminal menu with clear numbered options, so that I can navigate projects, tabs, and settings without memorizing CLI syntax.
7. As an agent or developer, I want to launch long-running background tasks on the remote VPS, so that resource-intensive jobs run independently of the local machine.
8. As an agent or developer, I want to launch detached background tasks on my local machine (Local Disk), so that builds or test watchers continue running even if the CLI exits.
9. As an agent or developer, I want to view a unified "Live Tabs" dashboard, so that I can monitor the status of both VPS tasks and local laptop tasks side-by-side.
10. As an agent or developer, I want to inspect and attach to local task logs, so that I can see the exact real-time output and exit codes.
11. As an agent or developer, I want to gracefully terminate stuck local or remote tasks, so that computing resources are freed cleanly.
12. As a user, I want to update server URL and API key settings persistently via the CLI menu, so that I do not need to reconfigure environment variables every time.

---

## Implementation Decisions

1. **Dual-Layer Execution Boundary**:
   - Remote Tasks: Dikelola oleh `connector.service` di VPS via HTTP endpoint `/mcp` dan Podman / process execution sandbox.
   - Local Tasks: Dikelola oleh `LocalTaskManager` pada Local Disk (`~/.connector-cli/tasks`), dengan shell redirection dan sentinel exit file (`.exit`) untuk mendukung Windows (`cmd.exe` / Git Bash) dan POSIX (`sh`).
2. **Session Disk Isolation (ADR 0002 & 0003)**:
   - Sesi terisolasi dibuat di temporary directory pengguna (`%TEMP%/connector-session-*`), dibersihkan saat sesi ditutup atau dibuang saat generasi baru dimulai.
3. **Agent Identity & Ledger Model (ADR 0004 & 0011)**:
   - Nama agent disimpan di `~/.connector-cli/config.json`.
   - API Key (`X-API-Key`) bertindak sebagai secret bearer authentication; nama agent hanya label atribusi pada commit/milestone di Ledger VPS.
4. **Type-Safety & Zero `any` (Matt Pocock Total TypeScript Standards)**:
   - Seluruh payload MCP menggunakan strict interfaces (`RunBackgroundResult`, `ProjectRecord`, `CliConfig`).
   - Penanganan error runtime menggunakan error narrowing (`(err as Error).message`).
5. **Pre-factoring Handoff Window**:
   - Minimal 80ms handoff delay saat spawn proses detached di Windows sebelum proses induk CLI keluar, memastikan event loop libuv menyelesaikan registrasi proses di OS.

---

## Testing Decisions

- **Testing Seam**:
  1. *High-level CLI seam*: Pengujian fungsi CLI end-to-end (`loadCliConfig`, `registerMcpConfig`, `mcpServerEntry`) dengan mock isolated temporary directories.
  2. *Process Manager seam*: Pengujian `LocalTaskManager` mencakup lifecycle lengkap: `start` → polling `refresh` → `output` verification → `kill` signal.
  3. *Skill Manifest seam*: Pengujian parsing dan download skill manifest GitHub.
- **Behavioral Rule**: Tidak menguji detail internal spawn/fd; hanya menguji perilaku observable eksternal (exit status, file log, status string).
- **Prior Art**: Mengikuti test suite Vitest di `tests/cli.test.ts`, `tests/local.test.ts`, dan `tests/skills.test.ts`.

---

## Out of Scope

1. Web-based GUI dashboard (tetap fokus pada TUI terminal CLI ala Claude Code).
2. Multi-tenant RBAC granular di VPS (cukup model Owner Key vs Agent Key).
3. P2P syncing antar mesin lokal tanpa VPS.

---

## Further Notes

Sistem ini adalah fondasi jembatan anti context-loss bagi autonomous coding agent untuk beralih secara mulus antara laptop dan server cloud.
