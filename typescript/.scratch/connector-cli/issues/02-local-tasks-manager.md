# 02: Dual-Layer Multitasking (Local Tasks Manager)

**What to build:**
Pengelolaan background command (Local Tasks / tabs) di laptop dengan shell redirection, sentinel exit recording (.exit), handoff window, attach/kill log, dan test suite cross-platform.

**Blocked by:** 01-core-config-and-identity

**Status:** ready-for-agent

- [x] LocalTaskManager.start mendukung Windows (cmd.exe & Git Bash sh.exe) dan POSIX sh
- [x] Sentinel exit file (.exit) mencatat exit code proses latar belakang secara persisten
- [x] LocalTaskManager.output menampilkan cuplikan log hingga 200KB
- [x] LocalTaskManager.kill menghentikan proses via SIGTERM/taskkill
- [x] Unit test tests/local.test.ts lolos 100%
