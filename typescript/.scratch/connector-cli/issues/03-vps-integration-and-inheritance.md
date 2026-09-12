# 03: Remote VPS Workspace & Progress Inheritance Integration

**What to build:**
Integrasi klien MCP untuk memanggil tool VPS (session.enter, project.list, project.latest, exec.run-background), membaca inherited memory dari generation ledger, dan auto-download skill manifest.

**Blocked by:** 01-core-config-and-identity

**Status:** ready-for-agent

- [x] callMcpTool mendukung generic typing typesafe tanpa type assertion 'any'
- [x] session.enter mencatat login dan menaikkan generasi di VPS
- [x] project.latest memuat MEMORY.md dan riwayat milestone generasi sebelumnya
- [x] downloadSkills mengunduh skill manifest dari GitHub ke Session Disk lokal
- [x] Unit test tests/skills.test.ts lolos 100%
