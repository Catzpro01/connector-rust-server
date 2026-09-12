# 01: Unrestricted Linux Shell Execution on VPS via Pure HTTP

**What to build:** Provide the agent with full Linux shell-level execution freedom on the remote VPS strictly through Streamable HTTP/HTTPS (Port 3210, `/mcp`), eliminating all internal SSH dependencies while returning exact stdout, stderr, and exit codes.

**Blocked by:** None (can start immediately)

**Status:** resolved

- [x] Remote commands run through `callMcpTool("exec.run", ...)` over HTTP without touching SSH port 22.
- [x] Terminal CLI command `connector-cli vps <command...>` streams stdout and stderr directly from the remote VPS.
- [x] Non-zero exit codes from remote Linux processes are captured and propagated correctly to the caller.
- [x] Execution operates within the VPS environment (`master`, user `fern`) with full bash freedom.
