# Rustie / Slate Studio: Beta five-blocker development report

**Date:** 20 November 2026 UTC
**Location:** `/home/developer/Projects/rust-daw`
**Status:** Native metronome, four native effects, compact Devices UI, real PulseAudio recording and experimental CLAP realtime audio effect code exist and are tested at source/backend level. Full realtime VST3/CLAP instruments and plugin native preset/state support remain unresolved.

## 1. Changes made in this pass

**New code authored in this pass**
- `crates/daw-core/src/model.rs`: added `Project.metronome_volume` with backwards-compatible serde default 1.0, unit checks for saved-volume round-trip and old sessions.
- `crates/daw-ui/src/studio.rs`, `studio_views.rs`: persisted compact metronome volume slider, true in-engine 128-frame gain ramp changes while playing, immediate click state without reopening the stream. The metronome responds to tempo and time signature already.
- `crates/daw-ui/src/studio_recording.rs`: when a named recording input is selected and unplugged, refuse recording with an actionable error rather than silently falling back to the webcam or another microphone.
- `crates/daw-ui/src/studio_live_plugins.rs`: strengthened the isolated realtime CLAP effect bridge: timed readiness before stdin writes, nonblocking child stdout and deadline-based full-block reads. A hung child that writes only a partial response no longer blocks the worker indefinitely at `read_exact`. The audio callback remains lock-free and falls back to delay-matched dry audio after missed blocks.

**Already present in the worktree when this pass began, validated rather than falsely credited to this pass**
- Sample-aligned click metronome (accented downbeat, time-signature aware).
- Four native persisted DSP devices: EQ3, EQ8 with draggable eight-band response plot, Vocal Reverb and Guitar Reverb.
- Compact Devices window with FX Chain, Plug-ins and Audio/MIDI tab, small controls, effect bypass/reorder.
- Live audio capture into an imported WAV take with physical input selection, monitor ring, dropped-frame telemetry, fresh-buffer checks.
- Offline CLAP/VST2/VST3 MIDI/audio rendering via isolated worker. Experimental **realtime CLAP audio effects** on audio tracks and groups through isolated worker; **not** realtime CLAP instruments or VST3 hosting.

## 2. Real UI and screenshots

New release binary: `target/release/daw-ui`, with `target/release/daw-plugin-host` also rebuilt. A real native GUI process using the existing 4-track review project ran for 5 seconds and ended with intentional `timeout` status 124 and no captured stdout/stderr error. **This is a startup smoke, not GUI mouse testing.** No current screenshot was captured; prior screenshot files predate this pass.

## 3. Verification

| Check | Observed |
|---|---|
| `cargo test --workspace --quiet` | PASS; 81 tests, zero failures: 19 core, 33 engine, 3 engine RT, 2 plugin-host, 24 UI |
| Strict `cargo clippy --workspace --all-targets -- -D warnings` | PASS |
| `cargo fmt --all`, `git diff --check` | PASS |
| `cargo build --release -p daw-ui -p daw-plugin-host` | PASS |
| `target/release/daw-ui --probe-recording` | PASS, real AIR 192|6 PulseAudio input, 44,100 Hz, 15,082 frames, 0 dropped, 342 ms saved/read WAV; temporary WAV deleted |
| `target/release/daw-plugin-host scan-clap /usr/lib/clap/lsp-plugins.clap` | PASS; real LSP EQ, reverb and limiter classes listed |
| `target/release/daw-ui --probe-live-clap /usr/lib/clap/lsp-plugins.clap in.lsp-plug.limiter_stereo` | PASS, 400 blocks at 44,100 Hz, peak 0.15, zero reported dry-fallback blocks, after new nonblocking IO hardening |
| `target/release/daw-ui --check-project '.../Rustie UX Review.slate.json'` | PASS, 4 tracks, 8 clips, 2 decoded WAVs |
| Native GUI startup with existing review project | PASS only for startup/survival 5 seconds; intentional timeout 124 |
| Subjective speaker listening, 10-second user microphone take saved/reopened, drag FX controls, MIDI live-record performance | NOT TESTED |

## 4. Remaining limitations

- **Plugin support is partial**: no live VST3, no realtime CLAP/VST3 instrument path for incoming MIDI, no plugin-native parameter panel, state/preset restore, comprehensive latency/XRUN testing or recovery under every plugin crash. The CLAP worker dry-fallback protects the render thread but can cause brief audible changes on CPU overload.
- The actual recording path writes real WAV, but human voice/guitar quality, monitor feedback, hardware-measured capture/monitor latency and save/reopen in live GUI have not been audited end-to-end. CPAL timestamp estimate is not end-to-end latency compensation.
- EQ/reverb DSP is implemented, but its musical quality and UI discoverability still need listening/mouse review. Realtime insert parameter changes may rebuild playback.
- Metronome click and volume are functional source/graph operations; no count-in or separate pre-roll recording phase yet.
- No demonstration screenshot or interactive UI acceptance available from remote Wayland connection. Do not present historical screenshots as proof of new panel.

