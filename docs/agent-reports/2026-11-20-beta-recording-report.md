# Rustie Beta | Recording Agent 02 | Capture, Monitoring and Clock

> **Date:** 2026-11-20 UTC
> **Status:** Partially verified, code implemented and committed. Hardware and GUI recording acceptance remain BLOCKERS FOR HUMAN ACCEPTANCE.
> **Worktree:** `/home/developer/Projects/rust-daw-agent-recording`
> **Branch:** `beta/recording`
> **Shared base (BASE_SHA):** `1ae60bb2106b595e7e46c33911753ee45eb6d6f6`
> **Recording code commit:** `be614f75e54f6820a193ebdaee32eaf4416a4d75`
> **Report:** This file, committed separately on the same branch. **No merge or push.**

## 1. Scope, inherited work and changes

At start the isolated branch already had **five dirty files**, including ~830 added/changed lines in `studio_recording.rs`, plus new code in `main.rs`, `studio.rs`, `studio_devices.rs`, and the engine no-allocation test. Those uncommitted edits were **inherited**, inspected and validated, not independently credited as newly created in this pass. They became part of the role's code commit because they implement the assigned recording pipeline.

The inherited implementation includes: a CPAL PulseAudio capture source filtered to exclude output-monitor devices, explicit failure for a missing named input, a fixed requested 512-frame capture buffer, a capture timestamp/freshness gate, a bounded ring feeding a separate 32-bit float stereo WAV writer, explicit dropped-frame telemetry, a `.wav.partial` file finalized and renamed only on clean completion, predicted first-capture/output-delay alignment, a ten-second device-free synthetic take test and CLI probe, project JSON/decode/waveform-data/export regression, and an engine realtime no-allocation check covering live monitoring plus metronome.

Further hardening performed after inspection:

| Area | Source | Implemented behavior |
| --- | --- | --- |
| Stream failure | `crates/daw-ui/src/studio_recording.rs` | CPAL stream errors set an atomic failure flag. GUI polling stops recording and reports failure, preserves finalized samples if available. No callback-side locks or disk I/O added. |
| Output clock failure | `crates/daw-ui/src/studio.rs` | A watchdog stall during recording now stops the take rather than letting it run while output project time is frozen. |
| Overruns | `studio_recording.rs` | A positive dropped-frame count now raises a user-visible warning that later audio can be temporally shifted; retained WAV is not characterized as loss-free. |
| Transport and save safety | `studio.rs`, `studio_track_management.rs` | Disallow a graph restart, another project opening, project save-in-progress and track deletion during a take; preserve finalized-take status on Stop/Pause. |
| Pre-arm monitoring | `studio_recording.rs`, `studio_devices.rs` | If monitoring is requested while playback is already running, rebuild the graph **before** hardware capture. Disable monitoring checkbox/apply button during the take. |
| Configuration safety | `studio_views.rs` | Tempo, time signature and audio backend selection are disabled while a WAV take is running. Seeking while recording already reports an explicit error. |
| Verification | `crates/daw-engine/tests/rt_no_alloc.rs`, `crates/daw-ui/src/main.rs`, `studio_recording.rs` | Real writer/reader/export samples, device-free 10-second probe, ring overrun, timestamp gate, partial-file behavior, monitoring/metronome realtime-alloc guard. |

**Integration warning:** `studio.rs` is a shared hotspot, `studio_views.rs` and `studio_devices.rs` overlap Agent 06's UI scope, and `studio_track_management.rs` lies outside the primary recording ownership. All changes are narrow, but Integrator 07 must inspect and resolve overlaps with parallel branches rather than wholesale overwriting files.

## 2. Measured evidence and timing table

The final **device-free synthetic** CLI test, using the branch's own release binary, produced:

| Property | Expected | Observed | Interpretation |
| --- | ---: | ---: | --- |
| WAV sample rate | 44,100 Hz | 44,100 Hz | Synthetic clock, not hardware negotiation |
| Frame count | 441,000 | 441,000 | 10.000 s encoded |
| Channels / format | stereo / 32-bit float | stereo / 32-bit float | WAV header and samples verified |
| Injected transient | frame 88,200 | frame 88,200 | Delta **0 frames** |
| Peak | nonzero | 0.90 | Synthetic signal only |
| Project JSON reopening | same audio source and clip | passed | Test round-trip, **not** GUI click-through |
| Decoded audio waveform data | nonzero PCM peaks | passed | No current GUI screenshot |
| WAV mix export | real signal at clip position | passed | In-memory production graph path |
| Output acoustic latency | unavailable | **NOT MEASURED** | Predicted callback delay is not speaker latency |
| Real physical mic/guitar | owner consent/manual routing needed | **NOT TESTED** | No human input saved or heard |

The scripted synthetic probe writes ten seconds of sample material to a disposable temporary WAV and removes it. It does not run a wall-clock ten-second physical recording. The separate ten-second unit test builds a project, serializes/reopens it, decodes samples, derives waveform peaks, and exports to a non-silent WAV. An injected transient is a **synthetic test signal**, not metronome leakage. There has been no physical test of the click bleeding through headphones into a microphone.

The earlier baseline report recorded ~15,082 AIR input frames in 342 ms at 44.1 kHz with zero dropped frames, but this was **previous-session evidence**, **not a new measurement here**. No `--probe-recording` live input capture was run in this pass, since the contract bars unapproved real recordings/input device changes.

## 3. Exact reproducibility and final verification

From the isolated worktree:

