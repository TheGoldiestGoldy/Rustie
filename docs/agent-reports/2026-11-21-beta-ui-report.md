# Wave 2 Devices/UI handoff

**21 November 2026 · Implemented on `beta/ui`.**
Worktree `/home/developer/Projects/rust-daw-agent-ui`. Base `e6b82a79feeac4158f1918b3f5598916d5bb177c`. UI source commits `caf7b9b`, `1897111` and `f971188`.

## 1. Behavior and changed files

| Area | Files under `crates/daw-ui/src/` | Behavior |
| --- | --- | --- |
| Library and discovery | `studio_browser.rs`, `studio_plugin_discovery.rs` | Fixed sidebar width, per-category search and real add/import actions. Bounded bundle discovery and one isolated CLAP/VST3 inspection at a time. Displays class, vendor, category and inspection errors. Stale results cannot replace the latest selection. LV2 is discovery-only. |
| Devices and DSP controls | `studio_devices.rs`, `studio.rs` | Track selector, scrollable FX Chain, Plug-ins and Audio/MIDI tabs. Built-in FX bypass, ordering and parameters edit the project. EQ8 uses the actual filter response, 22-point handle hit areas, fixed frequencies/Q, numeric gains and drag/double-click on gain. EQ3 and both reverbs expose the existing DSP parameters. |
| Plugin inserts | `main.rs`, `studio_live_plugins.rs`, `studio_plugin_render.rs`, `studio.rs` | Live assignment requires a successfully inspected, compatible class. Devices shows idle, bypass, missing bundle, activation failure and stopped workers with fallback counts. Bypass captures state from the selected track's active worker. Offline VST3 render remains independent of live inspection. |
| Navigation and layout | `studio_help.rs`, `studio_views.rs`, `studio.rs`, `studio_piano.rs`, `studio_session.rs` | Devices remains near File with a shortcut and a tab context menu. Related metronome and output controls stay together as the transport wraps. The resizable detail area preserves access to Mixer and Piano roll. Help presents a skippable guide. Session edits are limited to the header layout. |

The change adds no project fields, worker protocol changes, dependencies, or audio-thread work. The UI observes the bridge thread's existing lifetime and missed-block counter. Persisted VST3 bindings still use the bundle's host-selected class. CLAP keeps the selected class ID.

## 2. Native captures

Window-only captures of the release app built from `f971188`, showing the synthetic review project after a cold reopen. The branch uses the branding from the assigned base.

| View | 1540 × 940 | 980 × 640 |
| --- | --- | --- |
| Arrangement and Devices | [EQ8 and FX Chain](../screenshots/beta-ui-arrangement-devices-1540.png) | [Devices at minimum size](../screenshots/beta-ui-arrangement-devices-980.png) |
| Session and Mixer | [Session and Mixer](../screenshots/beta-ui-session-mixer-1540.png) | [Session and Mixer at minimum size](../screenshots/beta-ui-session-mixer-980.png) |

At both sizes the Click toggle and volume stayed together, as did the output selector and master peak. The footer shortcut hint fit. The reopened Devices window showed EQ8 at +16.2 dB, followed by the bypassed EQ3. The Plug-ins tab showed the saved Surge XT insert assigned with the engine idle and no native state saved.

## 3. Verification

Commands ran in the Devices/UI worktree with `CARGO_TARGET_DIR=/home/developer/.cache/rustie-target-ui`.

| Check | Result |
| --- | --- |
| `cargo fmt --all -- --check` | Passed |
| `cargo test --workspace` | **122 passed**, zero failures (22 core, 38 engine, 4 real-time checks, 3 plugin-host, 55 UI) |
| `cargo clippy --workspace --all-targets -- -D warnings` | Passed |
| `cargo build --release -p daw-ui -p daw-plugin-host` | Passed |
| `git diff --check` and staged whitespace check | Passed |
| Focused coverage in the workspace suite | 120 idle frames at each window size, EQ8 drag across other bands and inert clicks away from handles, class/vendor/role parsing, stale scan rejection, FX chain/state serialization and existing RT allocation guards |
| Worker `inspect-clap` and `inspect-vst3` | Surge XT 1.3.4 reported the expected class, vendor and instrument category in both formats. LSP VST3 exited 1 with conflicting class metadata. |
| UI `--probe-live-clap-instrument` and `--probe-live-vst3-instrument` | Surge XT, 550 blocks at 44.1 kHz each, zero missed blocks in both runs. Peaks 0.34119 CLAP and 0.34223 VST3. Bridge latency 512 samples. The probes process the graph headlessly. |

