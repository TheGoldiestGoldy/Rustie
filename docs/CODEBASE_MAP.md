# Codebase map

## Dependency direction

```text
                  +------------------+
                  |     daw-core     |
                  | time/model/scale |
                  +------------------+
                    ^   ^   ^    ^
                    |   |   |    |
      +-------------+   |   |    +----------------+
      |                 |   |                     |
 daw-parser        daw-engine              daw-host-client
 (placeholder)       |                      (placeholder)
                     v
                   daw-ui
                     (native egui)
       daw-plugin-host (placeholder executable, standalone)
```

`daw-ui` consumes `daw-core` and `daw-engine`. The core contains data-only models; an `AudioGraph` is assembled separately for playback.

## Files and responsibilities

| Path | Responsibility |
| --- | --- |
| `Cargo.toml` | Workspace members, version, edition, license declaration |
| `Cargo.lock` | Pinned application dependency resolution, must be committed |
| `crates/daw-core/src/time.rs` | Music ticks, tempo and signature; tick to sample conversion |
| `crates/daw-core/src/scales.rs` | Pitch class, mode and scale predicates/fold |
| `crates/daw-core/src/model.rs` | Serializable Project/Track/Clip/MidiNote model |
| `crates/daw-core/src/commands.rs` | Reversible edit-command infrastructure |
| `crates/daw-engine/src/graph.rs` | Sources and nodes, graph validation, routing, delay buffers, sine voice |
| `crates/daw-engine/src/engine.rs` | Lock-free command ingress, audio render, MIDI event scheduling and telemetry |
| `crates/daw-engine/src/driver.rs` | CPAL backend selection, device config, audio callback |
| `crates/daw-engine/src/realtime.rs` | Linux RT scheduling policy negotiation |
| `crates/daw-engine/src/audio_file.rs` | Format decode and in-memory sample-rate conversion |
| `crates/daw-engine/src/bin/daw-audio-probe.rs` | Harmless silent and opt-in audible diagnostics |
| `crates/daw-engine/tests/rt_no_alloc.rs` | Allocation-guarded rendering regression checks |
| `crates/daw-ui/src/main.rs` | eframe initialization / CLI smoke probe |
| `crates/daw-ui/src/studio.rs` | Main app state, project I/O, graph compile, transport and MIDI editor |
| `crates/daw-ui/src/studio_views.rs` | Transport, arrangement, Session grid |
| `crates/daw-parser/src/lib.rs` | Interchange placeholder only |
| `crates/daw-host-client/src/lib.rs` | IPC placeholder only |
| `crates/daw-plugin-host/src/main.rs` | Placeholder worker executable |

**Concurrent work:** At the time of audit, an untracked `crates/daw-ui/src/studio_browser.rs` existed in the live main worktree and was included by concurrently modified `studio.rs`. That browser is not part of snapshot `f578b9a`; integrate through a reviewed commit.

## Real-time ownership boundary

1. UI/control code constructs or rebuilds `AudioGraph`, decodes media, schedules edits.
2. `Controller` enqueues fixed-size `ScheduledCommand` values into `rtrb` SPSC.
3. The audio callback owns `RenderEngine`, consumes controls and fills interleaved output.
4. Telemetry uses atomics. No logging, disk access, heap allocation or locks in the render path by design.
5. Do not call expensive priority/D-Bus operations in the audio callback.

Changing the project model requires a consciously versioned mapping to engine graph and serialized documents; do not couple UI paint code to the RT render loop.

## External crates in snapshot

- `eframe 0.33.3`: desktop UI via egui
- `cpal 0.18.2`: native Linux sound backends, JACK and PulseAudio features enabled
- `rtrb 0.4.1`: lock-free ring buffer
- `symphonia 0.5.5`: decode audio files
- `serde`, `serde_json`: serializable models and JSON documents
- `zbus`, `libc`: Linux scheduling/priorities
- `assert_no_alloc`: audio render regression tests

Version listing comes from Cargo manifests. Use `Cargo.lock` for fully resolved transitive versions.
