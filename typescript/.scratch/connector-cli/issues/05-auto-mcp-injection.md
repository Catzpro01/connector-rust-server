# 05: Auto MCP Injection & Agent Harness Handshake

**What to build:**
Pendaftaran otomatis konfigurasi .mcp.json di direktori kerja aktif saat menghubungkan ke project VPS, sehingga runtime agent langsung mengenali server remote.

**Blocked by:** 04-interactive-tui-and-live-tabs

**Status:** ready-for-agent

- [x] registerMcpConfig menggabungkan server "connector" ke file .mcp.json tanpa merusak konfigurasi lain
- [x] cmdConnect mencetak path Session Disk lokal dan status kesiapan tool MCP
- [x] Verifikasi koneksi end-to-end dari terminal
