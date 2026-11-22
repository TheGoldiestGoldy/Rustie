# Agent 01 | Plugin Runtime Hardening | WAVE 2.5, CONDITIONAL

**Dispatch only after Wave 2 is integrated.** The initial plugin worker, realtime CLAP/VST3 effects and instruments, versioned state blob and worker-level state round-trip are already merged. Do not restart or rewrite that implementation.

## Worktree and entry gate

The integrator must create a fresh isolated worktree from the then-current integration SHA:
- Path: `/home/developer/Projects/rust-daw-agent-plugin-hardening`
- Branch: `beta/plugin-hardening`
- Base: exact `PLUGIN_BASE_SHA` published by Agent 07 after Wave 2.

Read the canonical roadmap and contract from `/home/developer/Projects/rust-daw-agent-integration/docs/beta-plan/`, plus `AGENTS.md`, `docs/FEATURE_MATRIX.md`, the plugin reports and current source. Confirm branch, HEAD, clean status and base SHA before editing.

## Mission and owned files

Close evidence-backed plugin reliability gaps without replacing the existing isolated worker architecture. Primary ownership: `daw-plugin-host`, `daw-host-client`, `daw-engine/src/live_plugin.rs`, `daw-ui/src/studio_live_plugins.rs`, and focused tests. Core model, `studio.rs`, `graph.rs` and `Cargo.lock` are collision hotspots; coordinate narrow changes with Agent 07. Do not edit Session or Devices UI.

## Ordered tasks

1. **GUI state round-trip:** prove that explicit project save captures native state, the serialized project contains the versioned bounded blob, cold reopen restores the same plugin identity and state, and a second save remains valid. Test CLAP and VST3 instruments/effects where supported. Determine and document autosave behavior; do not claim it saves state unless it calls the capture path.
2. **MIDI expression:** add bounded, sample-offset channel, sustain CC64 and pitch-bend forwarding where the plugin APIs support it. Preserve note-on/off and panic behavior. Define overflow behavior that cannot leave stuck notes.
3. **VST3 metadata:** investigate the installed LSP VST3 `moduleinfo.json` class-ID collision. Fix valid metadata handling without weakening class identity checks or accepting ambiguous/malformed bundles.
4. **Worker resilience:** test worker kill, crash, hang, malformed/oversized state, queue saturation, remove/replace and restart. Prove bounded recovery, no zombie processes, no audio callback blocking and no project corruption.
5. **Resource limits:** characterize multiple simultaneous instances, CPU/RAM growth, polyphony and session/arrangement graph teardown. Add explicit safe limits or diagnostics if evidence requires them.
6. Update the tested plugin compatibility matrix with exact bundle, class, version, format and probe result. Do not promise every CLAP/VST3 vendor or native editor support.

## Acceptance and handoff

- GUI save → close → cold reopen → plugin state restore passes for named test plugins, or a precise reproducible blocker is reported.
- Note-off, all-notes-off, sustain release and pitch bend are verified with event-level tests and real plugin probes where possible.
- Invalid plugin identity/state fails safely; existing valid and legacy projects still load.
- No audio-thread allocation, lock, blocking IPC, file I/O, process spawn or direct plugin invocation.
- Run the shared contract gate and focused probes from this worktree's own binaries.
- Commit only to `beta/plugin-hardening`; write `docs/agent-reports/YYYY-MM-DD-beta-plugin-hardening-report.md` with exact SHA, tests, compatibility matrix, protocol/schema changes and integrator instructions. Do not merge or push.
