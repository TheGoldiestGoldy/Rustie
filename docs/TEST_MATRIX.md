# Verification matrix

**Evidence discipline:** source inspection, unit tests, smoke tests, hardware tests and benchmarks are different types of evidence. A PR must name the category.

| Test / behavior | Current evidence | Required to claim done |
| --- | --- | --- |
| Tempo math and 960 PPQ | Unit tests in `daw-core` | Tests pass for tempo boundaries, rates and fractional BPM |
| Scale/fold math | Unit tests in `daw-core` | All 12 roots and 12 modes; bijective pitch mapping |
| Undo infrastructure | Core unit tests | Integrate into GUI operations; undo/redo multi-step save/open |
| Graph validation/PDC | Engine unit tests | Routing edge cases, plugin latency reports, null test |
| MIDI scheduling | Engine unit tests | Block boundaries, note-on/off exact order, seek, tempo switch |
| No render allocation | `rt_no_alloc.rs` integration guard | No allocations in our tested render code paths |
| Formatting / lint | Historically verified 2026-11-16 | New CI run passes for the exact current head |
| Audio backend stream | Historically exercised ALSA/JACK on one workstation | Output hardware path verified and backend recorded |
| Low-latency playback | A severe user-reported lag remains open | Measured UI/audio skew at cold/warm start, p50/p95 |
| 8-note demo | Program exists in UI source | All notes audible reliably on output hardware |
| Audio decoding | Symphonia unit tests in engine | Formats, channels, long-file, bad input, resampling |
| Session clip launch | UI only displays/selects grid | Beat-quantized launch/scene/stop semantics tested |
| Browser | Concurrent uncommitted implementation observed | Scan bounded, filters work, sound preview safe, no RT stalls |
| Plugins | Host/IPC crates are placeholders | Sandbox process loads signed/test plugin, state, crash recovery |
| Project serialization | JSON code exists | Save/reopen, missing media, compatibility migration tests |
| Recording | Not implemented | Capture/monitor/latency and dropout tests |
| .als interchange | Not implemented | Legal fixture corpus, compatibility read tests, lossy warnings |

## Automated commands

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
```

On Linux CI, system native dependencies must be installed. Avoid CI jobs that play audible test tones.

## Benchmark template

```text
Commit:
Date / OS / kernel:
Audio interface / selected backend:
Actual sink and device routing:
Sample rate / requested quantum / measured quantum:
Project tracks / clips / events / media source duration:
Cold vs warm: 
Start press -> first nonzero generated sample:
Start press -> first physical output (measured method):
GUI transport -> audible skew:
CPU / xruns / queue overflows:
Result + trace location (redacted):
```

Do not report frame buffer size as a latency benchmark. PulseAudio buffering, graph disconnected ports and the physical audio path matter.
