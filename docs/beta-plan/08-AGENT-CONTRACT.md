# Rusty Beta | Shared contract for every agent

This contract applies to Agents 00–06. Canonical plan directory: `/home/developer/Projects/rust-daw-agent-integration/docs/beta-plan/`.

## Branch and worktree safety

- Original `/home/developer/Projects/rust-daw` on `master` is dirty and must remain untouched. Never branch from it, reset it, clean it, stash it or use it as a build directory.
- Every specialist gets one separate sibling worktree and branch. Never let two agents write to the same path or branch.
- Before editing, print `pwd`, `git branch --show-current`, `git rev-parse HEAD`, `git status --short --branch`, and compare with the exact base SHA in your prompt.
- Session and UI Wave 2 branches intentionally start at `e6b82a79feeac4158f1918b3f5598916d5bb177c`. Do not rebase/reset them. Agent 07 merges their commits into the latest integration tip.
- Only Agent 07 writes/merges in `beta/integration`. Specialists commit to their own branch and stop. No agent pushes to GitHub or publishes releases.
- Read the canonical prompt from the integration worktree path; an older copy in a specialist branch may be stale.

## Ownership boundaries

- Agent 01 Plugin: plugin host/client, live plugin bridge, worker protocol and focused tests.
- Agent 02 Recording: recording capture/writer, monitor and input-specific tests. Wave 1 complete.
- Agent 03 Editing: clip operations, piano editing and history. Wave 1 complete.
- Agent 04 Session: Session slots/scenes, scheduling, migration and Session docs.
- Agent 05 UI: Devices/browser/help/theme and narrow UI-only integration points.
- Agent 06 Integrator: cross-module conflict resolution, broad model/graph/driver edits, packaging, versioning and final QA.
- Collision hotspots: `studio.rs`, `studio_views.rs`, `daw-core/src/model.rs`, `daw-engine/src/engine.rs`, `graph.rs`, `driver.rs`, `Cargo.lock`. Touch only when required; minimize the patch and document the API/schema contract.

## Engineering and compatibility rules

- Preserve old project JSON, serialized IDs, existing `.slate.json` files, plugin paths and user media. Add serde defaults and migration tests for schema changes.
- Realtime callbacks must not allocate, lock, block on IPC, perform file/disk I/O, spawn processes or call plugin code. Use bounded queues and explicit overflow/fallback telemetry.
- A worker probe returning PCM does not prove audible playback. A meter moving does not prove a recording. A GUI process surviving timeout is not a mouse test.
- Label every result PASS, FAIL or NOT TESTED and state the exact SHA, command, binary and evidence. Never upgrade a claim because another branch passed a similar test.
- Do not alter OS audio defaults, sound-server profiles, user input selection, real recordings or installed plugin files without explicit owner permission.
- Existing installed plugins are test inputs only. Do not redistribute commercial plugin binaries. Keep licensing and public-asset checks in the release gate.

## Required verification and isolation

Every coding agent runs:
```sh
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo build --release -p daw-ui -p daw-plugin-host
git diff --check
```
Use a unique `CARGO_TARGET_DIR` per worktree (for example `/home/developer/.cache/rusty-target-session` or `...-ui`) and bounded Cargo parallelism. Do not delete another agent's build cache. Run focused tests and relevant probes from your own binaries.

## Required handoff

- Commit SHA, branch, worktree and exact base SHA.
- Files changed, tests/probes and exact observed counts/results.
- Project JSON, IPC, audio timing, dependencies and licensing impact.
- Known defects, untested manual work and reproducible steps.
- A dated report at `docs/agent-reports/YYYY-MM-DD-beta-<role>-report.md` with merge notes for Agent 07.
- No agent except Agent 07 may declare Beta 1 GO/NO-GO.
