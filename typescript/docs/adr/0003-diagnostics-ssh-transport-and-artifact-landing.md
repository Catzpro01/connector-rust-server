# Structured Diagnostics, Direct SSH Git Transport, and Native Target Landing

We establish structured compiler diagnostic collection for self-healing bug fixes, point-to-point SSH Git transport for instant code sync, and native `target/release/` binary landing for local workflow parity.

## Status
Accepted

## Context
When remote builds fail, unstructured raw terminal dumps force human intervention. Central Git platforms (e.g. GitHub) introduce unnecessary network round-trips and external rate-limiting for local-to-VPS staging. Finally, storing fetched binaries in non-standard directories breaks existing local test runners and build scripts.

## Decision
1. **Structured Cargo Diagnostics**:
   - Remote Rust builds capture compiler JSON messages (`--message-format=json`) alongside ANSI logs, enabling the agent to extract exact file paths, line numbers, and error codes for automated self-healing.
2. **Direct SSH Git Transport**:
   - Repository synchronization between Laptop and VPS uses direct SSH URLs (`ssh://fern-vps/...`), bypassing external hosting services.
3. **Native Target Landing**:
   - Fetched release binaries from the VPS are mirrored into the local `target/release/` directory to seamlessly integrate with local CLI and test tooling.

## Consequences
- The agent can autonomously parse compiler failures and generate surgical fixes.
- Git sync is low-latency, private, and works without third-party Git services.
- Local tools and developers can execute compiled binaries without modifying file paths.
