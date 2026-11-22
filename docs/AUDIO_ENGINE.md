> Historical design/implementation notes. For current capabilities and gates, see [project status](PROJECT_STATUS.md) and [architecture](ARCHITECTURE.md).

# Audio engine, native backends and timing notes

**Snapshot:** `f578b9a`, 2026-11-17. Refer to source and current measurements before asserting improvements.

## Audio flow

```text
UI / control thread
  Project model -> graph compile -> predecode audio into RAM
     -> rtrb SPSC Controller -> ScheduledCommand{absolute engine_frame}
                                    |
                                    v
                         RenderEngine (audio callback)
                     fixed staging, MIDI voices, routing/PDC
                                    |
                                    v
                        CPAL native f32 stream
                   ALSA / PulseAudio / JACK (PipeWire)
                                    |
                                    v
                       device graph -> hardware output
```

`daw-core` stores musical time at 960 ticks per quarter note (PPQ). Engine scheduling uses a monotonic **hardware engine-frame clock**, distinct from project-relative play position. Track/clip edits affect the model and can require rebuilding the graph off the real-time thread.

## Implemented foundations

- Lock-free `rtrb` single producer/single consumer command queue capacity **1,024**.
- Fixed pending staging capacity **1,024** commands, **64** simultaneous linear gain ramps.
- Node routing graph with up to **64** nodes and declared latency delay buffers; calculated PDC does **not** prove correct external plugin latency handling.
- Simple built-in 16-voice sine synth; not a production sample/synthesis engine.
- PCM output through CPAL 0.18.2; enumerates native **ALSA**, **PulseAudio**, **JACK** hosts where present.
- Driver validates 44.1, 48 and 96 kHz requests; configured frame requests 64..2048, JACK buffer governed by server.
- Atomic callback count, rendered/project frames, command overflows, generated peak, block peak, *predicted callback-to-output delay*.
- Linux real-time scheduling negotiation outside the callback, using existing scheduling permissions/RTKit fallback.

## Non-RT / unsafe operations

Do **not** perform these inside the audio callback: memory allocation, I/O, lock acquisition, printing/logging, D-Bus, graph recompilation, large sort, file decode, sample resampling, plugin discovery, or UI work. Graphs/media and delay buffers must be prepared before starting the stream. Check `tests/rt_no_alloc.rs` whenever modifying the render path.

Tests cover our code paths, not internal behavior of all CPAL/platform dependencies. No tested live plugin worker exists.

## What audio latency numbers actually mean

- **Requested buffer:** nominal stream block request. `128 / 48000 = 2.67 ms` of samples per buffer, **not** round-trip or end-to-end latency.
- **Callback-to-output prediction:** supplied by the native backend's callback timestamps, subject to backend implementation.
- **Audible onset latency:** time from user pressing Play (or scheduled MIDI onset) to the physical output. Needs direct loopback/mic measurement.
- **Round-trip latency:** input -> processing -> output; **cannot be claimed without implemented and measured input**.
- **XRuns:** underrun/overrun deadlines; no quantitative xrun reporting is implemented yet.

## Reported device issues

A previous test showed rendered MIDI audio and GUI transport ahead of actual audible output by multiple notes. Switching to a lower-latency native path is a plausible mitigation but is **not proof that the fault is resolved**. See [KNOWN_ISSUES](KNOWN_ISSUES.md).

On the originally tested PC, an M-Audio AIR 192|6 was at one point configured to **Digital Stereo (IEC958)** rather than the analog hardware outputs. A JACK/PipeWire graph can render perfectly while having no connected physical playback sinks. Later docs state analog stereo was selected. These describe a single host snapshot; do not hardcode or modify another user's profile.

**Never automatically change the OS-wide card profile, default sink, sample clock, or realtime limits.** Ask for informed control before any disruption to recordings/calls.

## Safe diagnosis commands

```bash
cargo run -p daw-engine --bin daw-audio-probe
cargo run -p daw-engine --bin daw-audio-probe -- --silent 48000 pulse
cargo run -p daw-engine --bin daw-audio-probe -- --tone 48000 pulse
cargo run -p daw-engine --bin daw-audio-probe -- --silent 48000 alsa
PIPEWIRE_LATENCY=128/48000 cargo run -p daw-engine --bin daw-audio-probe -- --silent 48000 jack
```

The tone is intentionally low-volume. Disable/stop other sound sources if you need isolation and monitor speaker volume sensibly.

## Required next diagnostic experiments

1. Record exact OS/backend/device/config, cold vs warm stream, project tempo and notes.
2. On Play: monotonic timestamp, first scheduled NoteOn engine frame, callback frame, generated nonzero sample frame, backend predicted playback delay.
3. Record physical loopback output timestamp when feasible; compare against transport screen capture timestamp.
4. Test PulseAudio vs native ALSA vs JACK with *actual connected physical ports*.
5. Repeat after each UI/scheduler change; quantify median / p95 onset and audio/UI skew. Log queue and staging overflows.
6. Add tests for seek, rapid play-stop, MIDI event loss and clip changes during playback.
7. Verify continuous playback while browsing samples, loading assets and moving windows.

Avoid attributing all delays to buffer size without measuring the real signal path.