## 5. Next work, in order

1. Complete realtime CLAP **instrument** hosting and realtime VST3 effect/instrument compatibility with robust per-plugin state serialization and live MIDI event scheduling. Keep isolation, crash containment and latency compensation honest.
2. Run a 10-second physical mic/guitar WAV take, with direct-monitor checkbox tested, inspect recorded source, headphone latency, WAV alignment and project save/reopen; fix any discrepancies.
3. Add metronome count-in for recording (1-2 bars), user-adjustable click accent/tone if needed, and safe monitoring metering.
4. User human test of compact Devices pane, EQ8 curve, EQ3, Vocal/Guitar reverb, and plugin-insert flow. Fix actual usability issues from video, not guesses.

## 6. Git and files

Working branch `master`, starting HEAD `f578b9a`. This checkout had extensive inherited dirty modified/untracked files, all preserved. **No reset, commit, push, remotes change or publication.** No user project/audio files overwritten. Changes authored in this pass are the four source areas in section 1 and this report/docs. The test probe wrote and deleted only its temporary WAV.

## 7. Short next-agent instructions

Read `AGENTS.md`, this report, `docs/FEATURE_MATRIX.md`, `crates/daw-ui/src/studio_live_plugins.rs` and `crates/daw-plugin-host/src/clap_stream.rs`; check `git status` before editing. Treat the experimental CLAP live effect as a validated starting point, not proof of full plugin support. Prioritize realtime instrument MIDI or VST3 live host **with state recall and tests**. Preserve working capture, metronome and built-in FX. At end rerun workspace tests, strict Clippy, release build, CLAP live effect and recording probes. Create a new dated report, no commit/push without authorization.

## 8. Additional implementation in the same pass: live VST3 audio effects

After the initial report was drafted, a substantive **realtime VST3 effect** path was added and verified:
- New `crates/daw-plugin-host/src/vst3_stream.rs`: a 128-frame, stereo float, sample-rate-negotiated VST3 effect worker using the same binary IPC protocol as CLAP. The VST3 plugin loads/executes in the isolated helper, not the DAW's audio thread.
- Updated `crates/daw-plugin-host/src/main.rs` with `stream-vst3-effect` mode.
- Updated `crates/daw-ui/src/studio_live_plugins.rs` to attach either a CLAP file or VST3 bundle to the existing bounded live bridge, using independent worker subprocesses and the same 512-frame delay/dry fallback. The CLAP class ID is required only for CLAP; VST3 uses the host loader's class selection.
- Updated `crates/daw-ui/src/studio_plugin_render.rs` compact Plug-ins panel with a **＋ Insert live VST3** button, correct per-track plugin display and CLAP/VST3 effect wording.
- Updated `crates/daw-ui/src/main.rs` with `--probe-live-vst3 BUNDLE.vst3`.
- Real installed `/usr/lib/vst3/Surge XT Effects.vst3` processed one 1024-byte stereo PCM block directly under the new worker with exit 0, 1024 output bytes, and no stderr.
- Real end-to-end realtime bridge: `target/release/daw-ui --probe-live-vst3 '/usr/lib/vst3/Surge XT Effects.vst3'`: **PASS**, 400 blocks, 44,100 Hz, peak 0.15916, **16 dry fallback blocks**. This is a bounded synthetic probe, not actual speaker/listening or load testing.
- **Known incompatible example:** `/usr/lib/vst3/lsp-plugins.vst3` fails `vst3-host` metadata parsing because its `moduleinfo.json` maps one old class ID to multiple new classes. The DAW intentionally must not claim universal VST3 compatibility. The same LSP provider's CLAP limiter passes the isolated realtime CLAP bridge.
- The new VST3 endpoint supports **audio effects** on Audio tracks/groups, not realtime MIDI instruments, plugin-native preset/parameter editing, plugin-state recall, arbitrary buses or every VST3 vendor. `Track.live_clap` remains the backwards-compatible persisted field name for either CLAP or VST3 effect. Future agents should migrate naming only with explicit serde backwards compatibility.
- No Git commit, push or publication; all user projects unchanged. Updated source/test evidence supersedes any earlier sections above that describe only CLAP as live.

**Next agent priority update:** After human playback/capture UX testing, add realtime MIDI event routing for CLAP/VST3 instruments and proper plugin state/preset recall; diagnose host metadata compatibility for LSP VST3 without weakening validation.

**Final validation superseding interim counts above:** 82 full-workspace tests passed (19 core, 33 engine, 3 RT integration, 3 plugin-host, 24 UI); strict Clippy, release UI+worker build, format and diff whitespace all passed. Final live bridge probes at 44,100 Hz: LSP CLAP limiter 400 blocks / 0 fallback; Surge XT Effects VST3 400 blocks / 17 fallback. No further source edits after this verification, only documentation.
