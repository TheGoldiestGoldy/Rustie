# Rustie Agent Report: 2026-11-21 / recording and capture reliability

> **Status:** Implemented with human hardware acceptance still pending. Synthetic WAV capture and workspace checks pass. Physical microphone monitoring and GUI save/reopen were not performed.
> **Workspace:** `/home/developer/Projects/rust-daw-agent-recording`
> **Agent and date:** Codex recording/latency pass, 21 November 2026

## 1. What I implemented

| Area | Exact files | Change | Status |
| --- | --- | --- | --- |
| Capture backend | `crates/daw-ui/src/studio_recording.rs`, `crates/daw-ui/Cargo.toml`, `Cargo.lock` | Use a pinned PulseAudio capture stream with 512-frame fragments, timestamp freshness gating, explicit source IDs, callback completion tracking, and bounded ring handoff to the WAV writer. | Synthetic tests pass; physical user input test pending |
| WAV lifecycle | `crates/daw-ui/src/studio_recording.rs` | Drain published frames on stop, report queue drops, reject empty or failed takes, retain partial files with an explicit `.partial` suffix, and validate final WAV metadata. | Focused writer tests pass |
| Recording controls | `crates/daw-ui/src/studio.rs`, `crates/daw-ui/src/studio_devices.rs`, `crates/daw-ui/src/studio_session.rs` | Track selected device by stable device ID, disable source switching while recording, and block project/view changes while a WAV take is active. | Compiles; live GUI interaction not tested |
| Transport probe | `crates/daw-ui/src/main.rs` | Add callback-observation timing for play, seek, stop, and resume in the transport probe. | Compiles; physical acoustic onset not measured |

The recording path adds the `pulseaudio` crate dependency. It does not change project serialization or engine/plugin IPC. `studio.rs`, `main.rs`, and `studio_session.rs` are shared integration hotspots. No Session model or plugin worker protocol was changed.

## 2. Current state and evidence

The ten-second synthetic recording test writes and reopens a stereo, 32-bit float WAV. It checks the header, duration/sample count, non-silent transient, exact expected transient frame, and saved project reference. Temporary test audio is removed. No personal microphone content was recorded or saved.

Timing evidence is limited to synthetic frame placement and software-reported timing. Physical capture-to-playback round-trip latency, monitor feedback, headphone audibility, and a GUI save/reopen take were not measured. A physical recording probe was not run because the shared contract prohibits recording user input without explicit permission.

## 3. Verification

| Check | Exact command or procedure | Observed result |
| --- | --- | --- |
| Rust format | `cargo fmt --all -- --check` | Passed |
| Workspace tests | `cargo test --workspace` | Passed, zero failures; includes 19 core, 36 engine, and 34 UI tests, plus parser, host-client, and plugin-host suites |
| Focused recorder tests | `cargo test -p daw-ui recorder_tests -- --nocapture` | 11 passed, including ten-second synthetic WAV, writer tail drain, overrun accounting, `/dev/full` failure handling, and source selection |
| Clippy | `cargo clippy --workspace --all-targets -- -D warnings` | Passed |
| Release | `cargo build --release -p daw-ui -p daw-plugin-host` | Passed |
| Whitespace | `git diff --check` | Passed |
| Physical input and UI | No physical recording or live GUI take was made | **BLOCKER FOR HUMAN ACCEPTANCE** |
| Acoustic timing | No reference loopback or acoustic measurement | **BLOCKER FOR HUMAN ACCEPTANCE** |

## 4. Known issues and limitations

- A human must test a real microphone or guitar take, verify input routing, monitoring audibility/feedback, stop behavior, waveform display, project save/reopen, playback, and export.
- Software timing predictions do not establish acoustic round-trip latency. The transport callback probe also measures engine/sink callback observation, not audible onset.
- Source disappearance and device unplug recovery are guarded in code but were not exercised against hardware in this pass.
- Count-in ergonomics remain unimplemented.

## 5. What remains

- **P0:** Human test with a non-private test signal or explicit user authorization for a physical mic take. Save, reopen, inspect sample rate/channels/frame count, audition, and export the take. Record device and measured results without retaining personal audio.
- **P1:** Measure physical input-to-output latency only with a real electrical/acoustic loopback reference. Label predictions separately from measurements.
- **P2:** Add selectable one/two-bar recording count-in if the integrated UI flow can do so without baking the click into the take.

## 6. Git state and deliverables

- Branch: `beta/recording`
- Starting HEAD / observed worktree base: `b261252e9eed1a10b8f180496dd1095ba777544d` (`Document recording beta verification and human blockers`).
- This worktree was already dirty at entry with the seven listed modified files. They were preserved and validated as the active recording branch work, then committed together with this report.
- Commit: `fda7b3b2ad28ab65b6b2592d327b97541b24bffb` (`Harden recording capture and WAV take lifecycle`).
- Pushed: No.
- No recordings, project files, device configuration, or plugin binaries were added.

## 7. Next-agent handoff

Integrate the `beta/recording` commit after reviewing the narrow changes in the shared hotspots `studio.rs`, `main.rs`, and `studio_session.rs`. Keep the selected-input field as `Option<cpal::DeviceId>` consistent with device enumeration. No persisted schema migration is involved. Before claiming physical acceptance, perform the P0 checks above and retain only aggregate WAV metadata and timing results. The initial baseline coordinator SHA was not present in the shared contract, so this report records the branch HEAD actually inspected before changes.
