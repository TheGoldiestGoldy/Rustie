# Agent 04 | Session Grid, Clip-Slot Persistence and Live Timing | WAVE 2, PARALLEL

## Exact worktree and base

Use only `/home/developer/Projects/rust-daw-agent-session`, branch `beta/session`. Its verified clean starting SHA is `e6b82a79feeac4158f1918b3f5598916d5bb177c`. This is the deliberate Wave 2 base. **Do not rebase or reset it.** The integration branch has later integration-only commits; Agent 07 will merge your commit into the latest integration tip.

Read the canonical instructions from `/home/developer/Projects/rust-daw-agent-integration/docs/beta-plan/` because your worktree may contain older copies. Also read `AGENTS.md`, `docs/SESSION_ENGINE.md`, `docs/FEATURE_MATRIX.md`, the current model and Session source, and the Session pre-integration audit.

## Preflight findings to resolve

The preflight audit found: Session slots are not independent of Arrangement clips; scene launch is not atomic; MIDI scheduling depends on GUI repaint/lookahead; switching views can restart the graph; launch quantization is fixed rather than configurable. No implementation or commit was made during that audit. Reconfirm each finding against current source before changing it.

## Mission and ownership

Make Session a persistent, independent musical workspace. Own `crates/daw-ui/src/studio_session.rs`, Session-specific helpers, `docs/SESSION_ENGINE.md`, and narrowly scoped backwards-compatible Session model additions. Do not change plugin IPC, recording code, Devices UI/theme or native FX editors. `studio.rs`, `graph.rs`, `engine.rs` and `daw-core/src/model.rs` are collision hotspots; minimize edits and send Agent 07 an explicit API/schema contract.

## Ordered implementation

1. Trace current looped audio sources, rolling MIDI schedule, one active clip per track, queued bar launch, 128-frame ramps, shared Arrangement clip-index model and graph rebuild on view switch.
2. Design stable persisted SessionSlot/Scene IDs and track-slot states, with explicit linked/copied clip semantics. Keep Arrangement `Track.clips` unchanged. Migrate old projects whose Session slots were inferred from clip-list index; test the existing four-track review project.
3. Implement individual launch/stop, scene launch, stop-all, replace/cancel queued launch and configurable musical quantization using a stable engine-frame boundary.
4. Make scene launch atomic: all target commands apply at the same boundary, or the operation rolls back/retries. Queue-full must never leave a half-launched scene.
5. Remove GUI repaint as the deadline clock where feasible. Otherwise use bounded lookahead, missed-deadline telemetry and deterministic retry; document residual risk.
6. Avoid graph restart on view changes when safe. Keep loop boundaries click-free and loop duration stable through tempo/time-signature changes.
7. Add deterministic multi-track/multi-scene tests at different tempos, signatures and launch boundaries, including note-off/panic behavior.

## Acceptance and delivery

- Session slots/scenes remain distinct from Arrangement clips after save, close and reopen.
- Quantized launches occur at asserted engine frames; scene queue failure cannot partially apply.
- Old project JSON migrates without losing Arrangement clips, plugin state, track/group routing or IDs.
- No realtime callback allocation/blocking and no regressions to mixer, groups or MIDI monitor.
- Run all shared checks and Session-specific tests using a dedicated `CARGO_TARGET_DIR`.
- Commit to `beta/session`; report `docs/agent-reports/YYYY-MM-DD-beta-session-report.md` with schema examples, migration tests, API changes, exact results and integrator notes. Do not merge or push.
