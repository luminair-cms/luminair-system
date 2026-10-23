# Gemini Agent — Overrides

> Read [`AGENTS.md`](../../AGENTS.md) first. This file contains **Gemini-specific additions only**.

## Context Loading Order

Gemini automatically indexes the workspace. At session start, ensure loaded:
1. `AGENTS.md`
2. `.ai/context/project.md`
3. Skill relevant to the task (e.g. [`.ai/skills/clean-code.md`](../skills/clean-code.md))

## Tool Hints

- Prefer surgical edits over full-file rewrites.
- Use search tools to verify codebase symbols before making edits.
