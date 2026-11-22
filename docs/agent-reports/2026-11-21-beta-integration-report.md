# Rusty Beta 1 integration report

See the [later integration follow-up](2026-11-21-beta-integration-followup-report.md) for the current source and release decision.

**21 November 2026 · Wave 2 integrated · Beta 1 NO-GO**

**Worktree** `/home/developer/Projects/rust-daw-agent-integration` on `beta/integration`
**Verified source** `bcdac5e4c3569414a1a7445b4019fd079a3c79aa`

## 1. Integrated source

| Area | Commit and parent | Behavior |
|---|---|---|
| Session | `03782a7f1ad56683c29865babd0d8242e45ae48f`, second parent `04b7c37889f549fc960c83a44e7259cec5c3c08c` | Persistent sparse slots, legacy migration, render-clock scene scheduling and bounded atomic queue admission. `daw-core/src/session.rs`, `daw-engine/src/session.rs`, `daw-ui/src/studio_session.rs`. |
| Devices/UI | `89fb080670f259467c374769fab16a49723ed004`, second parent `e1b1e64ac8fe3ec3a0140139fd72ffbb7e44d0c6` | Compact Devices, real FX controls and metadata inspection, worker status, layout and help. The merge retains the Rusty build stamp, matched worker resolver, native-state identity checks and independent Session. |
| Recording regression | `bcdac5e4c3569414a1a7445b4019fd079a3c79aa` | Recording checks the 48-clip Arrangement capacity even from Session. If edits fill it mid-take, the WAV is finalized and the UI gives its import path. The guard and regression test are in `daw-ui/src/studio_recording.rs`. |

The [coordinator handoff](../beta-plan/01-BASELINE-COORDINATOR.md) records `BASE_SHA=1ae60bb2106b595e7e46c33911753ee45eb6d6f6`. Git confirmed all three Wave 1 specialist commits in the original integration ancestry and both clean Wave 2 tips at the intentional merge base `e6b82a79feeac4158f1918b3f5598916d5bb177c`. Session merged before UI. The UI conflict resolution kept Session launch controls and data paths, the single branded worker resolver, bounded CLAP/VST3 inspection and runtime status.

## 2. Native app evidence

- [Session scene 2 playing on the Quarter grid at 1540×940](../screenshots/beta-integration-session-playing-1540.png)
- [Session and Mixer at the 980×640 minimum](../screenshots/beta-integration-session-980.png)
- [Devices at the 980×640 minimum](../screenshots/beta-integration-devices-980.png)

The matching installed UI and worker came from the same clean source commit in an isolated profile. Native pointer and keyboard input exercised first-run Skip, BPM change to 127 and signature change to 6/8, ruler seek, audio split and undo/redo (8 → 9 → 8 → 9 Arrangement clips), WAV import from Library, FX chain add/reorder/bypass and EQ8 gain drag. I added scene 9, switched from scene 1 to scene 2 at Quarter quantization, observed four looping cells and a real graph peak on the private output, then stopped the slots. The persisted Arrangement retained its own clip count and the Session kept its eight occupied cells. F3 opened Devices at the 980×640 minimum. A cold start reopened 4 tracks, 11 Arrangement clips, 8 Session slots and 3 decoded WAVs. A second Save during playback yielded identical native state for the Surge XT CLAP instrument (50,310 bytes) and Surge XT Effects VST3 insert (1,150 bytes). EQ8 saved +15.868 dB at 350 Hz.

## 3. Commands and measurements

Builds used `CARGO_TARGET_DIR=/home/developer/.cache/rustie-target-integration`, `CARGO_BUILD_JOBS=4` and `RUSTY_BUILD_REVISION=bcdac5e4c356`. Raw results and helper scripts are in `/home/developer/.cache/rusty-wave2-qa.Qmwe5r`.

| Check | Command or procedure | Result |
|---|---|---|
| Formatting | `cargo fmt --all -- --check` | PASS |
| Workspace | `cargo test --workspace` | PASS, 128 tests including 6 realtime allocation tests |
| Lint | `cargo clippy --workspace --all-targets -- -D warnings` | PASS |
| Release build | `cargo build --release -p daw-ui -p daw-plugin-host` | PASS, Rust 1.100.0 |
| Patch | `git diff --check 7334c01..HEAD` | PASS |
| Legacy and fresh fixtures | `scripts/generate-ux-review.py` and release `daw-ui --check-project PATH` | Each legacy-form fixture decoded 2 WAVs and exposed 4 tracks, 8 Arrangement clips and 8 migrated Session cells |
| Recording writer | `daw-ui --probe-recording-writer` | PASS, 441,000 frames, stereo float, 44.1 kHz, 10.000 s, zero-frame transient error |
| Simultaneous virtual output and recording | Native GUI Record and Stop, private PulseAudio Null Output, remapped input fed by looped generated stereo PCM, then Save, cold reopen, `--check-project`, File → Export and `ffprobe` | PASS for the virtual path. 615.633084 s, 27,149,419 stereo frames at 44.1 kHz, zero dropped input frames, peak −15.35 dBFS. The saved take shows a waveform. The reopened project had 4 tracks, 9 Arrangement clips, 8 Session slots and 3 decoded WAVs. The stereo mix was 616.133333 s. |
| Virtual transport | `RUSTIE_PROBE_RATE=<rate> daw-ui --probe-transport <buffer> 1000` | Private PulseAudio Null Output advanced at 44.1/48 kHz with 64/128/512 requested frames. Reported delay was 1.79–11.52 ms. This measures the virtual callback clock. |
| Worker fault injection | Start `daw-ui --probe-live-clap` with LSP Limiter Stereo, signal its verified child PID with SIGKILL or SIGSTOP | Both returned 0 in about 1.2 s, kept peak 0.15000, produced 175 and 163 dry-fallback blocks respectively, and reaped the child |
| State identity | Workspace malformed-state and wrong-UID tests, native plugin-state worker save/load | Native playback dropped malformed or wrong-UID state before passing any bytes to workers and logged the reasons. The missing bundle was skipped and all 11 Arrangement clips and the saved Session layout survived Save. Instrument state round-tripped at 44.1 kHz and loaded at 48 kHz. |
| Missing or invalid plugin | Release probes against absent CLAP/VST3 bundles and an invalid LSP CLAP class | Expected exit 2, explicit error |
| Install | `scripts/install-rusty-local.sh` in a private test profile, `rusty --version`, `sha256sum -c SHA256SUMS` | PASS, version `Rusty v0.1.0-dev (bcdac5e4c356)`. Installed launcher opened the native app and resolved the matching worker. |
| Physical audio and mic | Owner-operated AIR 192\|6 take and listening | NOT TESTED |

