> Historical design/implementation notes. For current capabilities and gates, see [project status](PROJECT_STATUS.md) and [architecture](ARCHITECTURE.md).

# Agent 01 | Plugin Runtime and State | HISTORICAL WAVE 1 PROMPT

> The initial plugin implementation is already integrated. This file is retained as its original assignment record. For any current follow-up, use the canonical instructions in `docs/beta-plan/02-PLUGIN-AGENT.md`.

You are Rustie's plugin engineering agent. Use ONLY isolated worktree:
  /home/developer/Projects/rust-daw-agent-plugins (branch beta/plugins).
Read AGENTS.md, docs/beta-plan/{00-ROADMAP,08-AGENT-CONTRACT}.md,
docs/agent-reports/2026-11-20-beta-five-blocker-hardening.md, relevant source and Cargo dependencies.
Verify your HEAD matches the coordinator's BASE_SHA before editing.

## Mission
Turn existing isolated realtime CLAP/VST3 AUDIO-effect bridge into a usable plugin-hosting path for **tested audio FX + realtime MIDI instruments**, with native plugin-state save/restore, bounded failure and honest compatibility reporting. Build on existing code, do not replace it blindly.

## Owned implementation
- crates/daw-plugin-host/src/{clap_stream,vst3_stream,main}.rs and focused helper modules.
- crates/daw-host-client/src/lib.rs as needed for transport.
- crates/daw-engine/src/live_plugin.rs (no block/lock in callback).
- crates/daw-ui/src/studio_live_plugins.rs and optionally a NEW dedicated plugin state module.
- Requests to change daw-core/src/model.rs or central studio.rs/graph.rs must be minimal, backwards-compatible, documented in handoff for integrator; avoid changing other agents' files.
- Keep current offline CLAP/VST2/VST3 render modes working and do not rebrand VST2 offline as realtime support.

## Ordered tasks
1. Reverse-engineer existing per-track effect bridge contract (block size, channel count, worker handshake, timeout, fallback, declared latency) and test its crash/timeout behavior.
2. Add bounded sample/block-timestamped MIDI event packets to the realtime CLAP and VST3 **instrument** worker path. Support note-on/off and musical scheduling (including all-notes-off/voice cleanup on transport stop and worker restart). No direct plugin execution in audio callback.
3. Add explicit plugin descriptor/class selection (multi-class bundles), format, version, port/bus capabilities and sample-rate configuration. Reject unsupported layouts predictably. Diagnose installed LSP VST3 class metadata issue without weakening integrity checks or writing into system plugin paths.
4. Save/restore native plugin state/preset blobs via a versioned, size-limited project representation and worker messages; preserve old projects with serde defaults. Missing plugin, unsupported state or malformed data must not destroy project clips or other devices.
5. Isolated plugin worker failure recovery: bounded timeouts, no unbounded growth, deterministically reported fallback/dropout, cleanup on remove/replace, no stuck child processes. On realtime instrument failure output safe silence, not random stale notes.
6. Confirm latency accounting (bridge buffer + plugin reported latency where known), test routing/group/transport/bypass correctness. Publish what is compensable and what is not.
7. Test at least one installed CLAP audio effect, one installed VST3 audio effect and one CLAP/VST3 MIDI instrument with real output, using safe generated input, worker probes and a project save/reopen case. If plugin state API cannot be fully supported, clearly report blocker and do not falsely declare complete.

## Explicit acceptance
- One nontrivial MIDI note sequence reaches external instrument and produces timed nonzero audio; note-off suppresses lingering sound.
- Tested CLAP and VST3 effect paths process audio (not only scan/discover); current LSP CLAP and Surge XT Effects VST3 probes do not regress.
- Two cold launches with a project preserve plugin identity and the tested parameter/preset state.
- Killing/unplugging a worker in a test cannot block audio callback or crash DAW. Count fallbacks, deadline misses and recovered vs unrecovered cases.
- No false claim of “full VST3 compatibility”; list tested classes, formats, unsupported features and plugin-specific errors.
- Run all shared contract checks plus targeted integration tests. No proprietary plugin redistribution.

## Deliverables
Commit feature work to beta/plugins; report at docs/agent-reports/YYYY-MM-DD-beta-plugin-report.md with exact test commands, probe outputs, IPC schema/protocol changes, migration instructions, latency limitations, test matrix and integrator merge notes. Do not merge into another branch or touch another agent's worktree.
