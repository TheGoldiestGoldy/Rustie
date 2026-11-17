# Multi-agent coordination and handoff

This is the canonical protocol for parallel UI, audio, parser and documentation agents. Other agents were already changing the same local code during the Nov 17 documentation snapshot.

## Rules

1. **Never work directly over an unknown dirty worktree.** Check `git status --short -b`, remotes, branch, and recent commits.
2. Use a unique branch/worktree per task. No simultaneous writes to the same path, no force push, no blind reformat of another agent's files.
3. Document the baseline commit SHA at start and head SHA at delivery. Source inspection proves existence, not a successful run.
4. List *only* real implemented features in the status table. Describe prototypes and placeholders clearly.
5. Any change to the audio callback needs no-allocation tests and a measurable transport/latency regression check.
6. Screenshots and a slick panel do not make disabled controls functional. Make disabled state explicit.
7. No external binary, paid plugin, proprietary artwork, purchased samples, personal paths or credentials without rights and review.
8. Create small, reviewable commits. Keep docs in sync with code before merging.
9. Do not change OS-wide audio profiles or close user apps while testing without explicit permission.
10. Merge into shared branch only after verifying integration against newest source. **Never claim code was pushed if it exists only locally.**

## Handoff artifact template

```markdown
### Workstream
UI / Audio / Plugin / Parser / Docs / Build

### Baseline and changes
Base commit:
Head commit:
Branch / worktree:
Changed files:
Dependencies added:

### Implemented and user-visible
- Implemented:
- UI controls that work:
- UI controls not wired yet:

### Verification
- cargo fmt:
- cargo clippy:
- cargo test:
- Hardware smoke test (backend, machine, output):
- Screenshots or screen recording:
- Benchmarks (method, not impressions):

### Risks and next actions
- Known regressions:
- Files changed concurrently:
- Integrations/migrations needed:
- Secret/IP review:
- Next owner/action:
```

## Suggested ownership split

- UI agent: `crates/daw-ui` and screenshots; coordinate with model changes.
- Engine agent: `crates/daw-engine` audio callback, graph and measurements.
- Domain agent: `crates/daw-core` and format version migrations.
- Plugin agent: `daw-plugin-host`, `daw-host-client` and controlled integration points.
- Docs agent: `docs/` and root metadata, merging after another author's changes.

## Delivery definition

A workstream is integrated only when it has code, a reproducible demonstration, fitting tests, a commit, and a reviewed merge. "Agent wrote it" is not a verification category.