### Live plug-in compatibility on this build

Graph probes used the release UI with `RUSTIE_SAMPLE_RATE=44100` and `48000`. Each successful pairing produced nonzero PCM with a 512-frame host bridge. These are tested combinations on the Linux test workstation.

| Format and role | Plug-in/version | Class ID | Blocks and fallback at 44.1/48 kHz |
|---|---|---|---|
| CLAP effect | LSP Limiter Stereo 1.0.38 | `in.lsp-plug.limiter_stereo` | 400 each, 0/0 |
| VST3 effect | Surge XT Effects 1.3.4 | `ABCDEF019182FAEB566D624153465854` | 400 each, 0/0 |
| CLAP instrument | Surge XT 1.3.4 | `org.surge-synth-team.surge-xt` | 550 each, 0/0 |
| VST3 instrument | Surge XT 1.3.4 | `ABCDEF019182FAEB566D624153675854` | 550 each, 0/0 |
| VST3 inspection | LSP plug-ins bundle | Bundle-level failure | Exit 1, conflicting old-class mappings in `moduleinfo.json` |

Exact probe verbs are `--probe-live-clap`, `--probe-live-vst3`, `--probe-live-clap-instrument` and `--probe-live-vst3-instrument`. State proof used `daw-plugin-host save-state-clap-instrument` and `save-state-vst3-instrument`, with an empty input-state argument for the first capture and its output path for the second capture. `cmp` confirmed identical 50,310-byte CLAP and 52,030-byte VST3 instrument states.

## 4. Release defects and limitations

1. **Full mixdown ignores live inserts.** File → Export full mix to WAV compiles the Arrangement graph with `compiled_graph_with_live(false)`. An enabled effect is skipped and an external instrument track falls back to the built-in sine. The GUI showed both Surge inserts active during playback. The enabled-insert export and a bypassed copy each delivered 2,788,718 float samples that compared sample for sample with a maximum absolute difference of 0.0. Session-only cells are outside the Arrangement export. See `crates/daw-ui/src/studio_export.rs`.
2. **LSP VST3 does not load.** Run `daw-plugin-host inspect-vst3 /usr/lib/vst3/lsp-plugins.vst3`. It exits 1 because an old class maps to two current classes. The native VST3 UI save retained the tested bundle and state bytes with an empty class ID and no persisted plugin UID. Bundle replacement at the same path lacks class-bound state validation in that case. Session preloads workers per assigned MIDI track and per populated slot, so multi-slot live-plugin performance still needs a separate stress check.
3. **Package prerequisites are inconsistent.** Cargo metadata declares Rust 1.85 and MIT OR Apache-2.0. The locked Linux graph includes `uuid 1.29.2` through `zbus 5.19.0`, with a 1.89.0 requirement. Repository license and dependency notices are missing. `cargo tree --locked --offline -i uuid@1.29.2` shows the dependency reaching the UI and worker. Plug-in inspection also requires GNU `timeout`.
4. **Reliability scope.** The 10-minute private run did not log an input or output stall. The render guard measures allocation, and the application reports callback progress. Backend XRUNs and per-callback deadline misses are uninstrumented in this run. The private GUI's CPU included software rendering, so it gives no isolated audio-thread utilization figure.
5. **Owner acceptance remains open.** Physical audio, monitor feedback, microphone waveform alignment, acoustic round-trip latency and hardware XRUNs were unmeasured.

## 5. Next acceptance

Fix or explicitly scope the export behavior, resolve packaging prerequisites and notices, then rebuild and repeat the installed smoke. On the candidate, use the AIR 192\|6 at its real clock, listen to internal MIDI and imported stereo WAV, record ten or more seconds of voice/guitar, inspect the waveform and compare its timing to a reference transient. Save, quit, reopen and listen to the exported file. See the [mandatory gate table](../BETA1_RELEASE_DECISION.md) and [scoped follow-ups](../beta-plan/10-AGENT-DISPATCH.md).

## 6. Git and deliverables

Both no-ff merges and the recording guard were committed locally. The decision and this handoff are documentation on top of the tested source. The release installer ran in an isolated profile and verified a checksum for each executable. Public release artifact and push status are none. The Wave 2 patch contains Rust sources, Markdown and synthetic-project window captures. It adds no plug-in binary, sample recording or dependency. Raw logs and disposable fixture media stay outside the Git worktree.

## 7. Handoff

Start from the latest clean `beta/integration` HEAD. Use the verified source above when comparing current evidence. Preserve the matched UI/worker, all five already merged specialist tips, project migration and the original checkout. The sole integrator owns the next release decision.
