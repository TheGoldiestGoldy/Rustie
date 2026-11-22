# Rusty Beta 1 plugin runtime hardening

21 November 2026. Specialist branch `beta/plugin-hardening` in `/home/developer/Projects/rust-daw-agent-plugin-hardening`. Coordinator base `1b717f971ddfe5c9534ea117d77f89e49de3cba8`. Implementation and tested source `30da37d735cee3486496c5caa8e4dc444a40c16b`.

## Implementation

- `daw-engine/src/{engine,graph,effects,live_plugin}.rs` and `daw-ui/src/{studio,studio_midi_input,studio_live_plugins}.rs` carry the input channel, note on/off, sustain CC64, pitch bend and panic through fixed-size events with offsets inside 128-frame blocks. A full event block ends with panic. A full worker ring carries panic into the next admitted block at offset zero. The UI retries affected-node panic when the engine command queue is full and clears held notes after MIDI ingress overflow.
- `daw-plugin-host/src/{main,clap_stream,vst3_stream}.rs` routes raw MIDI to CLAP instruments that advertise MIDI input on the first note port and retains native CLAP note events for other instruments. VST3 sends channel and offset on notes and uses the plugin's controller mapping for CC64 and pitch bend. Panic releases sustain and sends note-offs for held VST3 notes. The render bridge remains bounded and lock-free.
- The live VST3 worker honors an explicitly bound current class and reports its actual class on startup. Explicit Save records that class on both the insert and captured state. Version 1 project JSON and the 262,144-byte state cap are unchanged. Legacy inserts without a recorded class use the existing bundle-default selection. Previously unbound legacy state remains readable. CLAP state writes and both live workers' state-file reads enforce the project limit in the child. The bridge also polls for child exit while idle.
- Explicit Save requests fresh state from running workers and keeps a previous snapshot if capture fails. Autosave continues to serialize the current project model, including any previously captured state. Saving with the engine idle yields no new worker snapshot.
- `daw-ui/src/{studio_views,studio_export}.rs` labels the command as Arrangement export and blocks it when a live plugin is enabled. The prior integration report reproduced identical PCM with enabled and bypassed inserts because the offline graph excluded live processing. The native verification here exercised the new guard. To include a plugin part, import printed audio and bypass the insert. The existing selected-clip render works separately for supported CLAP/VST3 bundles and is independent of the running plugin's preset and full-track chain. Exact-state or full-track printing remains open.

## Native GUI and compatibility

Used a private Xwayland display and PulseAudio Null Output at 44.1 kHz with disposable projects. For each pair, native Save produced version-1 blobs, cold open restored deliberately changed native values, and a repeated cold save serialized identical state. The test modified the preset name for both Surge instruments, one effect parameter for Surge XT Effects and the limiter's output gain. Both previously blank VST3 insert IDs were bound to the class the worker actually loaded.

| Installed bundle | Class ID | Plugin version | 44.1 / 48 kHz paced live probe | GUI cold state |
| --- | --- | --- | --- | --- |
| `/usr/lib/clap/Surge XT.clap` | `org.surge-synth-team.surge-xt` | 1.3.4 | PASS, 550 blocks each, 0 / 0 missed | PASS, 50,310 bytes |
| `/usr/lib/vst3/Surge XT.vst3` | `ABCDEF019182FAEB566D624153675854` | 1.3.4 | PASS, 550 blocks each, 0 / 0 missed | PASS, 52,030 bytes |
| `/usr/lib/clap/lsp-plugins.clap` | `in.lsp-plug.limiter_stereo` | 1.0.38 | PASS, 400 blocks each, 0 / 0 fallback | PASS, 393 bytes |
| `/usr/lib/vst3/Surge XT Effects.vst3` | `ABCDEF019182FAEB566D624153465854` | 1.3.4 | PASS, 400 blocks each, 0 / 0 fallback | PASS, 1,179 bytes after restoring and normalizing the edited parameter |
| `/usr/lib/vst3/lsp-plugins.vst3` | 24 ambiguous retired IDs | package 1.2.37-1.1 | FAIL, worker rejects installed metadata | NOT TESTED |

The instrument probes sent a note, sustain engagement, bend, note release, pedal release and panic through the real worker. Event-level tests verified MIDI channel, value and sample offset. A separate paced run recorded one miss per instrument at 44.1 kHz. Native playback with the virtual compositor also accumulated fallback blocks. The zero-miss runs describe the named paced probes.

