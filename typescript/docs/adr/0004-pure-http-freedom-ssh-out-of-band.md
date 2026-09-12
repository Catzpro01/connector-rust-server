# Pure HTTP Remote Execution with Shell-Level Freedom (SSH as Out-of-Band Backtest Only)

We decouple remote agent operations entirely from SSH port 22, guaranteeing full Linux shell-level freedom over pure HTTP/HTTPS Streamable MCP endpoints, while reserving SSH exclusively as an out-of-band developer backtest harness.

## Status
Accepted

## Context
Attempting to bundle SSH into the agent's application tools introduces fragile key dependencies, DNS hostname resolution failures, and port 22 firewall blocking. Meanwhile, restricting the agent to a narrow sandboxed API inhibits its ability to inspect, compile, and administer system workloads.

## Decision
1. **Pure HTTP In-Band Channel**:
   - The agent harness connects to the VPS exclusively via Streamable HTTP/HTTPS (port 3210, `/mcp`). All remote commands run via `exec.run` and `exec.run-background` with full `/bin/bash` shell freedom.
2. **Zero SSH in Application Tools**:
   - No SSH client code, keys, or port 22 tunneling are packaged inside `connector-cli` or its MCP server.
3. **Out-of-Band Backtest Harness**:
   - Developer SSH terminal sessions (`fern@master`) exist solely outside the application as a baseline to verify and compare agent execution results.

## Consequences
- The agent harness is completely portable and free from SSH key/port friction.
- The agent exercises complete shell-level freedom on the remote VPS without compromising host architecture.
- Developers retain a dependable out-of-band verification channel.
