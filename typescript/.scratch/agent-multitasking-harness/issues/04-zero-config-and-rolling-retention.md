# 04: Zero-Config Auto-Connect and Rolling Tab Retention

**What to build:** Ensure that installing the CLI (`npm install -g .`) automatically binds the agent to the VPS endpoint (`http://103.55.37.234:3210`) with zero user prompts, and enforces a 50-task rolling retention limit to preserve disk hygiene.

**Blocked by:** 02 (Linux-Native Command Prompt Background Tab Multiplexer), 03 (Native MCP Server Tools Integration)

**Status:** resolved

- [x] Package postinstall lifecycle script auto-generates `~/.connector-cli/config.json` with VPS URL and system agent name.
- [x] Interactive CLI menu opens directly with green connection status and zero setup questions.
- [x] Local task store bounds history to a maximum of 50 tasks, auto-pruning oldest completed records.
- [x] `connector-cli tab clean` and MCP tool `tab_clean` prune all exited tasks upon request.
