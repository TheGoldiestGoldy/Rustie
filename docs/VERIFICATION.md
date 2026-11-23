# Verification ledger

Different checks answer different questions. Test counts belong to a source snapshot, not to the product forever.

| Snapshot | Evidence | What it does not establish |
|---|---|---|
| 21 November integration, `028e2b1a8cd5` | 154 workspace tests; eight realtime allocation guards; format/Clippy/release checks; 615.816 s virtual recording, zero drops; native editing, Session, FX, SoundFont and named plug-in exercises | Prompt physical startup, headphone listening, analogue alignment, universal compatibility or release readiness |
| 22 November scoped mic candidate, base `bf52dee0003b` plus mic patch | 159 tests; eight allocation guards; two opt-in tests ignored by default; matched UI/worker build; bounded source-switch and removal tests; 11.27 s AIR mono take; save/cold reopen, centered PCM/export and stereo-import checks | Acceptance of concurrent seek code, acoustic latency or owner headphone clarity |
| 22 November owner mic check | Owner confirmed the pre-REC meter moved; agent validated a saved mono take | Clear spoken playback in both ears remains open |
| 22 November seek working state | Source changes and partial agent test progress; additional regressions in the snapshot | Final agent completion, native mouse/audio acceptance and measured repeated-seek latency |
| 21 November Autonatural Lab | 90 tests; synthetic render/import at two offsets; unchanged source bytes | Real-vocal naturalness or live pitch correction |

## 23 November instrument draft branch

The isolated beta/sf16-lnf1-foundation branch passed **173 workspace tests**: 30 daw-core, 52 daw-engine, 11 realtime-guard integration tests, 6 plug-in-host and 74 UI tests. Two opt-in PulseAudio tests were ignored. cargo fmt --all -- --check, strict workspace Clippy, cargo build --release -p daw-ui --locked and git diff --check passed on the sanitized branch.

This verifies compilation and automated tests, not physical audio, hardware MIDI, LNF-1 manual GUI interaction, sample parity or general host automation. No speaker/headphone acceptance was performed. The two current screenshot files whose footer showed a local QA path have had that footer removed. This PR does not rewrite public history.

## Independent publication snapshot checks

The combined snapshot passed **164 tests**, including **nine realtime allocation checks**, with **two opt-in PulseAudio tests ignored**. Formatting and strict Clippy also passed. Build and publication results are recorded in the [publication report](agent-reports/2026-11-22-github-publication-report.md). These run on the isolated combined source identified by [file hashes](source-snapshot-sha256.json), preserving the active integration worktree.

No new physical audio capture or owner-listening claim is made by the publication pass. No current screenshot is used as proof that the uncompleted seek acceptance passed.

## Reproduction entry points

```bash
cargo fmt --all -- --check
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo build --locked --release -p daw-ui -p daw-plugin-host
```

Opt-in audio tests require their documented private fixtures. Do not reinterpret ignored cases as passes. A silent probe still opens an audio device; use a safe test environment.

Detailed sources: [baseline report](agent-reports/2026-11-21-beta-integration-followup-report.md), [mic report](agent-reports/2026-11-22-mic-workflow.md), [routing/level analysis](agent-reports/2026-11-22-mic-routing-and-level.md), [release gates](BETA1_RELEASE_DECISION.md), [Lab report](research/AUTONATURAL_LAB_PORT_REPORT.md).
