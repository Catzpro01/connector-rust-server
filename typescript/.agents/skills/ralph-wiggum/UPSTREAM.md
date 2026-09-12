# Upstream provenance: ralph-wiggum

- Source: https://github.com/anthropics/claude-code (official plugin)
- Path: plugins/ralph-wiggum (plugin v1.0.0)
- Commit: fb63c4ad01610862351acc7c17ae38f9869c3d27 (fetched 2026-09-10)
- Vendored: 2026-09-10, 8 files verbatim under ./upstream/
- License: see upstream repo.

## Update procedure

Sparse-clone plugins/ralph-wiggum at new HEAD, replace ./upstream/ content,
re-chmod +x the two .sh scripts, update this file. SKILL.md (repo-local
adapter) stays unless the loop protocol itself changes.