LSP's installed metadata maps retired ID `56535461623200612F62207465737465` to `64737020616270326D2020206162326D` and `64737020616270327320202061623273`. A disposable fixture with ambiguous retired mappings removed loaded both current classes separately and returned their exact requested IDs. The installed bundle still fails closed, and its metadata and binary were left in place.

Screenshots and raw logs are under `/home/developer/.cache/rusty-target-plugin-hardening/qa-30da37d735ce/`. See GUI cold Save (local QA artifact; not distributed), export guard (local QA artifact; not distributed) and Session test (local QA artifact; not distributed).

## Verification

Dedicated target `/home/developer/.cache/rusty-target-plugin-hardening`, four Cargo build jobs, release stamp `RUSTY_BUILD_REVISION=30da37d735ce`. Focused native and headless probes used that worktree's release `daw-ui` and sibling `daw-plugin-host`.

| Command or procedure | Result |
| --- | --- |
| `cargo fmt --all -- --check` | PASS |
| `cargo test --workspace` | PASS, 137 tests, including seven realtime allocation checks and one that saturates the MIDI bridge |
| `cargo clippy --workspace --all-targets -- -D warnings` | PASS |
| `cargo build --release -p daw-ui -p daw-plugin-host` | PASS |
| `git diff 1b717f9 --check` | PASS |
| `daw-ui --check-project PATH` for the legacy review project and both freshly saved fixtures | PASS, each retained four tracks and eight Session slots, decoded two or three WAVs |
| Live CLAP/VST3 effect and instrument probes at 44.1 and 48 kHz | PASS per table, all produced nonzero PCM |
| `stream-clap-instrument`, `stream-vst3-instrument`, `inspect-vst3` negative probes | PASS, both workers rejected a 262,145-byte file, VST3 rejected a truncated snapshot envelope and an unknown current class |
| SIGABRT, SIGKILL and SIGSTOP sent to owned live effect worker processes at 44.1 kHz | PASS for containment, dry audio continued and all workers were reaped. CLAP returned 0 with 165–177 fallback blocks. VST3 returned 2 with 201–221 fallback blocks because deliberate failure crossed the probe's quality threshold. Complete runs lasted 1.23–1.28 seconds. |
| Native GUI hang, Save, Stop, Play and Save | PASS after playback advanced. The hung MIDI worker was reaped, the subsequent save retained its state, and clips and Session layout matched the original. Stop and Play started fresh workers and the next save captured valid state. |
| Native replacement and removal during playback | PASS. Replaced Surge XT CLAP with Surge XT VST3 and saved the new VST3 class and state. Removed it and saved the project with identical MIDI clips and Session slots. Prior worker PIDs disappeared on both graph rebuilds. |
| Native Arrangement export with enabled CLAP and VST3 inserts | PASS. File menu named the Arrangement export, and activating it reported that live inserts must be bypassed before writing. |

## Resources and remaining risks

- Short virtual playback used two external instruments and one VST3 effect. In Arrangement, three workers used 4.75% of one CPU core over 8.00 seconds and 181.7 → 182.0 MiB of summed RSS. In Session, two scenes were alternated across two instrument tracks with up to one and three simultaneous notes per tested slot. Six workers used 8.26% of one CPU core over 18.51 seconds and 463.7 → 467.9 MiB of summed RSS. Summed RSS can count shared pages more than once. Every sampled worker disappeared after Stop in both views.
- NOT TESTED for long-run plugin concurrency, maximum polyphony, backend XRUN and callback deadline measurement, physical MIDI or audible output. The short virtual run provides an initial resource measurement with the existing per-track and per-slot workers.
- Worker timeouts apply during IPC. An early GUI hang injection exceeded the harness's initial 1.9-second wait and needed targeted teardown after the harness terminated the UI. The later test after the audio graph advanced passed. A hung idle worker may remain suspended until the next request or graph teardown. Stop and Play is the tested restart path.
- VST3 raw legacy component payloads remain opaque and rely on vendor validation. Controller mappings and CLAP MIDI dialect support vary across plugins. LSP's installed VST3 bundle remains rejected. Full live-chain mixdown and hardware acceptance are open.

## Git and integrator handoff

Changes are committed locally on `beta/plugin-hardening`. Use the implementation commit above and this separate handoff commit. No merge or push was made. Build and distribute the UI and worker as a matched pair. VST3 appends the canonical ASCII class ID to its startup packet. Instrument MIDI packets retain their size, with the high nibble of the final byte carrying channel and the low nibble carrying event kind. There are no dependency, license or project-schema version changes. Keep older empty-UID project fixtures in the regression gate and preserve the Arrangement export guard until live-chain printing is verified.
