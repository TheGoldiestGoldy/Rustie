# Wave 2 Session handoff

**21 November 2026 · Implemented on `beta/session`**
Worktree `/home/developer/Projects/rust-daw-agent-session`
Clean starting commit `e6b82a79` · Feature commit `ffd8b74`

## Behavior and compatibility

The Session grid, clip editors and graph now use independent clip data. Each cell has a stable composite identity of track ID and scene ID. MIDI and audio tracks own sparse cells. Scenes have an explicit order and persist even when empty. Legacy load copies the old row contents, assigns fresh clip IDs, and preserves every Arrangement clip. Audio copies share their source file. Project clipboard, clip edits and undo/redo support the new IDs. The save format and migration example are in [Session engine](../SESSION_ENGINE.md).

The performer can choose sixteenth, quarter or bar quantization, replace pending targets, cancel pending launches, or quantize a stop for one track or the entire Session. The main transport Stop remains immediate. Session playback continues when browsing Arrangement. Starting Session playback from a running Arrangement requires recompiling the graph.

A scene enters the callback through a dedicated 16-item ring. One item contains all lane actions for the same grid boundary. On saturation the producer rejects the complete request before changing any track. The render graph owns loop phases, MIDI events and clip gate envelopes. Note-offs are sent to both internal sources and the live instrument rack. Audio and MIDI repeat against absolute musical ticks across tempo changes.

## Integration surface

| Area | Changed files under the crate source directories | Contract |
| --- | --- | --- |
| Core | `daw-core/src/{session,model,lib}.rs` | GUI load and preflight call `Project::prepare_session()`. Only missing layouts migrate. `Track.clips` remains Arrangement content. |
| Engine | `daw-engine/src/{session,engine,graph,lib}.rs`, `daw-engine/tests/rt_no_alloc.rs` | `Controller::launch_session(SessionLaunch)` queues a complete plan. Lane positions refer to the currently compiled track order. Optional Session voices attach to graph nodes. |
| UI | `daw-ui/src/{main,studio,studio_session}.rs` | Preflight checks both surfaces and decodes distinct assets. Studio compiles the correct graph, resolves visible selections, tracks the playing view and reads voice meters. |
| Shared editors | `daw-ui/src/{studio_audio_editor,studio_clip_management,studio_history,studio_piano,studio_track_management,studio_views}.rs` | ID lookups include owned cells, Arrangement-only operations remain scoped, and the old GUI scheduling state is removed. |
| Documentation | `docs/SESSION_ENGINE.md`, this report, screenshot | Schema, launch semantics and acceptance evidence. |

The branch adds no dependency, worker protocol field, plugin format change or license. Active voices and pending launches remain transient.

## Verification

All commands below ran from the assigned worktree with a dedicated target directory.

| Check | Observed |
| --- | --- |
| `cargo fmt --all -- --check` | Passed |
| `cargo test --workspace` | **122 passed**, zero failures. Core 24, engine 39, real-time allocation suite 6, plugin-host 3, UI 50. |
| `cargo clippy --workspace --all-targets -- -D warnings` | Passed |
| `cargo build --release -p daw-ui -p daw-plugin-host` | Passed |
| `git diff --check` and staged whitespace check | Passed |
| Release UI `--check-project` on the 17 November review project | 4 tracks, 8 Arrangement clips, 8 migrated Session slots, 2 decoded WAVs. |
| Release UI `--check-project` on the GUI-saved copy | 4 tracks, 8 Arrangement clips, 9 Session slots, 2 decoded WAVs. |

The compiled GUI opened a disposable copy of the review project. Automated window-targeted clicks added a ninth scene and created a fourth-row MIDI cell while leaving row three empty. Save produced nine scene IDs and nine occupied cells. All eight Arrangement clip payloads matched after applying the existing audio serde defaults. A new GUI process reopened that saved file. The first track still had an empty third row and populated fourth row. [Verified window capture, 1540 × 940](../screenshots/session-wave2-reopened.png)

The render test filled the 16-item Session ring and verified whole-scene rejection. The first two tracks activated at frame 6,000. Cancellation retained both voices. A subsequent two-track switch submitted one frame before its boundary activated at frame 18,000 on both lanes. MIDI continued across repeated callbacks without any GUI scheduling. The final stop produced silence. The render calls passed the allocation guard. The audio fixture verified 127 successive loop restarts at the expected absolute frames, including a tempo change and a source that ends before the loop. The grid test checked one frame before, exactly on and one frame after boundaries at 44.1 and 48 kHz, across three bar lengths and fractional BPM. These are rendered frame measurements.

## Remaining acceptance and handoff

Physical output, perceptible switch clicks, stressed external plugins and device XRUN rates still need the owner’s listening pass. Edits that rebuild the graph can interrupt playback. Session audio reanchors decoded samples without time stretching. A request received by the callback after a grid line waits for the next one. A saturated Session queue reports an error and requires a fresh launch.

Integrator can merge the local `beta/session` branch from the Wave 2 baseline and run the shared gate again after the UI merge. Pay particular attention to graph compilation and editor selection in `studio.rs`, and preserve the narrow Arrangement-only split guard in `studio_views.rs`. Update the integration-level handoff after merging. For hardware acceptance, launch two populated scenes around a bar boundary, switch to a row with empty cells, then cancel a queued scene while holding an external instrument note. Check audible continuity, note release and group mixer behavior. The branch includes only source, tests, documentation and a capture of the synthetic review project.
