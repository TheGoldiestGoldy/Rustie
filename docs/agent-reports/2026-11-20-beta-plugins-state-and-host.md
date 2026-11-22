# Rustie / Slate Studio: Agent 01 | Plugin Runtime and State Report

**Date:** 20 November 2026 UTC
**Status:** Partial (state runtime implemented and verified; full live acceptance pending)
**Workspace:** `/home/developer/Projects/rust-daw-agent-plugins` (isolated plugin-agent worktree)
**Agent:** 01 (Plugin Runtime and State)

## 1. What I implemented

### Shared base and versioned plugin-state representation
- New crate module `crates/daw-core/src/plugin_state.rs`: `PluginStateBlob` with
  version (`PLUGIN_STATE_VERSION = 1`), format (`clap`/`vst3`), plugin uid, capture
  sample rate and hex-encoded raw bytes; hard cap `MAX_PLUGIN_STATE_BYTES = 262_144`
  raw bytes; validation rejects wrong version, wrong format, bad hex and oversized
  payloads. Missing JSON fields deserialize with safe defaults (old projects load).
- Exported through `daw-core/src/lib.rs`.
- `crates/daw-core/src/model.rs`: `Track.live_clap.state: Option<PluginStateBlob>`
  added with `skip_serializing_if = "Option::is_none"` so pre-state projects keep
  opening. Old JSON without `live_clap` or `state` continues to deserialize.

### Live worker control, state protocol, latency handshake, restore
- `crates/daw-engine/src/live_plugin.rs`: added `LivePluginControl` (state request
  flag + condvar-bounded state answer + reported latency/tail) on both
  `LivePluginEffect` and `LivePluginWorker`; public `capture_state(timeout)`, worker
  `take_state_request`/`deliver_state`, `latency_samples`/`tail_samples`,
  `set_reported_latency`. Capture path is mutex+condvar only (UI/worker threads), never
  the audio callback.
- `crates/daw-plugin-host/src/clap_stream.rs`: new binary protocol — parent packet is
  `opcode(u8) + 128-frame stereo f32 audio (+MIDI trailer for instruments)`; child reply
  is `status(u8) + audio (+u32 state length + state on success)`. Opcode bit 0 requests a
  native state save; status bit 1 marks failure. Startup readiness is now a fixed
  `b"R" + latency u32 + tail u32` packet for CLAP effects, CLAP instruments and VST3
  effects/instruments. Added CLAP state/latency extension wrappers (main-thread worker only)
  and an optional state file argument for restore
  (`stream-clap-effect BUNDLE CLASS RATE [STATE]`,
  `stream-clap-instrument BUNDLE CLASS RATE [STATE]`).
- `crates/daw-plugin-host/src/vst3_stream.rs`: same opcode/state protocol, VST3 state save
  (`save_state`) / restore (`load_state`), readiness packet with latency/tail, optional
  state file
  (`stream-vst3-effect BUNDLE RATE [STATE]`,
  `stream-vst3-instrument BUNDLE RATE [STATE]`), and native state capture CLI helpers.
- `crates/daw-ui/src/studio_live_plugins.rs`: worker thread sends requested opcodes with
  150 ms (first block 2.5 s) absolute-deadline reads/writes, answers state through the
  bounded control channel, uses the announced plugin latency for the graph delay
  compensation sum, and writes accepted state blobs to private 0600 temp files for the
  child to load at startup.

### Project save integration
- `crates/daw-ui/src/studio.rs`: new `live_clap_controls: HashMap<u64, LivePluginControl>`
  (cleared on every live graph rebuild, since workers are replaced then), capture of
  validated state via `capture_live_plugin_states()` before each explicit project save, and
  reported plugin latency added to the fixed bridge delay for graph delay compensation.

### Descriptor inspection
- `crates/daw-plugin-host/src/clap_render.rs`: new `inspect-clap BUNDLE` printing
  `format=clap id=… name=… vendor=… version=… features=…`.
- `crates/daw-plugin-host/src/vst3_stream.rs`: new `inspect-vst3 BUNDLE` printing VST3
  module metadata (name, vendor, version, category, uid, audio/midi/bus and gui flags,
  reported latency/tail).
- `crates/daw-plugin-host/src/main.rs`: new CLI modes and optional `[STATE]` args for the
  live stream workers, plus `save-state-*` native-state capture modes.

## 2. Current state / screenshots

- No UI appearance change was made; no screenshot was captured during this pass.
- Implemented in source and covered by unit tests (state validation, old-JSON default, worker
  protocol invariants).

## 3. Verification

