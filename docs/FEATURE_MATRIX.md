# Feature matrix — development snapshot, 23 November 2026

The 21 November baseline and 22 November follow-ups have different verification scopes. See [current status](PROJECT_STATUS.md) and [verification](VERIFICATION.md). Later dated notes below are historical.

Legend: Implemented = functional in source and compiled; Partial = usable but with a defined limit; Discovery only = listed but not loadable; Planned = not implemented.

| Feature | Status | Evidence / constraints |
|---|---|---|
| Rust native launch / ALSA/Pulse/JACK backend selector | Implemented | `studio.rs`, `daw-engine`, backend health depends on machine |
| Global transport, BPM, project save/open | Implemented | `studio_views.rs`, `studio.rs` |
| Arrangement scroll, timeline seek, zoom, clip move, selected-clip trim/split/copy and undo | Verified in current GUI for seek, split, undo and redo | Native pointer and keyboard input at 127 BPM and 6/8 verified the split and saved clip counts. Audible continuity during edits awaits owner acceptance. |
| Arrangement multi-clip editing and history | Implemented / playback review pending | Shift-click to select clips across tracks. Move, copy/paste, duplicate, split and delete a selection. Undo/redo restores its clips. Trim works on individual edges. |
| Track mute, solo, gain, pan, delete via context menu | Implemented | `studio_views.rs`, graph controller |
| Library categories and text search | Implemented | `studio_browser.rs` |
| Sample folder chooser, bounded indexing, audio import | Implemented | Up to 2,400 indexed audio files / 180 visited directories per refresh; runtime folders not persisted |
| Native CLAP / VST2 / VST3 discovery and offline rendering | Partial | Isolated CLAP, VST2 and VST3 clip rendering and bounded discovery. The browser exposes role and inspection errors. The LSP VST3 bundle fails metadata validation. |
| LV2 discovery | Discovery only | Indexed but no DSP host |
| Built-in 16-voice sine instrument | Implemented | Default for legacy and unassigned MIDI tracks. |
| Built-in SoundFont instrument | Implemented / bank-dependent | Local GM bank and per-track preset, mouse and computer-key audition, audio-thread swap regression and legacy fallback. No bank is distributed with the app. |
| 16-channel SoundFont engine (sf-16) | Engine foundation / dedicated UI incomplete | Per-channel program, controllers, tuning, sustain, pitch bend and mute/solo; allocation-guarded render path. No production bank is bundled. |
| LNF-1 native piano device | Partial / experimental | Original 32-voice engine, 20 persisted per-track parameters and atomic live updates. No sample parity, general host automation/undo, telemetry or manual GUI/audio acceptance. |
| Piano root/mode across 12 scales, key highlighting and fold | Implemented | `studio_piano.rs`, `daw-core/src/scales.rs` |
| MIDI note creation, move, resize, delete, duplicate, velocity | Partial / GUI test pending | Integrated `studio_piano.rs` and `studio_history.rs` have bounded document undo/redo and velocity-lane manipulation; code tests passed, manual gesture proof absent |
| Snap granularity 1/4..1/32, note length, in-key restriction | Implemented | `studio_piano.rs` |
| Audio Session loop source | Implemented | `graph.rs::Source::LoopedSamples`; RAM-based |
| MIDI Session rolling loop events | Implemented | `daw-engine/src/session.rs` schedules note events and loop phase on the render clock. |
| Clip launch / scene launch / stop track / stop all | Implemented | Quarter, sixteenth and bar grids. Track stop, Stop all and cancellation use the bounded scene path. Transport Stop is immediate. |
| Create MIDI/add audio into next Session slot | Implemented | `studio_session.rs` |
| Session slots independent of arrangement clips | Implemented | Sparse cells belong to ordered, persisted scene IDs. Legacy migration copies old row content and preserves Arrangement clip IDs and offsets. |
| Live quantized launch and gapless hot swap | Render-verified | Atomic queue admission, replacement, cancellation, engine-owned loops and 128-frame gain ramps. Physical transition quality and external instruments under Session load still need testing. |
| Realtime CLAP / VST3 audio-effect inserts | Partial | LSP Limiter Stereo CLAP and Surge XT Effects VST3 passed paced 400-block tests at 44.1 and 48 kHz with zero fallback on baseline runs. CLAP worker kill/hang used dry fallback and cleaned up. Arrangement export requires bypass or printed audio for live inserts. |
| Realtime CLAP / VST3 MIDI instruments | Partial | Surge XT CLAP and VST3 passed paced 550-block tests at both rates with zero misses. Native state is bound to its worker class on explicit Save. Supported plug-in MIDI input includes channel, sustain and pitch bend. Hardware MIDI remains to be tested. |
| Native metronome | Implemented / live audition pending | Sample-aligned tone clicks, accented beat one, top-transport toggle, persisted on/off and independent volume control with ramped live updates; BPM/time-signature-linked bar grid; engine source+tests passed. |
| Four native insert effects | Implemented | Current native GUI saved EQ3, EQ8 and two reverbs on the same track with a reordered chain and bypassed EQ3. Changes use actual DSP; audition awaits physical playback. |
| Compact Devices / FX chain workspace | Implemented | Track-aware chain and plug-in inspection, Audio/MIDI, worker status, and a skippable guide. F3 opens Devices. |
| Input recording / WAV takes / optional live monitor | Owner listening pending | Per-track PulseAudio device and channel selection, Mono 1 and 2 or Stereo 1–2, selected-track input meter, and a capture-driven REC timer and waveform. Finalized mono WAVs play centered at neutral pan. Software monitoring starts off. |
| Arrangement WAV mixdown | Partial | Native instruments, imported audio and built-in FX render. The export refuses projects with an enabled live CLAP/VST3 insert. It covers Arrangement clips and built-in effects. |
| Hardware MIDI input, editable MIDI take recording and audition | Partial | `midir` ALSA port tested, selected-track audition while transport stopped, note-to-clip capture and undo; real keyboard and human listening unverified. |
| Live plug-in sustain and pitch bend | Partial | CC64 and pitch bend use the instrument input dialect or VST3 controller mapping when available. MIDI takes store notes. |
| Warp, .als projects | Planned | Not implemented |
| GPU backdrop blur / liquid shaders | Planned | UI is faux glass only |
| High-quality resampler and streamed audio | Planned | Audio decoded into RAM |

