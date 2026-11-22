# Rustie / Slate Studio: Agent 4 realtime MIDI instrument delivery

**Date:** 20 November 2026 UTC
**Project:** /home/developer/Projects/rust-daw
**Starting reference:** docs/agent-reports/2026-11-20-beta-five-blocker-hardening.md
**Git:** master, starting HEAD f578b9a; pre-existing extensive modified/untracked files preserved. No reset, commit, push, branch change or user audio/project overwrite.

## Outcome

**Real, experimental realtime MIDI-instrument hosting now works for installed CLAP and VST3 Surge XT**, in separately supervised plugin-host child processes. Notes travel from the actual graph's MIDI nodes through fixed-size, sample-offset MIDI packets to CLAP/VST3, and stereo audio returns through the pre-existing fixed-latency, lock-free bridge. Both formats passed an end-to-end headless synthetic MIDI-note graph probe with nonzero audio and **zero missed blocks** in final runs. This is not proof of universal VST3/CLAP compatibility, hardware round-trip latency, plugin-state recall or actual human audition.

## Source changes in this pass

- crates/daw-engine/src/live_plugin.rs: each 128-sample audio block now carries a preallocated maximum of 64 timestamped MIDI events (note-on, note-off, all-notes-off). All MIDI operations on the callback are fixed-memory and bounded. Overflow replaces the last event with all-notes-off and increments telemetry rather than silently leaving hanging notes. Reset adds panic to protect against stale notes after seeking.
- crates/daw-engine/src/effects.rs, graph.rs: MIDI note commands are forwarded to live plugin instruments; all-notes-off also reaches instruments. A successful external plugin replaces the diagnostic built-in sine oscillator on that node, preventing doubled sound; if activation fails, the sine remains available. The graph sends MIDI events with offsets in the current 128-frame audio block. Unit tests check note on/off, sample offsets, over-capacity panic, and graph-level routing.
- crates/daw-plugin-host/src/clap_stream.rs: new stream-clap-instrument mode with 1024 bytes of stereo PCM plus 1-byte event count and 64 fixed 4-byte MIDI entries, sound generator uses zero audio-input ports and the CLAP note event API. Verifies the CLAP class declares 'instrument'. A one-byte 'R' readiness marker is emitted **only for instrument mode** after activation.
- crates/daw-plugin-host/src/vst3_stream.rs: new stream-vst3-instrument mode, 0-in/2-out 128-frame host with sample-offset send_midi_event_at and matching fixed-size MIDI/audio framing and readiness marker. Existing VST3 audio-effect path retained.
- crates/daw-plugin-host/src/main.rs: dispatches both instrument worker modes.
- crates/daw-ui/src/studio_live_plugins.rs: starts CLAP/VST3 instruments from a MIDI track; waits for the child activation marker off the audio callback (bounded timeout) so an invalid or slow class does not silently displace the built-in synth. Parent stdin **and** stdout are nonblocking, and full packet writes/reads have absolute deadlines. Child stalls cannot block the realtime audio callback; missed instrument output is silence, while effect misses remain delay-matched dry audio.
- crates/daw-ui/src/studio.rs: connects selected MIDI-track instrument to Arrangement/Session MIDI nodes; preserves track/group effects, monitor note routing and source gain; adds 512-frame plugin latency compensation metadata.
- crates/daw-ui/src/studio_plugin_render.rs: compact Plug-ins tab now offers CLAP or VST3 instrument insertion for MIDI tracks and CLAP/VST3 effect insertion for Audio/Group, with honest warnings. Existing persisted Track.live_clap field deliberately remains unchanged for backwards compatibility with old sessions.
- crates/daw-ui/src/main.rs: new CLI probes --probe-live-clap-instrument BUNDLE.clap CLASS_ID and --probe-live-vst3-instrument BUNDLE.vst3.
- docs/FEATURE_MATRIX.md, AGENTS.md: updated verified state, next tasks.

## Final verification (observed, not inferred)

