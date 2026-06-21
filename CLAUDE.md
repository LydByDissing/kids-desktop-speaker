# Project Instructions for AI Agents

This file provides instructions and context for AI coding agents working on this project.

<!-- BEGIN BEADS INTEGRATION v:1 profile:minimal hash:6cd5cc61 -->
## Beads Issue Tracker

This project uses **bd (beads)** for issue tracking. Run `bd prime` to see full workflow context and commands.

### Quick Reference

```bash
bd ready              # Find available work
bd show <id>          # View issue details
bd update <id> --claim  # Claim work
bd close <id>         # Complete work
```

### Rules

- Use `bd` for ALL task tracking — do NOT use TodoWrite, TaskCreate, or markdown TODO lists
- Run `bd prime` for detailed command reference and session close protocol
- Use `bd remember` for persistent knowledge — do NOT use MEMORY.md files

**Architecture in one line:** issues live in a local Dolt DB; sync uses `refs/dolt/data` on your git remote; `.beads/issues.jsonl` is a passive export. See https://github.com/gastownhall/beads/blob/main/docs/SYNC_CONCEPTS.md for details and anti-patterns.

## Agent Context Profiles

The managed Beads block is task-tracking guidance, not permission to override repository, user, or orchestrator instructions.

- **Conservative (default)**: Use `bd` for task tracking. Do not run git commits, git pushes, or Dolt remote sync unless explicitly asked. At handoff, report changed files, validation, and suggested next commands.
- **Minimal**: Keep tool instruction files as pointers to `bd prime`; use the same conservative git policy unless active instructions say otherwise.
- **Team-maintainer**: Only when the repository explicitly opts in, agents may close beads, run quality gates, commit, and push as part of session close. A current "do not commit" or "do not push" instruction still wins.

## Session Completion

This protocol applies when ending a Beads implementation workflow. It is subordinate to explicit user, repository, and orchestrator instructions.

1. **File issues for remaining work** - Create beads for anything that needs follow-up
2. **Run quality gates** (if code changed) - Tests, linters, builds
3. **Update issue status** - Close finished work, update in-progress items
4. **Handle git/sync by active profile**:
   ```bash
   # Conservative/minimal/default: report status and proposed commands; wait for approval.
   git status

   # Team-maintainer opt-in only, unless current instructions forbid it:
   git pull --rebase
   git push
   git status
   ```
5. **Hand off** - Summarize changes, validation, issue status, and any blocked sync/commit/push step

**Critical rules:**
- Explicit user or orchestrator instructions override this Beads block.
- Do not commit or push without clear authority from the active profile or the current user request.
- If a required sync or push is blocked, stop and report the exact command and error.
<!-- END BEADS INTEGRATION -->


## Build & Test

_Add your build and test commands here_

```bash
# Example:
# npm install
# npm test
```

## Architecture Overview

_Add a brief overview of your project architecture_

## Conventions & Patterns

_Add your project-specific conventions here_

---

## Project Ground Rules

These rules are **mandatory** for all agents and contributors. They override general defaults.

### Skills to Use

| Task | Skill |
|------|-------|
| Documentation, architecture, C4 diagrams, requirements | `/docs-skill` |
| All other implementation and research work | `/llm-dsl` |

Never use ad-hoc markdown files or TodoWrite for task tracking or documentation. Always invoke the correct skill.

### Pre-Code Gate (hard stop)

No code may be written for a component unless **all three** of these exist first:

1. **Requirements** — documented via `/docs-skill` (as a spec, ADR, or requirements section in the Sphinx docs)
2. **C4 Component** — the component is mapped and documented in the C4 model via `/docs-skill`
3. **Beads issue** — a `bd` issue exists, is claimed, and references the requirements and C4 component

If any of these is missing, stop and create it before writing implementation code.

### Test Mandate

Every code change **must** include tests. No exceptions.

- Write the test file alongside the implementation file, in the same PR/commit
- Tests must pass before closing any beads issue or handing off
- Run the test suite as part of the session close protocol
- "Code without tests" is treated as incomplete work — do not close the issue

### Enforcement Checklist (session close)

Before marking any implementation issue done:

```
[ ] Requirements exist in docs (docs-skill)
[ ] C4 component is documented (docs-skill)
[ ] Beads issue references requirements + C4 component
[ ] Tests written and passing
[ ] bd close <id> run
[ ] git status clean (or staged changes reported)
```
