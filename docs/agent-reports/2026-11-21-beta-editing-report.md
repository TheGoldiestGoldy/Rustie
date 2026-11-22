# Rustie Agent 03 · Arrangement editing · 21 November 2026

## 1. Implementation

The original Wave 1 editing commit, `8997222`, already shipped single-clip trim and split, piano velocity editing and bounded history. Its report identified batch Arrangement editing as unfinished. This follow-up adds it to the dedicated `beta/editing` worktree.

| Area | Files | Behavior |
| --- | --- | --- |
| Selection and movement | `crates/daw-ui/src/studio_views.rs`, `studio.rs`, `studio_clip_management.rs` | Shift-click toggles clips across audio and MIDI tracks. Dragging a selected clip body moves the selection by one snapped delta and clamps it at time zero while preserving relative spacing. Click an empty lane to clear the selection and seek. Trim works on the clip whose edge is dragged. |
| Batch actions | `crates/daw-ui/src/studio_clip_management.rs` | Copy/cut/paste, duplicate, split and delete operate on the selection through the Edit menu or shortcuts. Multi-clip paste retains the original tracks and relative spacing. Single-clip paste can use a compatible track. Duplicate places the copy after the selection's span. Split processes clips intersecting the playhead, with one musical split tick and accurate source offsets across tempo changes. Capacity and paste destinations are checked before inserting copies. |
| History | `crates/daw-ui/src/studio_history.rs` | Each batch is one undo/redo edit. History restores the selected IDs and primary clip. The history observer avoids cloning the whole project on unchanged frames. |
| Controls and documentation | `crates/daw-ui/src/studio_views.rs`, `docs/FEATURE_MATRIX.md` | Added Shift-click help, selection-aware menu labels, and paste routing guidance. Each clip's context menu operates on that clip. |

Arrangement keyboard map

| Action | Shortcut |
| --- | --- |
| Toggle a clip in the selection | Shift-click |
| Cut, copy, paste at playhead | Ctrl+X, Ctrl+C, Ctrl+V |
| Duplicate | Ctrl+D |
| Split intersecting selected clips | S |
| Delete | Delete |
| Undo and redo | Ctrl+Z and Ctrl+Y or Ctrl+Shift+Z |

## 2. Native application evidence

[Arrangement and piano roll](../screenshots/beta-editing-arrangement.png) shows the release GUI at 1540×940 with synthetic MIDI clips, decoded WAV waveforms and the updated Arrangement hint. The Session view also opened with all four tracks and both populated scenes. These were window-only XWayland captures after opening the synthetic project. Pointer behavior was exercised separately with egui input events.

## 3. Verification

Cargo used `CARGO_TARGET_DIR=/tmp/rustie-target-editing` and `CARGO_BUILD_JOBS=6`.

| Command or procedure | Result |
| --- | --- |
| `cargo fmt --all -- --check` | Passed |
| `cargo test --workspace --quiet` | 108 passed, including 47 UI tests |
| `cargo clippy --workspace --all-targets -- -D warnings` | Passed |
| `cargo build --release -p daw-ui -p daw-plugin-host` | Passed |
| `git diff --check` | Passed |
| `python scripts/generate-ux-review.py /tmp/rustie-editing-evidence.SHva0H` followed by the release `daw-ui --check-project` for the generated project | Valid project with 4 tracks, 8 clips and 2 decoded audio files |
| Release `daw-ui --project` with and without `--session` | Both native views opened. Captured the application window by PID with `xprop` and `import -window`. |
| `arrangement_shift_click_and_drag_outside_moves_selection_without_seeking` | The actual Arrangement widget selected two clips, dragged them together, captured the release outside the panel, and kept the playhead fixed. Clicking a subsequent empty lane sought and cleared selection. |
| `batch_edits_preserve_mixed_tracks_offsets_and_survive_save_reopen` | Grouped audio/MIDI duplication, paste spacing, tempo-aware splitting, batch undo/redo, and 4/4 and 6/8 bar lengths passed. Saved and reopened through the Studio project path. The WAV decoded and matched its original bytes. |
| `batch_capacity_failure_preserves_project_selection_and_next_id` | Rejected an oversized batch with the project, selection and ID allocator intact. |
| Existing split and editor regressions | Sample continuity passed at 44.1 and 48 kHz. The workspace run included the Library-width, piano velocity, and history regressions. |

## 4. Limits and human acceptance

- The native windows were inspected at rest. Sustained playback while editing, physical audio output, and manual mouse and keyboard acceptance remain unverified.
- Live editing still uses the existing graph restart path. A batch calls it once. An audible interruption is possible.
- A saved audio source offset still uses decoded PCM frames. Compatibility across a change in the negotiated session rate requires separate validation.
- Multi-clip paste requires every original track to be present. Opening another project clears a multi-clip clipboard. Edge trim operates on one clip.

## 5. Next verification

After integration, open the synthetic 4-track project, Shift-click a MIDI clip and a WAV clip, then drag, split, duplicate and undo while the transport is running. Listen at the split and during graph restart. Switch into Session and verify that copying, pasting, duplicating and removing slots still target the independent slot model. Repeat the Arrangement edit through save and cold reopen.

## 6. Git and artifacts

- Worktree `/home/developer/Projects/rust-daw-agent-editing`, branch `beta/editing`.
- Coordinator baseline `1ae60bb2106b595e7e46c33911753ee45eb6d6f6`. This commit follows `8997222`, which is already an ancestor of the integration branch.
- Local commit includes the four UI source files above, feature matrix, this report and the Arrangement capture. Synthetic projects, WAVs and the Session capture were kept in the review directory under `/tmp`.
- Runtime editor state only. Project JSON, plugin IPC, dependencies and licensing retain the existing formats. Copied and split objects use newly allocated clip IDs. The capture was inspected and shows the synthetic review project.

## 7. Integration handoff

The newer `beta/integration` tip inspected was `1b717f9`. A read-only `git apply --check` confirmed source overlap. Transfer this follow-up with conflict resolution on the integrator's branch.

- In `studio.rs`, wire the in-memory selection, typed clipboard, drag origin, shortcut dispatch and load cleanup into the current app. Maintain the existing device, Session and recording lifecycle fields and callbacks.
- In `studio_views.rs`, carry over the menu item text, Arrangement hover help, primary-button capture and deferred click/drag/drop handling. Keep the newer transport layout.
- In `studio_clip_management.rs`, keep the integration branch's Session-specific lookup, insertion and deletion paths. Apply batch mutations to Arrangement clips, with the newer application's ID allocator and playback update hooks. The deletion cleanup in this Wave 1 branch predates independent Session slots.
- In `studio_history.rs`, keep current project/Session restoration and add the Arrangement ID list to its snapshot and validation. Run the focused regressions and shared gate on the integrated build.
