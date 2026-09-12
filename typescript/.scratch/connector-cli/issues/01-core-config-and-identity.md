# 01: Core Configuration & Agent Login Identity

**What to build:**
Sistem autentikasi agent berbasis nama (login tanpa password) dan konfigurasi server VPS yang tersimpan persisten di Local Disk (~/.connector-cli/config.json) dengan isolasi test directory.

**Blocked by:** None (can start immediately)

**Status:** ready-for-agent

- [x] Fungsi readStoredConfig, saveStoredConfig, dan localDiskConfigPath
- [x] Fungsi promptAgentName untuk prompt interaktif nama login agent
- [x] Fungsi loadCliConfig dengan parameter opsional configDir untuk isolasi unit test
- [x] Unit test tests/cli.test.ts lolos 100%
