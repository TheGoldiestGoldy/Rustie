# Autonatural development report | 2026-11-17

**Status: Partial working prototype**
**Workspace:** `/home/developer/Projects/Autonatural` (separate from `~/Projects/rust-daw`)

## Implemented
- Parsed six original zip specifications (`spec/`).
- Built `nt-core` Rust library: YIN pitch detection, note center segmentation, scale parsing, interval-aware target selection, soft-knee tuning correction and experimental TD-PSOLA.
- Built `nt-cli` offline 32-bit float WAV encoder with A/B variants and synthetic test fixture.
- Built `nt-clap` native CLAP effect: parameter automation, worker thread, lock-free processing queue, 400 ms host-reported latency, mono/stereo.
- Vendored ISC-licensed NIH-plug with two CLAP state patches needed for validator compliance.
- Measured a synthetic 431.35 Hz center moving to 435.80 Hz, with a target of 440 Hz; full naturalness not established.

## Verification
- `cargo fmt --all -- --check`: passed
- `cargo clippy --workspace --all-targets -- -D warnings`: passed before packaging
- `cargo test --workspace`: all 12 Rust tests passed after packaging source changes, including native worker F0
- `cargo build --release --workspace`: passed
- `clap-validator 0.4.1`: 44 cases; 34 passed, 10 skipped, 0 failed
- Test fixture + offline A/B WAVs generated and measured under `artifacts/synthetic-ab/`.
- Real vocal recordings, actual hardware listening and live in-DAW playback were **not tested**.

## Limitations
- Rustie has no working CLAP host; cannot instantiate plugin inside Rustie yet.
- No plugin custom UI, pitch roll, manual targets, MIDI input or A/B control beyond bypass.
- Simplified pitch/segmentation; 400 ms latency and potential boundary artifacts.
- No real-vocal sound evaluation, no complete G1-G11 gate pass.
- No Python reference implementation/parity suite despite original M0/M1 requirement.

## Deliverables
- Source at `~/Projects/Autonatural`
- Release executable `target/release/nt`
- Release CLAP library `target/release/libnt_clap.so`
- Installed `~/.clap/Autonatural.clap` (verified by file and clap-validator)
- Synthetic samples `artifacts/synthetic-ab/`
- `README.md`, `THIRD_PARTY.md`, `docs/DECISIONS.md`, `docs/PLUGIN_VALIDATION.md`
- Source/binary/audio release archive under `~/Downloads/Autonatural-experimental-2026-11-17.zip`

## Next-agent handoff
1. Read `spec/AGENTS.md`, all six source documents, this report and `docs/PLUGIN_VALIDATION.md`.
2. Rerun `cargo test --workspace`, strict Clippy, release build, clap-validator.
3. Build real-vocal evaluation and verify G3/G5/G6/G7/G8/G9/G10/G11 before trusting correction. Keep default bypass on uncertain material.
4. Optimize worker throughput and transition seams, obtain actual host 48k/96k playback and latency metrics.
5. When core is audibly convincing, implement the Midnight Ember-compatible native GUI and Rustie plugin hosting.
6. Do **not** overwrite files in the Rustie repository; concurrent work is ongoing.
7. Recreate a report in `docs/agent-reports/` after future changes, listing truthful checks, screenshots/audio and next priorities.
