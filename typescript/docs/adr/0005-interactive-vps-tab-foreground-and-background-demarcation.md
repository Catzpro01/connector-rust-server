# Interactive Remote VPS Tab Session (Foreground Shell by Default with Explicit Background Detach)

We establish that the primary VPS Tab experience is an authentic, interactive foreground remote shell session (`[VPS] fern@master:~/work$ `) over Streamable HTTP MCP, reserving background detachment (`bg` / `&`) as an explicit user choice.

## Status
Accepted

## Context
When performing remote work on the VPS, treating every operation as an immediate background task creates a disconnect: the developer and agent lose the immediate feedback of being *inside* the VPS terminal, miss live stdout/stderr streams, and cannot naturally navigate directories or inspect files. A remote tab must present the real environment of the VPS by default.

## Decision
1. **Interactive VPS Tab Session as Primary Experience**:
   - `connector-cli vps` (without arguments) and `connector-cli tab vps` launch an interactive remote shell session directly on the VPS.
   - The prompt prominently reflects the remote identity: `[VPS] fern@master:<cwd>$ `, ensuring the user is explicitly aware of their presence in the VPS workspace.
2. **Stateful Working Directory Tracking over HTTP**:
   - Each interactive command execution runs inside the tracked working directory (`currentCwd`), updating automatically on `cd` commands across stateless HTTP JSON-RPC calls.
3. **Foreground Execution by Default**:
   - Standard commands run interactively in the foreground, streaming stdout and stderr live to the user.
4. **Explicit Background Detachment**:
   - Multitasking background tabs are created explicitly via `bg <cmd>`, `<cmd> &`, or `connector-cli tab new --vps <name> <cmd>`, preserving clear demarcation between active work and long-running background tasks.
5. **Universal Portability (Pure HTTP)**:
   - All session interactions occur over HTTP Streamable MCP (`/mcp` `exec.run`), requiring zero SSH credentials or port 22 access.

## Consequences
- Remote work feels instantaneous, interactive, and transparently located on the VPS.
- Full context and working directory are maintained across commands.
- Long-running builds (Rust cargo build, npm test) can still be detached to background tabs on demand.
