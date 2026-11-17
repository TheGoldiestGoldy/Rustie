# Project status and evidence register

**Snapshot:** 2026-11-17, committed baseline `f578b9a`; original branch `master`. The UI and documentation agents are editing the original worktree concurrently. This scaffold was created in a separate branch, `docs/github-scaffold-20261117`, to avoid overriding those edits.

## Status vocabulary

- **Verified in source:** Confirmed by reading checked-in Rust code; may still fail in a particular device or build.
- **Historically tested:** Described as tested in commit-time notes (2026-11-16); not independently re-benchmarked during this documentation pass.
- **In progress:** Observed uncommitted work by a separate agent, explicitly not part of this Git snapshot.
- **Prototype:** A usable partial path that does not meet production acceptance criteria.
- **Absent:** No functional implementation was identified.

## Inventory

| Subsystem | Evidence | Status |
| --- | --- | --- |
| Workspace | Root `Cargo.toml`, six workspace members, edition 2024, Rust 1.85 minimum | Verified in source |
| Musical clock | `daw-core/src/time.rs`: 960 PPQ, TempoMap with sample-frame conversion | Verified in source |
| Scales | `daw-core/src/scales.rs`: twelve modes, MIDI pitch membership, scale fold math | Verified in source |
| Project model | `daw-core/src/model.rs`: tracks, clips, MIDI notes, audio paths, mixer flags | Verified in source |
| Undo/redo core | `daw-core/src/commands.rs` command stack, NOT yet comprehensive UI integration | Verified in source |
| Graph | `daw-engine/src/graph.rs`: DAG routing, buffers, PDC for declared latency | Verified in source |
| Playback | `daw-engine/src/engine.rs`: queue, scheduled commands, MIDI voice, gain ramps | Verified in source |
| Native output | `daw-engine/src/driver.rs`: CPAL ALSA/PulseAudio/JACK, 44.1/48/96 kHz validation | Verified in source |
| Audio decoding | `daw-engine/src/audio_file.rs`: Symphonia decode and simple linear resampling | Verified in source |
| Arrangement | `daw-ui/src/studio_views.rs`: mixer headers, clip interactions, ruler | Prototype |
| MIDI editor | `daw-ui/src/studio.rs`: root/mode, fold, editing | Prototype |
| Session view | `daw-ui/src/studio_views.rs`: grid drawing and selection, no launch | Prototype |
| Project read/write | `daw-ui/src/studio.rs`: JSON save/open using serde_json | Prototype |
| Asset browser | `studio_browser.rs` seen uncommitted in live worktree, with sample scan / plugin discovery | In progress, not merged in baseline |
| CLAP/VST3/LV2 execution | Placeholder crates; no functional plugin ABI/worker bridge | Absent |
| Audio recording | No input stream / record arm path | Absent |
| MIDI hardware | No MIDI device I/O | Absent |
| .als compatibility | `daw-parser` placeholder, no importer | Absent |

## Relevant existing commits

1. `007a15d` (2026-11-16) Implement native Rust DAW core and Phase 2 audio engine
2. `48a1eb8` (2026-11-16) Route audio probe through native PulseAudio and expose output level
3. `f578b9a` (2026-11-17) Improve Slate Studio arrangement UI and low-latency playback

These are original local repository commits; the clean public GitHub snapshot may not preserve those Git SHA objects or their commit ancestry.

## Historical machine-side checks

Previous docs record 33 passing tests, strict Clippy, formatting checks, native ALSA 44.1 kHz and JACK 48 kHz callbacks as of Nov 16. They also record an audible-playback-routing fault and a report of a noticeable visual/audio playback delay. **Revalidate after each engine/driver change**. No current xrun histogram, loopback latency, MIDI timing variance, high-track-count stress benchmark or plugin crash-recovery result exists.

## Known documentation drift

The original architecture notes had incorrectly called Phase 5 UI "NOT implemented"; this documentation branch corrects it to **prototype implemented**. The baseline contains a graphical desktop app, but many professional workflows are absent. Keep architecture and status documents in sync after each merge.

## Updating this document

On every significant merge: record commit SHA/date, feature acceptance proof, exact tests, machine/driver, reproduction steps for residual defects and any file-format changes. Avoid unqualified "fully working" claims.
