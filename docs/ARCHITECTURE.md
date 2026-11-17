# Architecture, implementation status and verification

## Phase 1: core musical domain (implemented)

- Pure `daw-core` with integer 960 PPQ and tempo map integration to device frames using checked i128 arithmetic.
- Root C..B, 12 scale modes, chromatic vs scale membership and bijective fold-index/pitch mapping over MIDI 0..127.
- Track/clip/note models; bounded Command-pattern Undo/Redo on the UI thread.
- Tests for tempo boundaries, sample rates, fraction BPM, all 12 roots × 12 scales, and edit history.

## Phase 2: audio engine (functional initial implementation)

Code:
- `graph.rs`: topologically sorted DAG with audio/MIDI tracks, sends, returns, master, nodes identified by ID. Graph and edge delay lines preallocated off-thread. Up to 64 nodes, maximum declared per-node/path latency of 262144 frames.
- `engine.rs`: lock-free rtrb SPSC control queue, fixed-size sorted command staging, sample-accurate events on a monotonic engine clock, play/stop/seek, node gain/pan/send changes, 64 concurrent linear gain ramps, fixed MIDI voices, sanitized stereo/mono outputs. Queue overflow reported via atomic telemetry. Audio thread owns the graph.
- `driver.rs`: CPAL v0.18 with native PulseAudio, JACK and ALSA output selection and fallback for supported-device discovery, f32 stereo preference, 44100/48000/96000 Hz config validation, requested 64–2048 buffer frames (JACK controls own quantum), live stream callback, atomically published kernel callback TID.
- `realtime.rs`: control-thread real-time priority negotiation: use existing SCHED_FIFO scheduling where available, otherwise Linux direct scheduling via RLIMIT_RTPRIO, and finally RTKit D-Bus. No scheduling D-Bus calls inside the render callback.
- `audio_file.rs`: Symphonia decode to RAM buffers and basic offline linear resampling.
- `src/bin/daw-audio-probe.rs`: safe device enumerator and explicit silent/tone probe.
- `tests/rt_no_alloc.rs`: allocator guards around engine render paths including MIDI, PDC, timed automation, seek/stop and RAM sample buffers.

Verified on the originally tested Linux workstation on 2026-11-16:
- 33 tests passed across Phase 1 and Phase 2.
- Strict Clippy, all workspace targets, no warnings.
- Cargo formatting verification passed.
- Native ALSA 44100 stream ran with SCHED_FIFO granted via existing realtime permissions.
- Native JACK 48000 stream ran and callback already had real-time scheduler policy 1 (SCHED_FIFO).
- Per-client `PIPEWIRE_LATENCY=128/48000` produced approximately 750 callbacks and 96000 frames over 2 sec; system metadata quantum stayed 1024.
- Measurements confirm callbacks and scheduling, not measured xruns, round-trip latency, plugin safety, or audio fidelity.

Remaining Phase 2 production-hardening:
- Capture/input stream and armed recording; hardware MIDI ports/clock synchronization.
- Robust XRun reporting, real latency/CPU metering, varied hardware device support.
- Professional resampler, disk-streaming, click-free sample seeking/voice releases, sample fade, phase-coherent edit playback.
- Plugin-reported PDC integration, automation for arbitrary parameters, feedback/sidechain routing, safe graph hot swapping and deferred destruction.
- Scheduling deadline instrumentation, stress tests with high track counts and 64/128 samples on multiple machines.

## Phase 3: plugin hosting (NOT implemented)

- Separate worker process and shared-memory audio channel, CLAP and VST3/VST2 ABI.
- Defer VST2 legality to separate implementation and distribution review.
- A crashed worker may leave an audio deadline missed; isolation is not a guarantee of zero glitches.

## Phase 4: Ableton .als interoperability (NOT implemented)

- Gzip XML format is only a container; version-dependent schema and proprietary plugin states need independent corpus validation.
- Opaque XML preservation needs stable ordering and IDs; cannot promise lossless round-trip by default.

## Phase 5: native UI (prototype implemented)

- The committed baseline includes an egui desktop shell, Arrangement editor, integrated mixer controls, scale-aware MIDI piano roll and Session grid overview.
- Session independent clip launching, plugin chain hosting, full device mapping and production-grade UI polish are not implemented.
- A searchable sample/plugin browser is under active parallel development and must be integrated/retested before being marked merged.

## Technical caveats

- JACK/CPAL and ALSA internals can block or allocate, even though the isolated Rust engine render tests do not.
- Relative end-to-end latency combines device, OS, converters and potentially extra buffering; sample quantum by itself is not a measurement.
- Latency compensation calculates declared paths, but without real plugin processing no independent PDC null test has yet been done.
- Sine synth is a diagnostic only and has no production envelopes or sample-accurate voice-steal behavior.
