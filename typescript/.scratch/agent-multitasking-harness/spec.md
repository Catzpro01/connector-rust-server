# Spec: Agent Multitasking & Remote HTTP Harness (V1.0)

Status: implemented

## Problem Statement

When an autonomous AI agent or developer runs resource-intensive, long-running operations (such as compiling Rust crates, building Docker images, running full test suites, or installing npm packages), synchronous CLI execution blocks the conversation turn. The agent sits idle, burns token context, and risks hit-and-miss timeouts. Furthermore, delegating work to a remote VPS has historically suffered from fragile SSH authentication, DNS hostname resolution failures, and complex API key configuration.

## Solution

A dual-layer command prompt tab multiplexer (`connector-cli` & native MCP tools) designed for Linux-native agent environments (Linux, WSL, containers). The harness communicates with the remote VPS strictly over a Streamable HTTP/HTTPS MCP link (`http://103.55.37.234:3210/mcp`) without SSH. The agent can spawn asynchronous background tabs in milliseconds on either the local laptop or the VPS, immediately proceed with other coding tasks without blocking, and non-intrusively peek or attach to execution logs on demand via file-based passive sentinels.

## User Stories

1. As an autonomous AI agent, I want to spawn a background tab for heavy builds (`cargo build`) via `tab_new`, so that my current execution turn is not blocked waiting for compilation.
2. As an autonomous AI agent, I want to immediately receive a tab ID and running status, so that I can transition to writing documentation, specs, or refactoring other files without idling.
3. As an autonomous AI agent, I want to run long-running commands directly in the remote VPS environment over HTTP (`vps_exec` / `connector-cli vps`), so that I do not need SSH keys, known_hosts files, or port 22 access.
4. As an autonomous AI agent, I want to open a background tab directly in the VPS (`tab_new` with target `vps`), so that heavy compilation burns VPS CPU rather than draining the developer's laptop battery.
5. As an autonomous AI agent, I want to query a live dashboard of all running and completed tabs (`tab_list` / `connector-cli tabs`), so that I can see the real-time execution status of my multitasking workload.
6. As an autonomous AI agent, I want to attach and peek at the stdout/stderr logs of any tab (`tab_attach` / `connector-cli tab attach`), so that I can inspect progress without interrupting the process.
7. As an autonomous AI agent, I want the system to record process termination using a passive sentinel `.exit` file, so that I can verify success or failure via exit codes without active polling loops.
8. As an autonomous AI agent, I want to terminate runaway or frozen background processes (`tab_kill` / `connector-cli tab kill`), so that system resources are immediately reclaimed.
9. As an autonomous AI agent, I want to clean up exited tabs (`tab_clean` / `connector-cli tab clean`), so that my task history and dashboard remain uncluttered.
10. As a developer, I want the harness to install out-of-the-box (`npm install -g .`), so that it connects to the VPS immediately with zero manual configuration, prompt wizards, or API key setup.
11. As a developer, I want all local tasks to run inside standard Linux POSIX shells (`/bin/sh` / `bash`), so that script execution remains fully portable across Linux and WSL environments.
12. As a developer, I want my agent to possess native MCP tool definitions for tab management, so that any AI assistant (Claude Code, Antigravity, Cline) can orchestrate multitasking natively without raw bash syntax.

## Implementation Decisions

1. **Pure HTTP/HTTPS Transport for VPS (Zero SSH)**:
   - All remote operations communicate with the VPS exclusively via Streamable HTTP JSON-RPC 2.0 to endpoint `http://103.55.37.234:3210/mcp`.
   - Direct SSH connections (port 22) are entirely removed from the execution path. Remote execution is driven by MCP tool `exec.run` and `exec.run-background`.

2. **Linux-Native Shell Multiplexing**:
   - Background tasks run through POSIX-compliant `/bin/sh` or `/bin/bash` with stdout/stderr redirection to log files (`.log`) and exit code capture to sentinel files (`.exit`).
   - Process detached spawning uses `child_process.spawn` with `detached: true` and `child.unref()`.

3. **Dual-Layer Tab Architecture**:
   - `LocalTab`: Detached process on the developer's laptop/WSL, stored persistently under `~/.connector-cli/tasks/`.
   - `VpsTab`: Detached container task on the VPS remote workspace, queried through `exec.list` and managed via `exec.kill`.
   - Unified interface (`TabManager`) presenting both local and remote tabs in a single dashboard.

4. **Passive Sentinel Lifecycle & Auto-Pruning**:
   - Status checks inspect the presence and numeric value of `.exit` files.
   - Rolling limit retention bounds task history to the 50 most recent records, auto-pruning older exited tasks.

5. **Zero-Config Postinstall Bootstrap**:
   - A `postinstall` script automatically provisions `~/.connector-cli/config.json` defaulting to `http://103.55.37.234:3210`, Open Dev Mode, and system-derived agent identity.
   - Interactive CLI menus present live connection indicators without prompt wizards.

6. **Native MCP Server Protocol Integration**:
   - Stdio MCP server exposing 6 canonical tools: `tab_new`, `tab_list`, `tab_attach`, `tab_kill`, `tab_clean`, `vps_exec`.
   - Registered in `.mcp.json` at project root.

## Testing Decisions

- **Testing Seams**:
  - Highest possible seams: The public CLI commands (`connector-cli tab`, `connector-cli vps`, `connector-cli status`) and the Stdio MCP server tool handlers.
  - No internal private methods are tested; tests evaluate observable external behaviors.
- **Verification Criteria**:
  - Tab spawning must complete in <100ms and return a valid ID while the child process continues running in the background.
  - Log attachment must return non-empty streaming output as the background process writes to stdout.
  - Kill operation must successfully terminate the process and record exit code 143/SIGTERM.
  - VPS remote commands must successfully execute on `master` (`fern`) via HTTP MCP without SSH.
- **Prior Art**:
  - Tests in `tests/local.test.ts`, `tests/cli.test.ts`, and `tests/skills.test.ts` using Vitest with mock and real detached spawns.

## Out of Scope

- SSH port 22 key-based tunneling or interactive TTY SSH sessions.
- Full visual canvas UI replication of n8n.
- Direct root access or kernel-level modifications on the VPS host OS.

## Further Notes

- All changes adhere strictly to the normative vocabulary defined in `CONTEXT.md` and architectural records ADR 0001, 0002, 0003, and PRD V1.0.