```sh
cd /home/developer/Projects/rust-daw-agent-recording
export CARGO_TARGET_DIR=/tmp/rustie-target-recording
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo build --release -p daw-ui -p daw-plugin-host
git diff --check
"$CARGO_TARGET_DIR/release/daw-ui" --probe-recording-writer
"$CARGO_TARGET_DIR/release/daw-ui" --check-project "$HOME/Music/Rustie UX Review/Rustie UX Review.slate.json"
```

**Final observed results (after code edits):** format PASS; **94 workspace tests passed, 0 failed** (19 core, 36 engine, 4 engine realtime, 3 plugin-host, 32 UI); strict Clippy PASS; release UI + worker build PASS; diff whitespace PASS. Synthetic writer probe PASS, with **441,000 frames / 10.000 seconds / 0 transient-frame delta**, 44.1 kHz stereo float. Existing four-track review project preflight PASS: **4 tracks, 8 clips, 2 decoded audio files**. Focused `cargo test -p daw-ui recorder_tests`: 9/9 passed before additional safety changes, and included among final full-workspace tests.

Validation transcript: `/tmp/rustie-recording-final-validation.log` (machine-local, not committed). Built binary: `/tmp/rustie-target-recording/release/daw-ui`. No GUI mouse test, WAV take recorded using GUI, real listening test or new screenshot was performed. Do not confuse a process/format/test pass with those validations.

## 4. Compatibility, safety and limitations

- No JSON schema, persisted ID, plugin worker protocol, audio IPC frame size or third-party dependency was changed. Existing tempo/time-signature and metronome-volume persistence remain intact. Recorded audio clips are written as ordinary project audio clips, and only a successfully finalized WAV is imported.
- The implemented physical recording route is through **PulseAudio CPAL**. JACK/ALSA output choices do not prove JACK/ALSA capture; those routes are not supported by this assignment's hardware tests. The selected named input is pinned, and an absent device fails explicitly rather than silently switching microphones. Automatic selection can still choose the first physical input if the AIR is absent.
- Direct software monitoring remains opt-in and is not guaranteed feedback-safe at every hardware routing configuration. Hardware direct-monitoring may double the sound if enabled at the same time; verify headphones and gain manually. The GUI does not promise zero-latency monitoring.
- File writer errors leave a visibly named `.partial` take rather than a purported complete clip. A missing-parent writer failure and empty recording are tested; actual full-disk, permission loss during writing and power-failure recovery are **not** tested.
- Ring overrun is counted, not concealed. The currently accepted frames are preserved, but dropped frames compress the resulting WAV timeline after the overrun; this is explicitly warned and **not yet sample-gap compensated**.
- Time placement uses CPAL timestamps, first accepted buffer and backend-reported output delay. This is **estimated**, not a real analog round-trip or acoustic onset measurement. A tick-quantized project clip is not sample-exact in position. The old reported audible-playhead lag was not independently measured here.
- There is no safe asynchronous one-/two-bar recording count-in yet. Metronome on/off, time signature and volume persist, but **count-in remains unimplemented**. No claim that click cannot leak acoustically into an open microphone.
- An active capture error, stalled input (>1.5 seconds without fresh frames) or output clock stall now stops a take with warnings. These are source logic and regression coverage, not hardware unplug tests.
- The existing test project's UI actually showing a waveform, saving/reopening a new mic take, and audible WAV export must be manually validated on real equipment before Beta 1 sign-off.

## 5. BLOCKERS FOR HUMAN ACCEPTANCE

1. Select an actual AIR 192|6 **physical input** in Devices and an empty audio track, record at least 10 seconds of voice/guitar, then stop. Check source routing and explicitly confirm the chosen microphone, sample rate, WAV header/duration and any dropped-frame warnings. Do not capture private audio in the repo.
2. Listen to take, waveform and exported mix using headphones; save the project, quit/reopen via the GUI and verify the WAV and clip position survive. Check transient against an independently supplied loopback/reference if available; report measured and predicted latency separately.
3. Test transport Stop, Pause and seek rejection, unplug the named input before arm and during an active take, ensure no unapproved fallback, and deliberately test a writer fault in a disposable path. Exercise monitor off/on **between takes** with hardware direct-monitoring off to avoid a feedback loop.
4. If count-in is required for Beta 1, Integrator 07 should coordinate new pre-roll state machine with the recording and transport agents, include persisted 0/1/2 bar choice and tests, and ensure the software click never enters the WAV writer path.

## 6. Git, privacy and integrator handoff

- Baseline verified: `1ae60bb2106b595e7e46c33911753ee45eb6d6f6`.
- Code commit: `be614f75e54f6820a193ebdaee32eaf4416a4d75` on `beta/recording`. Report committed separately after the code commit.
- The workstation had no configured Git author identity. A **commit-scoped** `Rustie Recording Agent <rustie-recording@local.invalid>` identity was used; no global Git config changed.
- No OS audio defaults or profiles, user input selections, real sound-server configuration, existing user projects or recordings were changed; no GitHub push/merge occurred.
- Commit does not include private audio or temporary probes, commercial plugins, credentials, system recordings or binaries.
- **Integrator 07:** inspect `git show be614f7` and this report, then cherry-pick/merge the branch only after reconciling `studio.rs` and `studio_views.rs` with the Session/Editing agents and `studio_devices.rs` with UI Agent 06. Run all shared contract checks and separate hardware/GUI acceptance, and update `AGENTS.md`, `README.md` and `docs/FEATURE_MATRIX.md` in the integrated branch. Do **not** declare Beta 1 READY based solely on this isolated pass.

**Recommended next step:** a 10-second permitted physical headphone/mic validation with visual project reopen, followed by an explicit count-in design that does not restart transport mid-take.
