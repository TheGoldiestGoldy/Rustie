# Rustie development report: 2026-11-19 / live MIDI keyboard audition

> **Status:** Implemented, compiled, instrument-level tests passing; actual ALSA MIDI port connected successfully. Physical key press / audible speaker validation still outstanding.
> **Repo:** /home/developer/Projects/rust-daw (master, inherited dirty worktree).

## 1. Implemented
- Added native ALSA hardware MIDI input via `midir 0.11.1` to `daw-ui`, with selectable port in **Devices > Hardware MIDI keyboard**, explicit disconnect, and panic/all-notes-off action. The MIDI callback only sends bounded events to the UI/control thread; it never calls audio engine code or allocates audio buffers.
- Routed note-on/note-off and velocity-zero note-off to the selected MIDI track, remembering the original note-to-node mapping when the selection changes. Handles MIDI channel panic (CC 120/123). The synthesizer remains the existing 16-voice diagnostic sine synth, not a production plugin.
- Added `EngineCommand::SetMonitoring(bool)` and MIDI-only graph rendering when transport is stopped: MIDI synth audio passes through group/master buses, but audio clips do not sound and the project clock does not move.
- In Session, preloaded a dedicated live-monitor MIDI node per MIDI track, separate from bar-queued clip nodes. Increased graph node capacity from 64 to 128 so this does not invalidate ordinary large Session layouts.
- Added `--probe-midi [port-name-substring]` CLI: enumerates MIDI ports and optionally opens/closes a real port without storing incoming notes. Corrected `--probe-input` to report the actual selected output rate rather than hardcoded 48 kHz; corrected recording failure diagnostics similarly.
- Added tests for message parsing and stopped-transport audio monitoring (including audio-clip exclusion and fixed project clock).

## 2. Current state and visuals
- Rebuilt binary: `target/release/daw-ui`.
- Devices dialog includes new MIDI keyboard section. **No screenshot or human mouse test of the new binary**: connected GNOME Wayland session does not provide usable direct GUI capture/click automation.
- Existing `daw-ui` process was left running, so the user must **close and relaunch** Rustie to load the new binary. No project/source WAVs were overwritten.

## 3. Verification
| Check | Result |
| --- | --- |
| `cargo test --workspace --quiet` | PASS: 61 tests across the workspace; no failures |
| `cargo clippy --workspace --all-targets -- -D warnings` | PASS |
| `cargo fmt --all -- --check` and `git diff --check` | PASS |
| `cargo build --release -p daw-ui` | PASS |
| `target/release/daw-ui --probe-midi 'AIR 192 6'` | PASS: port enumerated and actual ALSA MIDI input opened/closed |
| `target/release/daw-ui --probe-input` | PASS: compatible capture formats advertised at selected 44.1 kHz; no microphone audio captured |
| `target/release/daw-ui --probe-ui-rate` | 44,100 Hz |
| `RUSTIE_PROBE_RATE=44100 target/release/daw-ui --probe-transport 128 650` | PASS: 205 callbacks, project frame 204972, predicted playback delay 10.55 ms (NOT measured hardware latency) |
| `target/release/daw-ui --check-project '~/Music/Rustie UX Review/Rustie UX Review.slate.json'` | PASS: 4 tracks, 8 clips, 2 decoded audio files |
| Real keyboard-to-headphones, real voice recording and new GUI visual click | NOT TESTED |

## 4. Limitations
- MIDI input **plays** notes; it does not record MIDI performances, persist port choice across app restarts, capture sustain pedal/CC, implement pitch bend, or route to external VST3/CLAP synths.
- MIDI callback uses a bounded 1024-event queue and UI frame polling (up to 256 events/frame); it is not proven low-latency under real keyboard load. Hot-unplug requires reconnecting in Devices.
- Live VST3/CLAP hosting, plugin state recovery, reliable measured recording latency, speaker-onset measurement and manual visual testing remain open.
- Running app process predates new release. No audio-device profiles/defaults changed.
- Global `MAX_NODES=128` is a fixed prototype bound, not unlimited tracks or guaranteed performance.

## 5. Priorities
1. **Human acceptance:** fully close/reopen Rustie, open Devices, select `AIR 192 6 ... MIDI 1`, select a MIDI track, play notes with transport **stopped** and **playing**, test mute/solo and Session view. Confirm no unrelated audio loops or stuck notes. Test ear-level onset.
2. Perform a short mic WAV take and save/reopen using the current 44.1 kHz interface; quantify audio/input latency and dropped frames, correct alignment.
3. Next substantial engineering milestone: implement a **real isolated CLAP/VST3 live audio/MIDI host** with plugin state persistence and crash containment. Existing offline VST2/VST3 rendering does not meet real-time support.
4. Add MIDI record-to-clip/overdub and CC/sustain handling. Maintain source tests and performance instrumentation.

## 6. Git and artifacts
- Changed: `crates/daw-ui/Cargo.toml`, `Cargo.lock`, `crates/daw-ui/src/main.rs`, `studio.rs`, `studio_recording.rs`, NEW `studio_midi_input.rs`, `crates/daw-engine/src/engine.rs`, `graph.rs` and handoff/docs.
- Branch: `master`. Extensive pre-existing modified and untracked files preserved; **no reset, commit, push or remote changes**. No private media copied or published.
- Build path: `target/release/daw-ui`. CLI probes open no microphone media; the MIDI port probe records no messages.

## 7. Next agent: simple instructions
1. Read `AGENTS.md`, this report, `docs/FEATURE_MATRIX.md`, `crates/daw-ui/src/studio_midi_input.rs` and `crates/daw-engine/src/{engine,graph}.rs`. Run `git status --short` before editing.
2. Retest the user-facing live MIDI workflow on actual keys and headphones; check Session transport stop/start with live monitoring and whether selected MIDI track changes are followed.
3. Implement the next real deliverable, not a mockup: MIDI clip recording or an isolated CLAP/VST3 live-host vertical slice, with tests. Do not conflate file discovery/offline render with realtime plugin hosting.
4. Run Cargo workspace tests, strict Clippy, format, release build; create a NEW factual per-agent report. Preserve the user's WAVs/projects, don't publish or force-push.
