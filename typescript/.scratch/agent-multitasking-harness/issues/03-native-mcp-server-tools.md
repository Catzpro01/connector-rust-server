# 03: Native MCP Server Tools Integration (`multitasking-tabs`)

**What to build:** Expose the dual-layer multitasking tab harness directly as standard Model Context Protocol (MCP) tools via a local Stdio server, registered in `.mcp.json` so AI coding agents (Antigravity, Claude Code, Cline) can orchestrate tabs natively.

**Blocked by:** 02 (Linux-Native Command Prompt Background Tab Multiplexer)

**Status:** resolved

- [x] Stdio MCP server implements JSON-RPC 2.0 protocol over standard input/output.
- [x] Exposes 6 canonical tools: `tab_new`, `tab_list`, `tab_attach`, `tab_kill`, `tab_clean`, and `vps_exec`.
- [x] Registered in `.mcp.json` at project root with command `connector-cli` and argument `mcp`.
- [x] Agent can call `tab_new` and immediately receive task ID without waiting for process execution to finish.
