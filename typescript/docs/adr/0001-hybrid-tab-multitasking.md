# Hybrid Tab Multitasking and Non-Blocking Task Execution

To eliminate agent execution starvation during heavy builds and tool runs, we decouple long-running operations into independent tabs (Command Prompt multiplexers) distributed across Laptop and VPS. The agent relies on passive sentinel exit checks rather than blocking loops or intrusive interruption signals, and synchronizes source code and artifacts via Git worktrees and selective artifact fetching.

## Status
Accepted

## Context
When an autonomous coding agent compiles Rust code, installs heavy dependencies, or executes long test suites, synchronous execution blocks the agent conversation turn. The user needs the agent to work concurrently—like a developer opening multiple terminal tabs—switching immediately to research, write, or refactor code while the compile task runs asynchronously in the background.

## Decision
1. Provide a dual-layer tab multiplexer (`connector-cli tab`) supporting both `LocalTab` (Laptop) and `VpsTab` (Remote VPS).
2. Use file-based passive sentinels (`.exit` files) to record termination codes without active agent polling or interrupt hooks.
3. Adopt an Auto-Hybrid execution strategy where resource-heavy tasks (`cargo build`, release packaging) route to the VPS while editing and unit verification remain local.
4. Synchronize source trees via Git worktrees and selectively pull resulting artifacts via `connector-cli`.

## Consequences
- The agent never sits idle waiting for compile commands to finish.
- Laptop system resources remain free for interactive responsiveness while VPS takes the CPU burn.
- Task logs are isolated per tab ID and can be peeked at or killed deterministically.
