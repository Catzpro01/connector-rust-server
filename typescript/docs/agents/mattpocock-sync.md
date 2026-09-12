# Matt Pocock Skills — Sync Manifest

## Status terakhir

- **Upstream:** https://github.com/mattpocock/skills
- **Release terakhir:** v1.2.3 (2026-08-06)
- **Main HEAD saat sync:** `3cca18b368ae95cdbdebbff572ccafa662551015`
- **Tanggal sync:** 2026-09-10
- **Jumlah skill upstream:** 37 · **Skill custom repo:** 36

## Lokasi di repo ini (pasca wipe v1, 2026-09-10)

| Lokasi | Status |
|---|---|
| `.claude/skills/<nama>/` | 37 upstream + 36 custom = 73, byte-identical upstream |
| `.agents/skills/<nama>/` | Mirror identik (74) |
| `agent-workspace/skills-mattpocock/` | DIHAPUS saat wipe v1 — salinan penuh terakhir ada di tag `pre-v1-wipe` |

## Daftar 37 skill upstream (disync)

ask-matt, claude-handoff, code-review, codebase-design, diagnosing-bugs,
domain-modeling, git-guardrails-claude-code, grill-me, grill-with-docs,
grilling, handoff, implement, implement-spec, improve-codebase-architecture,
loop-me, migrate-to-shoehorn, prototype, research, resolving-merge-conflicts,
retro, scaffold-exercises, setup-matt-pocock-skills, setup-pre-commit,
setup-ts-deep-modules, tdd, teach, to-questionnaire, to-spec, to-tickets,
triage, wait-what, wayfinder, wizard, writing-beats, writing-for-agents,
writing-fragments, writing-shape

## Daftar 36 skill custom (JANGAN dioverwrite saat sync)

brainstorming, context7-cli, context7-mcp, dispatching-parallel-agents,
executing-plans, finishing-a-development-branch, govern-pr, graphify,
llm-council, mem-search, prompt-master, receiving-code-review,
release-readiness, requesting-code-review, runtime-governance-review, strix,
strix-verify, strix-wire, subagent-driven-development, systematic-debugging,
task-observer, test-driven-development, understand, understand-chat,
understand-dashboard, understand-diff, understand-domain, understand-explain,
understand-figma, understand-knowledge, understand-onboard,
using-git-worktrees, using-superpowers, verification-before-completion,
writing-plans, writing-skills

## Cara update ke versi terbaru (berikutnya)

```bash
rm -rf /tmp/mattpocock-latest
git clone --depth 50 https://github.com/mattpocock/skills /tmp/mattpocock-latest
# Salin tiap skill upstream ke .claude/skills/<nama>/ dan .agents/skills/<nama>/
# (hanya 37 nama di daftar atas — skill custom jangan disentuh),
# verifikasi dengan diff -rq, update manifest ini.
```