| Right-click track management (both views) | Implemented | Create MIDI/empty audio; add media to existing audio track; rename; group via actual return bus; move/ungroup/remove with confirmation. Nested groups and multi-selection not supported |
| Dedicated lower mixer panel | Implemented | Visible when switching to Session, manual Mixer tab/button; actual track/group VOL/PAN/M/S and master peak; no fake per-track telemetry |
| Timeline/piano drag-drift mitigation | Implemented (awaiting UX retest) | Disable egui content drag-to-pan; wheel and scrollbar kept; simulated headless regression passed |
| Arrangement release-click seek | Implemented working state; final acceptance incomplete | Ruler, empty lane and clip-body clicks target the tempo map; live seeks queue on the existing stream. Agent stopped before desktop/audio acceptance. |
| Output sink clock negotiation | Partial; startup failures remain | Baseline probes intermittently failed to advance within one second; physical output delay and XRUNs await measurement. |

| Stable Library width and explicit resize | Implemented, manual retest pending | Video revealed panel width growth shifting center workspace; explicit browser width and 120-frame stability test; ‹/› resizing instead of mouse edge dragging |
| Context menu on Arrangement lanes/clips and Session slots | Implemented, manual retest pending | Whole track lane and clip responses get track-specific menu; synthetic secondary-click test passed |
| Upright mixer strips and real master meter | Implemented, manual visual retest pending | Distinct vertical track faders, pan, M/S, group bus styling; real master peak only; no fake per-track meters |

## 2026-11-19 verified engineering pass
- `cargo test --workspace --quiet`: **74 passing tests**, none failed (18 core, 29 engine, 3 real-time integration, 1 plugin-host, 22 UI).
- `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all -- --check`, `git diff --check`, `cargo build --release -p daw-ui -p daw-plugin-host`: passed.
- Actual CLAP worker scanned LSP classes/Surge XT and rendered two synthetic files: Surge MIDI 2.0s/48kHz, peak 0.333; LSP limiter effect 1.5s/48kHz, peak 0.275. Both verified by ffprobe as stereo float WAV.
- `daw-ui --probe-recording`: actual physical AIR source @44100Hz, 15038 captured stereo frames with 0 dropped queue frames; tested storage, not microphone signal fidelity or monitoring audibility.
- New GUI build ran 6 seconds with no stderr until intentional `timeout` exit 124; no actual pointer screenshot or physical audio/mic inspection.
- See 19 November report (referenced in the original handoff; file unavailable in the supplied checkout).


**Further input-clock root-cause correction:** CPAL's default PulseAudio capture buffering was reporting ~1010 ms input latency and captured ~1s of audio during a nominal 350ms test. Changed PulseAudio input requests to a 512-frame fixed buffer and added a stream-ready plus capture-timestamp gate, keeping stale pre-roll out of WAV takes. Verified three additional 350ms tests on the real AIR 192|6 input: **339, 342, 342 ms recorded**, opening **5-6 ms**, **0 dropped queue frames**; no temp audio retained. This measures file/callback alignment only, not real analogue input-to-speaker round-trip latency.

## Validation log
- `cargo fmt --all` applied.
- `cargo test --workspace` passed.
- `cargo clippy --workspace --all-targets -- -D warnings` passed.
- `cargo build --release -p daw-ui` passed, so the launcher uses an updated binary.
- These checks do **not** prove correct speaker routing, visual polish at every DPI, reliable live gig performance, or plugin compatibility.

Additional verification (23:00 UTC pass): 40 workspace tests passed, strict Clippy and release build passed. New egui simulated pointer drag tests cover editor-height persistence; physical mouse and audio not tested. See `docs/DAW_READINESS_AUDIT_2026-11-17.md`.

Round 2 verification: `cargo test --workspace` 43 passed; strict Clippy, format, release build and `git diff --check` passed. Temp grouped review JSON (5 tracks) preflighted and the GUI process survived 6 seconds. Physical mouse sound/group listening remains unverified. See `docs/agent-reports/2026-11-17-ux-round2-track-management-report.md`.

## Relevant source files
- New: `crates/daw-ui/src/studio_browser.rs`, `studio_piano.rs`, `studio_session.rs`
- Modified: `crates/daw-ui/src/studio.rs`, `studio_views.rs`, `crates/daw-engine/src/graph.rs`

[executed on device: Johnnysstation (1200896c-45a0-4cb2-8dfa-479e25d803a4)]