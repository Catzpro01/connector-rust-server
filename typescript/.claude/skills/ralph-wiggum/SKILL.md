---
name: ralph-wiggum
description: "Run the Ralph Wiggum self-referential loop: repeat the SAME prompt until a completion promise is genuinely true or max iterations hit. Use for well-defined iterative tasks with automatic verification (tests, lint, build)."
---

# Ralph Wiggum Loop

The Ralph Wiggum technique (Geoffrey Huntley: "Ralph is a Bash loop") adapted
as an agent skill. Upstream: the official Anthropic plugin
`anthropics/claude-code/plugins/ralph-wiggum`, vendored verbatim under
`./upstream/` (see `UPSTREAM.md` for provenance).

## The loop (no Stop-hook infra here — run it as discipline)

1. **Write the PROMPT first.** One block, immutable for the whole run:
   - task + hard completion criteria (exact commands that must pass),
   - the promise line: `Output <promise>TEXT</promise> ONLY when ...`,
   - `--max-iterations N` (always set; the primary safety net),
   - stuck-protocol: "past iteration K without progress: stop forcing,
     document blocker + attempts + alternatives, end WITHOUT the promise."
2. **Create state** `.ralph-loop.local.md` at repo root (gitignored):
   frontmatter `active/iteration/max_iterations/completion_promise/started_at`
   + the prompt body. Same shape as upstream (see `upstream/scripts/`).
3. **Iterate with the SAME prompt.** Each iteration: work → run the
   verification commands → read own prior work from files/git → improve.
   Never rewrite the prompt mid-run; tune it only between runs
   ("failures are data").
4. **Promise honesty (hard rule).** Emit `<promise>TEXT</promise>` ONLY when
   the statement is completely and unequivocally TRUE. Never emit a false
   promise to escape, even when stuck — end via max-iterations with a
   blocker report.
5. **Stop conditions:** promise genuinely emitted, or iteration == max.
   On stop: delete the state file, report iterations used + what passed +
   what remains.

## Iteration discipline

- One verification cycle per iteration; quote real command output, never
  paraphrase results from memory.
- If verification tooling is unavailable (no toolchain/network), SAY SO and
  end the loop — an unverified promise is a false promise.
- Iteration > perfection: ship the small green step, let the loop refine.

## When to use / not use

Good: well-defined tasks with checkable success (tests/lint/build green),
refactors, "get X passing" grinds. Bad: tasks needing human judgment or
design decisions, one-shots, unclear criteria, production debugging.

## Bundled upstream files (`./upstream/`, verbatim)

- `commands/ralph-loop.md`, `commands/cancel-ralph.md`, `commands/help.md`
- `hooks/hooks.json`, `hooks/stop-hook.sh` (Stop-hook mechanism for real
  Claude Code — not active here)
- `scripts/setup-ralph-loop.sh` (state-file setup for real Claude Code)
- `.claude-plugin/plugin.json`, `README.md` (prompt-writing best practices)

To use as a genuine Claude Code plugin instead of this skill, install
upstream directly (see UPSTREAM.md) — the hook then enforces the loop
mechanically.