| Check | Result |
|---|---|
| cargo test --workspace --quiet | **PASS: 86 tests** (19 core, 36 engine, 3 engine RT, 3 plugin-host, 25 UI) |
| cargo clippy --workspace --all-targets -- -D warnings | PASS |
| cargo fmt --all | PASS |
| cargo build --release -p daw-ui -p daw-plugin-host | PASS |
| git diff --check | PASS |
| target/release/daw-ui --probe-live-clap-instrument '/usr/lib/clap/Surge XT.clap' org.surge-synth-team.surge-xt | **PASS: 550 blocks @ 44100 Hz, peak 0.34711, 0 missed blocks** |
| target/release/daw-ui --probe-live-vst3-instrument '/usr/lib/vst3/Surge XT.vst3' | **PASS: 550 blocks @ 44100 Hz, peak 0.32892, 0 missed blocks** |
| Invalid CLAP MIDI instrument class (LSP limiter) | Rejected: exit 2, readiness marker absent; no silent false success |
| target/release/daw-ui --probe-live-clap /usr/lib/clap/lsp-plugins.clap in.lsp-plug.limiter_stereo | PASS: 400 blocks, peak 0.15000, 0 dry fallback |
| target/release/daw-ui --probe-live-vst3 '/usr/lib/vst3/Surge XT Effects.vst3' | PASS: 400 blocks, peak 0.15916, 16 dry fallback blocks |
| target/release/daw-ui --probe-recording | PASS: AIR 192 6 analog stereo, 44100 Hz, 15,082 frames, 342 ms WAV, 0 dropped; temp WAV removed |
| target/release/daw-ui --check-project '/home/developer/Music/Rustie UX Review/Rustie UX Review.slate.json' | PASS: four tracks, eight clips, two WAVs decoded |
| Native GUI startup on a **temporary project copy** | Survived 5s; intentional timeout exit 124; no captured stderr errors. NOT hands-on mouse/audio testing. |

The earlier instrument synthetic probe before readiness hardening missed 25/550 blocks; the final CLAP probe missed zero. Do not extrapolate that to real heavy projects.

## Known limitations and risks

1. **Plug-in state/preset recall is NOT implemented.** Project JSON persists bundle path, CLAP class and enabled flag, not the plug-in's full native state. Preset changes inside a plugin cannot yet be saved/reloaded. There is no native plugin editor/parameter automation.
2. **Compatibility is not universal.** Two Surge XT instrument formats and two effect samples were tested; third-party plugin behavior varies. Installed LSP VST3 bundle still fails the metadata parser. No MIDI channel, pitch bend, sustain CC or aftertouch forwarding into external plugins; instrument note events currently use channel 1.
3. The 512-frame bridge introduces **approximately 11.6 ms of DSP bridge delay at 44.1 kHz**, in addition to unmeasured sound-server/hardware and plugin processing delay. A dropped instrument block is silent, not sine fallback. GUI audible playhead alignment, device round-trip latency, XRUNs and complex mixes remain unverified.
4. In Session view the current architecture can start independent instrument helper processes for multiple MIDI clip nodes. CPU, RAM, polyphony and crash recovery under dozens of loaded instances are not characterized. Limit active plugins until tested.
5. A 64-event burst beyond the fixed MIDI block capacity becomes a panic event. This is safer than hanging notes but drops some events; larger MIDI stream capability (CC, bends, channels, scalable scheduling) remains required.
6. Voice/guitar monitoring, project WAV capture/reopen, real hardware MIDI key-press listening, compact Devices tab mouse flow, effect musical quality and real instrument preset usage still await human verification.

## Next agent instructions

**Do not restart this project or rewrite the bridge.** Read AGENTS.md and this report, inspect git status before editing, and verify the latest source. Prioritize **native plugin state/preset save/restore** in the supervised worker with an explicit size cap and versioned backward-compatible JSON, then VST3 metadata compatibility, MIDI CC/sustain/pitch bend and multi-instance resource handling. Keep sample-offset scheduling and bounded nonblocking worker IPC. Add save/reopen integration tests before claiming session recall. Conduct physical speaker/MIDI/microphone GUI testing before first beta release; document measured end-to-end latencies rather than relying on host timestamps.

**Reproduce:** cargo test --workspace --quiet; cargo clippy --workspace --all-targets -- -D warnings; cargo build --release -p daw-ui -p daw-plugin-host; run both --probe-live-*-instrument commands above plus existing audio-effect and recording probes. Do not change sound-server defaults, overwrite the user's projects, reset the worktree or commit/push without permission. End with a new detailed dated report and the next-agent instructions.
