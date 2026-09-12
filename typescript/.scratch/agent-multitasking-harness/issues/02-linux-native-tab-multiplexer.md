# 02: Linux-Native Command Prompt Background Tab Multiplexer

**What to build:** Allow the agent to spawn non-blocking background command prompt tabs in milliseconds on Laptop (using POSIX `/bin/sh`) and on VPS (using `exec.run-background`), switch immediately to subsequent tasks, and passively inspect status via file sentinels and live streaming logs.

**Blocked by:** 01 (Unrestricted Linux Shell Execution on VPS via Pure HTTP)

**Status:** resolved

- [x] Local tasks run asynchronously via `/bin/sh` or `/bin/bash` in detached mode without blocking the CLI or agent turn.
- [x] VPS background tasks spawn via `exec.run-background` and map seamlessly into the unified tab dashboard.
- [x] Termination is recorded via passive sentinel `.exit` files containing integer exit codes.
- [x] `connector-cli tab attach <name|id>` streams live output without stopping the active background process.
- [x] `connector-cli tab kill <name|id>` gracefully stops runaway local or VPS processes.
