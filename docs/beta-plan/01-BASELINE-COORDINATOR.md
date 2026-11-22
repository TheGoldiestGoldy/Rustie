# Agent 00 | Baseline Coordinator | COMPLETED FOR BETA 1

> **Current status:** COMPLETE. The approved baseline is `1ae60bb2106b595e7e46c33911753ee45eb6d6f6`. Wave 1 is integrated and the two Wave 2 worktrees already exist at `e6b82a79feeac4158f1918b3f5598916d5bb177c`.
>
> **Do not run this procedure again for the current Beta 1 cycle.** In particular, do not snapshot or branch from the dirty original `master` checkout. Use [00-ROADMAP.md](00-ROADMAP.md) and [10-AGENT-DISPATCH.md](10-AGENT-DISPATCH.md) for the current dispatch state.
>
> This file is retained as the one-time baseline procedure for a future, explicitly authorized new development cycle. The procedure below is historical and does not override the current roadmap.

## Historical mission

Establish a safely captured, reproducible Git baseline and isolated worktrees. Do not implement product features in this assignment.

## Historical procedure for a future cycle

1. Confirm no other agents are writing; inspect branch, HEAD, status including untracked files, active processes, Cargo workspace and current reports.
2. Inventory modified/untracked paths. Exclude generated binaries, user projects, personal recordings, plugin binaries, credentials and anything unsafe to commit. Preserve all user files.
3. Obtain owner authorization before creating a new baseline commit. Never reset, clean, stash or overwrite inherited work to manufacture a clean tree.
4. Run format, workspace tests, strict Clippy, release UI/worker build and `git diff --check`. Record failures exactly.
5. Record the baseline SHA and intended file manifest. Keep excluded user assets safe and document them.
6. Create one branch/worktree per specialist from the exact approved SHA. Verify every worktree starts at that SHA and is clean.
7. Create later-wave worktrees only from the integrator's published checkpoint SHA, never from an older base.
8. Save a dated baseline report with included/excluded files, SHA, commands/results and worktree table.

## Acceptance

No specialist starts from a stale or dirty source snapshot. Provide exact dispatch paths and SHAs. Stop after baseline setup; the coordinator does not implement features or merge specialist code.
