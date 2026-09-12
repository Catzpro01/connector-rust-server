---
name: agent-multitasking
description: Autonomous command-prompt tab multiplexer and remote execution protocol. Enables AI agents to spawn background tasks on laptop or VPS, avoid blocking execution turns, and inspect logs on-demand.
---

# Agent Multitasking & Remote VPS Harness

This skill equips AI agents with the mental model and concrete tools to perform true multitasking using **Command Prompt Tabs** (dual-layer: Laptop & VPS).

## Core Laws

### 1. Zero Blocking Turns
When an operation takes more than 3 seconds (compilation, package installation, test suites, container builds):
1. **NEVER** run it as a synchronous blocking command in your primary execution turn.
2. **SPAWN** a new background tab immediately using `tab_new` (via MCP) or `connector-cli tab new`.
3. **SWITCH** immediately to the next task in your plan (reading specs, editing unrelated files, writing documentation) without idling or sleeping.
4. **INSPECT** the tab output (`tab_attach` or `connector-cli tab attach <name>`) only when you actually need the result of that task.

### 2. Strict Tab Lifecycle Rule
- **Ephemeral Task Tabs (One-off / Installation / Build)**:
  Once a task finishes (e.g. `apt-get install`, `cargo build`, `npm install`, compiler checks, migrations) and its logs and exit code have been verified, the agent **MUST IMMEDIATELY CLOSE** the tab using `tab_clean` or `tab_kill` (`close <name>` / `clean` in CLI).
  _Prohibition_: NEVER leave finished/exited task tabs accumulating in the tab dashboard.
- **Long-Running Service Tabs (Daemons / Watchers / Dev Servers)**:
  Tabs running continuous processes (HTTP servers, test watchers, workers) remain active in the background. The agent can switch between tabs, inspect logs, and easily return to previous tabs with `prev` / `switch`.

### 3. Container Sandbox Root Isolation
- **Root Project Sandbox**: Remote execution on the VPS runs inside an isolated Podman container (`ubuntu:24.04`) mounting the project directory at `/work`.
- **Privilege Demarcation**: The agent operates as `root` inside `/work` (with full permission to install packages via `apt`, compile native tools, and manage files), but is strictly isolated from the host VPS operating system and credentials.
- **Pure HTTP Streamable MCP**: All transport operates strictly over Streamable HTTP (`http://103.55.37.234:3210/mcp`), without SSH port 22 dependencies. The interactive terminal experience emulates SSH logins (`[VPS:container|<project>] root@<host>:/work#`).

## Available Native Tools (MCP & CLI)

You can invoke these tools either via MCP (`multitasking-tabs` server) or directly through shell commands:

### 1. Open New Background Tab (`tab_new`)
- **CLI (Laptop)**: `connector-cli tab new [name] <command...>`
- **CLI (VPS)**: `connector-cli tab new --vps [name] <command...>`
- **MCP Tool**: `tab_new({ name: "build-task", command: "cargo build", target: "laptop" | "vps" })`

### 2. Live Tab Dashboard (`tab_list`)
- **CLI**: `connector-cli tabs`
- **MCP Tool**: `tab_list({})`
- Displays all live tabs across Laptop and VPS, showing execution status (`🟢 running` / `⚪ exited`) and exit codes.

### 3. Attach / Peek Log Output (`tab_attach`)
- **CLI**: `connector-cli tab attach <name|id>`
- **MCP Tool**: `tab_attach({ name: "build-task" })`
- Retrieves live stdout and stderr from the background tab. If the tab has exited and is no longer needed, close it immediately per the Tab Lifecycle Rule.

### 4. Stop a Tab (`tab_kill`)
- **CLI**: `connector-cli tab kill <name|id>`
- **MCP Tool**: `tab_kill({ name: "build-task" })`

### 5. Clean Exited Tabs (`tab_clean`)
- **CLI**: `connector-cli tab clean`
- **MCP Tool**: `tab_clean({})`
- Prunes finished tasks to maintain a clean workspace dashboard.

### 6. Direct VPS Shell (`vps_exec`)
- **CLI**: `connector-cli vps "<command>"`
- **MCP Tool**: `vps_exec({ command: "uname -a" })`
- Direct point-to-point remote execution inside the VPS project container via pure HTTP streamable MCP (without SSH port 22).

### 7. Interactive Remote VPS Session
- **CLI**: `connector-cli vps`
- Interactive session providing an SSH-like login prompt (`root@<host>:/work#`), multi-tab switcher (`tabs`, `switch <name>`, `prev`), and smart editor interception (`nano`, `vim`, `write`).