| Check | Exact command or procedure | Observed result |
|---|---|
| Rust format | `cargo fmt --all -- --check` | PASS (after `cargo fmt --all`) |
| Rust tests | `cargo test --workspace` | PASS: **91 passed**, 0 failed (22 core incl. 3 new state tests; 38 engine; 3 RT; 3 plugin-host; 25 UI) |
| Clippy | `cargo clippy --workspace --all-targets -- -D warnings` | PASS |
| Release | `cargo build --release -p daw-ui -p daw-plugin-host` | PASS (release profile, `CARGO_TARGET_DIR=/tmp/rustie-target-plugins`) |
| Live CLAP effect probe | `daw-ui --probe-live-clap /usr/lib/clap/lsp-plugins.clap in.lsp-plug.limiter_stereo` | PASS: 400 blocks, 44.1 kHz, peak 0.15000, dry fallback 0 |
| Live VST3 effect probe | `daw-ui --probe-live-vst3 /usr/lib/vst3/Surge XT Effects.vst3` | PASS: 400 blocks, 44.1 kHz, peak 0.15916, dry fallback 0 |
| Live CLAP instrument probe | `daw-ui --probe-live-clap-instrument /usr/lib/clap/Surge XT.clap org.surge-synth-team.surge-xt` | PASS: 550 blocks, 44.1 kHz, peak 0.35225, missed 0 |
| Live VST3 instrument probe | `daw-ui --probe-live-vst3-instrument /usr/lib/vst3/Surge XT.vst3` | PASS: 550 blocks, 44.1 kHz, peak 0.36180, missed 0 |
| Worker state capture (CLAP) | `daw-plugin-host save-state-clap-instrument /usr/lib/clap/Surge XT.clap org.surge-synth-team.surge-xt 44100 STATE-IN STATE-OUT` | PASS: default state 50,310 bytes; re-capture 50,310 bytes (identical sha256 `f564c207…`) |
| Worker state round-trip (VST3) | `daw-plugin-host save-state-vst3-instrument /usr/lib/vst3/Surge XT.vst3 44100 STATE-IN STATE-OUT` | PASS: state 52,030 bytes, restore+save hash-identical (`19400c82…`) |
| Worker state capture (VST3 effect) | `daw-plugin-host save-state-vst3 '/usr/lib/vst3/Surge XT Effects.vst3' 44100 STATE` | PASS: state 1,150 bytes |
| Worker state capture (CLAP effect) | `daw-plugin-host save-state-clap /usr/lib/clap/lsp-plugins.clap in.lsp-plug.limiter_stereo 44100 STATE` | PASS: state 393 bytes |
| Descriptor inspect | `daw-plugin-host inspect-clap …`, `daw-plugin-host inspect-vst3 …` | CLAP lists classes (features reported); LSP VST3 fails moduleinfo validation: `old class 56535461623200612F62207465737465 maps to both …` |
| Git whitespace | `git diff --check` | PASS |

**Latency numbers:** all live probes reported bridge delay 512 samples plus plugin-reported
latency 0 samples at 44.1 kHz.

## 4. Known issues and limitations

- All live probes are headless synthetic-block probes; no speaker listening, physical
  MIDI key test, or human microphone/speaker validation happened in this pass.
- Plugin-native state recall is wired end to end (model + worker save/load + restore +
  capture), and real state blobs were captured/round-tripped from installed Surge XT CLAP/VST3
  and LSP CLAP limiter workers. No **runtime save from the GUI** has been exercised yet
  (only explicit worker CLI capture paths), and autosave does not yet call
  `capture_live_plugin_states`.
- LSP VST3 bundle remains rejected by the host metadata parser (confirmed in
  `inspect-vst3`; conflicting class mapping), while its CLAP limiter state path works.
- No plugin-native parameter editor/automation was added; state recall therefore restores
  the captured blob at activation.

## 5. What remains: next priorities

1. Exercise the GUI save path: `capture_live_plugin_states` → persisted `live_clap.state` →
   cold-launch restore.
2. Final integration: integrator merges Wave 1 work, reviews the shared `daw-core`/
   `daw-engine`/`daw-ui`/`daw-plugin-host` changes, resolves version bumps (IPC version/frame
   size unchanged: 128-frame blocks, state opcode/status additions).
3. Real hardware speaker/microphone latency and manual device acceptance remain open.

## 6. Git state and deliverables

- Branch: `beta/plugins`; Worktree: `/home/developer/Projects/rust-daw-agent-plugins`;
  BASE SHA (coordinator baseline): `1ae60bb2106b595e7e46c33911753ee45eb6d6f6`.
- Committed: No. Pushed: No.
- Changed/untracked files: implementation sources listed in section 1 plus this report.
- New artifacts/state files in `/tmp` (not committed): `/tmp/rustie_clap_state_*.bin`,
  `/tmp/rustie_vst3_state_*.bin`, `/tmp/rustie_clap_lsp_state.bin`, `/tmp/rustie_vst3_effect_state.bin`.

## 7. Next-agent handoff

Read `AGENTS.md`, `docs/beta-plan/08-AGENT-CONTRACT.md`, the prior plugin reports, and the
current implementation before further edits. Do not touch another agent's worktree or the
shared main checkout. The next narrow milestone is: verify live state save/load and finish
the acceptance items from `docs/02-PLUGIN-AGENT.md` without weakening integrity checks.
