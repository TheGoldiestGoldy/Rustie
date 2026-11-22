# Slate Studio: source-verified DAW readiness audit

Date: 2026-11-17. Scope: local `master` tree inspected; no claims of production readiness inferred from passing unit tests. This document is a prioritized engineering backlog, not a promise that missing features have been built.

## Evidence from current source

- **Already works at source/test level:** native egui transport and two workspace layouts, audio/MIDI arrangement clips, piano editing, a basic MIDI synth, mixing gains/pan/mute/solo, imported decoded audio, sample-accurate internal engine commands, JSON project save/open, audio backend selection, and clip/scene triggering. See `crates/daw-ui/src/studio*.rs`, `crates/daw-engine/src/{graph,engine,driver}.rs` and `crates/daw-core/src/model.rs`.
- **Newer than older docs:** `studio_session.rs::queue_session_track_launch` now queues live launches at a future bar boundary, preloads Session clip nodes, and ramps gain for 128 frames; `engine.rs` has relevant tests. It no longer necessarily restarts the driver on *every* live clip launch. Changes of Arrangement/Session view and playback/edit resets still restart through `start_playback`. This code-level behavior is not a verified live-gig, glitch-free test.
- **Piano editor update in this pass:** `detail_panel` fills available height so a resize persists; MIDI keyboard spans 0..=127; scroll aligns to the clip pitch center. Regression tests simulate pointer drag/release and pitch visibility.
- **Host is empty:** `crates/daw-plugin-host/src/main.rs` prints a placeholder message; `crates/daw-host-client/src/lib.rs` is a placeholder; plugin browser only discovers entries. On this machine `/usr/lib/clap/` includes LSP Plugins and Surge XT libraries but they do not load in Slate Studio.
- **Recording absent:** UI deliberately disables Record; `daw-engine/src/driver.rs` opens audio output streams, not track-armed input streams or disk capture.
- **Export absent:** no project bounce/export pipeline or offline mix to WAV/FLAC/MP3 exposed by the application. `graph.rs::render_frame` is a useful DSP primitive but not a renderer/exporter.
- **Undo wiring absent:** the core crate has a tested command history, but piano edits directly mutate `Project` in `studio_piano.rs`; there is no integrated UI undo/redo workflow.
- **Project safety:** JSON contains project tracks and external audio file paths, not self-contained audio assets, plugin instances, session slot identities, or recovery journals. Sample sources are predecoded to RAM and currently resampled linearly.
- **Reliability unproven:** no measured speaker-to-playhead latency, physical input/output round-trip timing, dropout/XRUN acceptance tests under high load, crash recovery, or project reopen test with missing media and plugins.

## P0: blockers to calling it a complete DAW

| Priority | Missing capability | Definition of done |
| --- | --- | --- |
| P0-A | Real CLAP plugin host (instruments and effects) | Load a native CLAP instrument (e.g. installed Surge XT), send MIDI, render audible instrument output; load an LSP stereo effect; adjust parameters; save/reopen exact plugin state; bypass/remove without restarting the app; handle scanner/plugin failure. Verify thread safety and latency reporting. |
| P0-B | Recording of real sources | Track arm, selectable M-Audio input ports, monitored/non-monitored recording to files, synchronized capture while other tracks play, stop/pause/discard and recovery, peak monitoring, no allocation/disk I/O on the realtime callback. |
| P0-C | Export / bounce | Full-song offline stereo WAV at 16/24-bit with deterministic length/sample rate, correct MIDI and audio, fades/tails, no clipping, reproducible comparisons to live engine. Later stems and additional codecs. |
| P0-D | Safe editing and preservation | Complete undo/redo for note/clip/track/mixer operations; autosave, crash recovery, missing media relink; media-copy project folder and reliable reopen on a second Linux machine. |
| P0-E | Input MIDI and synchronization | Enumerate ALSA/JACK MIDI devices, arm/monitor/record a keyboard, correct timestamps and latency, sustain/CC/pitchbend and panic/all-notes-off, no stuck notes. |
| P0-F | Audio hardware qualification | Measure startup onset and round-trip latency on ALSA/PipeWire/JACK; expose actual buffer/sample rate, XRUNs; stress-test 30+ tracks and real plugins over extended sessions, compare exported mix timing. |

**Recommended plugin approach:** start with CLAP and `clack-host` (MIT/Apache-2.0), using the official CLAP host example. Isolate scanning/metadata from the GUI; instantiate and allocate off the realtime thread; keep audio processor handles on appropriate threads and provide bypass/state/latency. Rust `clack-host` is a low-level foundation, not a drop-in DAW plugin system. Do not claim VST3/LV2 support by merely scanning directories. VST3 and LV2 support belong after a tested CLAP vertical slice. Reference: https://github.com/prokopyl/clack ; https://github.com/free-audio/clap-host

## P1: essential professional editing/mixing workflow

- Arrangement clip split, slip, trim, duplicate, crossfade, copy/paste, marquee/multi-select and sane snapping/loop-region editing. Current clip move and MIDI note manipulations are a beginning, not a full editor.
- Mixer insert/send/return chains, track grouping, routable buses, a real meters/limiter path, common stock EQ/compressor/gate/reverb/delay and predictable bypass behavior.
- Editable automation lanes and recording/playback of mixer/plugin controls (the engine has sample-accurate command scheduling; there is no timeline automation authoring).
- Piano roll note multi-selection, legato/transposition/scale tools, velocity-bar dragging, quantize/humanize and grid division controls; time signature/tempo maps in the UI instead of the fixed `4/4` transport label.
- Session slots serialized separately from Arrangement clips; clip and scene editing independent of linear arrangement, retrigger/re-queue behavior, user-selectable launch quantization and UI-free MIDI scheduling that survives GUI stalls.
- Sample streaming and quality resampling rather than full RAM decode and linear interpolation, especially for lengthy concert tracks.

## P2: deployment and broader compatibility

- Reproducible Linux package (AppImage/Flatpak or native distribution) and first-run device/permissions wizard; automated tests across representative Arch, Ubuntu and Fedora systems.
- Plug-in validation against multiple vendors, UI embedding, state versioning, crash isolation and backward compatibility; transport/MIDI clocks and MIDI controller mapping.
- Better waveform editing, pitch/time stretch, warp markers, comping, take lanes, stretching formants, specialized scoring/video features, optional multi-platform support.
- Accessibility, keyboard customization, project templates, media portability, user docs and support diagnostics.

## Minimum honest release gate

Do not advertise "complete DAW" until a tester on a supported Linux installation can, in a single session: select physical audio input, record a voice and MIDI keyboard, load third-party instrument and effect, edit notes/clips with undo, save/reopen with all media and plugin states intact, and export a correct WAV without a crash or unexplained large output latency. Log and inspect any XRUNs. Until then market as a promising pre-release composition and clip sketching tool, not a general substitute for established DAWs.

Reference benchmarks: https://ardour.org/features.html and https://manual.ardour.org/working-with-plugins/ ; official source docs used for expectations, not copied implementation.