Native pointer and key input exercised the compiled GUI on an isolated Xwayland display with a synthetic review project and a private home. Skip wrote the first-run marker and F3 opened Devices. In the Library, adding EQ3 opened the selected track's chain. The EQ8 drag raised only its 350 Hz band to +16.24 dB. EQ3 low reached +16.0 dB, and bypass plus Move down put EQ8 before the bypassed EQ3. At 980 × 640, scrolling reached both reverbs and their three controls. Saved wet/decay/damping values were 0.67 / 2.17 s / 0.73 for Vocal and 0.10 / 1.77 s / 0.88 for Guitar. Audio/MIDI opened at minimum size. In the Session Mixer, the wheel reached pan and routing at 980 × 640. A pointer drag moved the first strip's pan to R 50 in memory. Right-click opened clip and track actions in Arrangement and Session. Ctrl+F focused the Library, where a search for `surge` returned two CLAP and two VST3 bundles.

Surge XT CLAP displayed its vendor, instrument category and class. Assigning it to MIDI and pressing Ctrl+S wrote an enabled insert with class `org.surge-synth-team.surge-xt`. LSP VST3 displayed its metadata error and disabled Replace. The previously assigned Surge insert stayed on the track. Choosing the Surge XT Effects CLAP on the MIDI track displayed the Audio/Group requirement and disabled replacement. The release UI accepted the saved fixture through `--check-project` and reported four tracks, eight clips and two decoded audio files.

The dedicated test and Clippy log is `/home/developer/.cache/rustie-target-ui/wave2-gate.log`.

## 4. Remaining acceptance

Physical audio, mic/MIDI hardware, owner mouse acceptance and native plugin-state save versus a cold GUI reopen remain open. The VST3 host reports one class per bundle. Runtime worker-exit labeling remains unverified in the GUI. Runtime fallback reporting does not diagnose the plug-in's internal error. The LSP VST3 metadata issue reproduces in the worker. FX control changes still use the existing graph rebuild after pointer release.

## 5. Integrator verification

On the combined candidate, open Devices at 980 × 640 with F3 and via the right-click tab menu. Select a track, add EQ3 and EQ8, drag a gain, bypass one effect, reorder the chain, save and reopen. Repeat with the Session Mixer visible. Check a supported live instrument and effect, an incompatible track role, and an inspection failure. With a test output enabled, confirm the stopped-worker label and fallback on a worker exit. Use the owner sign-off and release gate from the canonical integration worktree.

## 6. Git and evidence

Source, tests, this report, and native captures of synthetic data are committed locally on `beta/ui`. The branch was neither merged nor pushed. The captures exclude user project data, recordings, desktop contents and plug-in binaries.

## 7. Handoff to Agent 07

Merge the local `beta/ui` branch from the current integration tip with the existing Wave 2 Session delivery. Merge previews require review of `main.rs`, `studio.rs`, `studio_devices.rs`, `studio_help.rs`, `studio_plugin_render.rs` and `studio_views.rs` for integration branding and symbol overlaps. Keep the Rusty version/build stamp and a single `crate::plugin_worker_path()` for the installed and Cargo worker names. Preserve integration's plugin-state identity validation in `studio_live_plugins.rs` alongside the bridge runtime handle. Preserve Session's independent slot lookups and phase/scheduling logic, and apply the UI-only header changes deliberately in `studio_session.rs`. Keep the track monitor instance first where the state capture looks up its runtime. Rerun the shared gate and repeat the native checks on the final installed build. Use this report to update the integration-level handoff after the merge.
