# Rustie UX Review: Round 2 (track management and mixer)

**Build date:** 2026-11-17. **What changed:** Session/Arrangement right-click track menus; actual audio-routing group buses; bottom-editor Mixer tab; editing-canvas drag-panning disabled; deletion confirmation.

Start a fresh application process using **Rustie UX Review** from the desktop launcher or `./scripts/start-ux-review.sh`. **Close the previous application window first.** The review launcher opens your same saved project and never overwrites reviewer edits. If you want a separate copy for destructive testing, use File > Save as… first.

## Focused retest

| ID | Do this | What should happen |
| --- | --- | --- |
| R2-01 | Select Arrangement, move/resize notes and drag clips around as before, while playback runs. | The canvas must not spontaneously pan horizontally. Manual horizontal scrollbar/Shift+wheel scrolling must still work. Note if the *window itself* moves versus just the content. |
| R2-02 | Click Session (Tab). Look at the lower editor for **Mixer**, or click the dedicated **Mixer** button in Session's toolbar. | A separate mixer appears with actual VOL, PAN, M, S for each track and master peak telemetry. When no audio is playing the master peak is zero; no fake per-track meters. |
| R2-03 | Right-click any track name in Arrangement or Session, choose Rename; enter a meaningful name and save. Then save project and reopen. | Both views and the saved JSON reflect the new name. |
| R2-04 | Right-click the Chords track and choose **Group this track into new bus**. Open Mixer, move the bus volume and pan, mute/solo the bus; add the Bass track using **Move to group**. | A real bus is inserted before grouped tracks, audio is routed through it, and group mute/gain/pan affect grouped audio without changing other tracks. Session scene launching should still work. |
| R2-05 | Right-click the group header, choose **＋ MIDI track in this group** and **＋ Audio track in this group**. Use the audio track right-click **Import audio clip** action. | New tracks appear and route into the group. Empty audio tracks can exist before importing. |
| R2-06 | Try **Remove from group** then **Delete group (keep its tracks)**. Save/reopen. | Group disappears after a confirmation, but child tracks and their clips remain. Deleting an ordinary track shows a warning because undo is not wired into the UI. |

## Important boundaries

- Group buses are actual audio graph mixer nodes, with group volume, pan and mute. Group nesting, group effect racks, sends, automation, and group collapse/expand are not implemented yet.
- Track groups persist in JSON. The old project schema remains backward-compatible via optional `group_id`.
- Right-click offers creating MIDI tracks or empty audio tracks, importing clips, rename, grouping, ungrouping and deletion with confirmation. There is no multi-selection/group-many-at-once workflow yet. Move tracks to the group one by one.
- The horizontal drift mitigation disables drag-to-pan over the content while retaining scrollbar and wheel navigation. It has headless regression coverage, but whether it fixes the user's intermittent symptom remains to be established through actual mouse use.
- No new plugin host, microphone/MIDI recording, export, undo or crash recovery has been introduced in this pass.

## Report a regression

Give the **R2 test ID**, action, actual result and frequency (always/sometimes/once). For sideways movement, state whether the full app window shifts or only the timeline/piano content shifts. If you can, note whether a horizontal scroll gesture, dragging a MIDI note, or playback triggers it. The user-reported audio and piano flows from Round 1 are the baseline and must not regress.
