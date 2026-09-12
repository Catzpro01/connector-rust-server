# Auto-Hybrid Routing Rules, Retention Policy, and Shadow Branch Git Sync

We establish explicit heuristics for auto-routing compute tasks, bound tab disk usage with a 50-item rolling retention limit, and isolate remote VPS builds using a temporary shadow Git branch.

## Status
Accepted

## Context
Running long operations on the laptop drains resources, while manually specifying `--vps` for every heavy build is friction-heavy. Moreover, unbounded task metadata accumulates disk usage, and transferring uncommitted local code to the VPS risks polluting main Git history.

## Decision
1. **Keyword Rulebook Routing**:
   - Route to VPS: `cargo build --release`, `docker build`, `npm run build:full`, heavy benchmarks.
   - Run on Laptop: `cargo check`, unit tests, linting, git status, file edits.
2. **Rolling Limit 50 Retention**:
   - Limit local tab history to the 50 most recent records. Exited tabs beyond this threshold are pruned automatically during clean operations.
3. **Shadow Build Branch (`sync/vps-build`)**:
   - Workspace state is committed to a transient branch before triggering VPS builds, preserving local branch history and ensuring pristine build environments.

## Consequences
- The agent automatically chooses the optimal execution venue without user micro-management.
- Disk usage on the agent machine remains strictly bounded.
- The Git log of feature and main branches remains clean and free of experimental intermediate commits.
