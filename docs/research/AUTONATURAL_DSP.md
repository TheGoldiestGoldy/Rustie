# Autonatural

**Experimental Rust vocal pitch correction** designed around preserving the singer's performance.

> **This is an engineering preview, not a production-ready autotune.** The first native CLAP build works on Linux and validates structurally, but no real vocal listening test, timbre gate, live latency benchmark or multi-host plugin GUI acceptance test has passed. Do not use it on an unrecoverable session.

Autonatural is built from the six design documents in `spec/`, originally supplied as `~/autonatural.zip`. It keeps the source analysis, decision logic and resynthesis separate so that the same DSP can run in the offline CLI and the native CLAP wrapper.

## Key idea

Instead of snapping the fundamental frequency in every audio frame to the closest 12-tone note, Autonatural measures the center of **contiguous vocal note regions**, assigns a target, and applies a **region-level pitch offset**, respecting a **tolerance band**. Vibrato and expression riding on top of the center are retained by default. Unvoiced/low-confidence sections are intended to remain dry.

This is not yet the complete algorithm documented in `spec/DSP_ALGORITHM.md`. In particular: YIN rather than probabilistic pYIN, simplified contour segmentation, no confidence-aware key autodetection, no verified automatic tuning-offset estimate, no long-note sag splitting, and PSOLA resynthesis still requires serious evaluation. This implementation does **not** reach all acceptance gates.

## What works now

- **Rust DSP crate** `nt-core`: pure Rust YIN F0 tracking, scale masks, note-level region planning, Viterbi interval-aware selection, smooth-knee tolerance curve, and experimental TD-PSOLA.
- **Offline CLI** `nt`: input mono/stereo PCM WAV at 8 kHz to 192 kHz, 16-bit, 24-bit, 32-bit integer or float (within supported decoder assumptions), output 32-bit float WAV with identical sample rate, channels and number of samples; `tune` and `ab` modes.
- **CLAP plugin** `Autonatural (Experimental)`: native Linux x86-64 `.clap`; mono and stereo; four automatable host controls (Tightness, Scale mode, Key root, Bypass); audio analysis/synthesis on separate worker; callback uses fixed-size lock-free rings and delayed dry fallback. **400 ms host-reported latency** at 48 kHz. 400 ms is **not** acceptable for live vocal monitoring.
- **Tests**: Rust unit tests, synthetic A/B renders, strict Clippy, full release build, and 44 CLAP validator cases (34 passed, 10 legitimately skipped; no failures using validator 0.4.1).

There is currently **no custom visual plugin editor or pitch roll**. A CLAP host provides its default parameter controls.

## Install on this Linux computer

The compiled plugin is at:
`/home/developer/Projects/Autonatural/target/release/libnt_clap.so`.

The installation is at:
`~/.clap/Autonatural.clap`.

Restart/rescan your **CLAP-capable** DAW, then search for **Autonatural (Experimental)**. If it does not show, verify the host actually supports CLAP and loads shared-object `.clap` files.

**Rustie caveat:** Rustie (the DAW in `~/Projects/rust-daw`) currently only **discovers** CLAP bundles, with no implementation of CLAP hosting or realtime plugin processing. Autonatural cannot yet process audio inside Rustie. It can be used in a separate CLAP host when available.

## Offline usage

From the project directory:

```bash
./target/release/nt tune input.wav output.wav --key "A minor" --tightness 50
./target/release/nt tune input.wav output.wav --key "F# dorian" --tightness 80 --tolerance 6
./target/release/nt ab input.wav artifacts/my-vocal-ab --key "C major"
```

The `ab` command creates `original.wav`, `tuned_default.wav`, `tuned_tight.wav`, `tuned_loose.wav`, and `REPORT.md`. Always listen to the files rather than relying solely on a pitch-meter report. The pitch-center planning considers the mono average of stereo channels. Use **dry monophonic vocals**, not mixed/polyphonic music.

`--key auto` currently uses **Chromatic** as a safe fallback, not intelligent key inference. Pick the actual key/scale in this preview.

## Building and running tests

```bash
cd ~/Projects/Autonatural
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --release --workspace
python3 eval/synth/generate.py
target/release/nt ab eval/fixtures/demo_vocal_like_48k.wav artifacts/synthetic-ab --key chromatic
python3 eval/measure.py
```

To validate the CLAP shared library, use [clap-validator](https://github.com/free-audio/clap-validator):
```bash
clap-validator validate ./target/release/libnt_clap.so
```

The included `vendor/nih-plug` is a permissively-licensed local copy of upstream NIH-plug (commit `de421011`) with two narrowly-scoped CLAP wrapper fixes. See `docs/DECISIONS.md` and `THIRD_PARTY.md`. Run a release build before installing an updated plugin.

## Audio performance and risks

- Plugin latency is a fixed **400 ms** nominally. In asynchronous real time, worker-thread scheduling may cause additional late processing and fallback to delayed dry; there is no instrument-grade guaranteed latency.
- The current PSOLA may introduce audible waveform discontinuities, grain flutter, level differences, and phase differences, especially at transitions, low-confidence audio, and between 200 ms worker blocks. Quality on human voices is **unverified**.
- Parameter automation is not yet sample-accurate at the worker analysis window level.
- Unsupported sample rates in the plugin use dry fallback; offline CLI supports up to 192 kHz.
- No anti-clip limiter is inserted; export a copy and verify peaks.
- The plugin modifies audio only while running in its host. No network, no telemetry, no access to source recordings beyond host-supplied buffers.

## Project map

```text
spec/                        Original supplied product/DSP/architecture/design/roadmap docs
crates/nt-core/              Pure Rust DSP, tests, note planning and resynthesis
crates/nt-cli/               WAV render + A/B command
crates/nt-clap/              Native Linux CLAP wrapper + streaming worker
vendor/nih-plug/             ISC framework with audited patches
eval/synth/                  Deterministic synthetic voice-like fixture generator
eval/fixtures/               48 kHz synthetic WAV + ground truth JSON
eval/measure.py              Numeric f0, vibrato-RMS and file integrity check
artifacts/synthetic-ab/      Rendered WAVs and reports, not tracked
docs/DECISIONS.md            Architectural deviation and patch reasons
docs/PLUGIN_VALIDATION.md    Measured results and remaining acceptance gates
docs/agent-reports/          Work report / next agent handoff
```

The name **Autonatural** reflects the user's request. Original specs sometimes say **NaturalTune**; they are kept unchanged for provenance.

## Next milestones

P0: real dry vocal reference recordings and comprehensive Gates G1-G11 from the spec; validate pitch tracker, region segmentation, chromatic errors, fixed-note targets, vibrato correlation, formant envelopes, clicks and realtime continuity. P1: robust algorithmic cleanup and more efficient streaming with 80-200 ms natural mode. P2: custom Midnight Ember-compatible pitch-roll UI, MIDI targets, user overrides, A/B transport, scale picker, Rustie host integration. See `spec/ROADMAP.md`.
