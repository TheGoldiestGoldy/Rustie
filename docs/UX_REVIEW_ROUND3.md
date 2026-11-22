# Rustie UX Review Round 3: video-confirmed layout drift and mixer

**Date:** 2026-11-18. Close any old Rustie window, then start **Rustie UX Review** or run `~/Projects/rust-daw/scripts/start-ux-review.sh`. The existing reviewer project and original audio loops are preserved; no test fixture has overwritten your edits.

## What the reviewer video established

The 23.4-second screencast `Screencast From 2026-11-18 00-07-00.mp4` shows the window's LEFT LIBRARY panel expanding while the arrangement, mixer and central editor shift right together. This continues with transport stopped. The former diagnosis (egui content drag-to-scroll) did not match the symptom. The Library used a content-measured resizable `SidePanel`, and its nested scroll/layout content could enlarge the persisted panel width. The fix now controls that width explicitly; the `‹` and `›` buttons beside LIBRARY let the user narrow/widen deliberately. These manual changes are expected; spontaneous growth is not.

## Review checklist

| ID | Action | Required behavior |
| --- | --- | --- |
| R3-01 | Do nothing for 30-60 seconds with transport stopped, then start and stop playback and drag notes/clips | The Library/arrangement boundary should remain in place unless the Library resize controls are intentionally clicked. No rightward creep |
| R3-02 | Click Library `‹` or `›` then continue working | Only the requested one-step size change occurs, no automatic increase afterwards |
| R3-03 | In Arrangement, right-click *within the clip with MIDI notes* and in the empty lane between clips, NOT the track control header | The same context menu for that particular track appears: Rename, Add MIDI/audio, Group, Move to group, Remove etc. Left-click/drag still selects/moves clips |
| R3-04 | In Session, right-click a clip slot and blank slot area of an existing track | Track-specific right-click menu opens; clip Launch button still works |
| R3-05 | Open Mixer (lower editor tab), widen it vertically if useful and change a channel's fader and pan | Distinct upright channel strips with a tall volume fader, separate M/S and pan; no overlapping labels; audible mixer operations should still work. Master meter shows actual output peak |
| R3-06 | Group tracks and switch Arrangement/Session during playback, then Save/Reopen | Group routing and clip playback continue correctly; names/groups persist; no visual creep |

## Honest boundaries

- The new mixer provides **vertical volume faders**, horizontal pan controls, group buses, M/S and a real master peak meter. Individual per-track meters are **not available** because the engine does not yet provide that telemetry. No decorative fake meters are shown.
- The library no longer accepts freeform edge dragging; width is adjusted with the `‹` / `›` header controls. This avoids the observed automatic growth and preserves an explicit resize affordance.
- The sidebar regression test checks 120 redraws without user input and the lane-context test simulates a right-click away from the header. Manual testing of the user's exact Wayland desktop remains the final acceptance step.
- User project files and media were not overwritten. No plugin hosting, recording, finished-song export or GUI undo was introduced in this pass.

## Share results

Reply with pass/fail for R3-01 through R3-06. If drift remains, mention whether the LIBRARY edge moves as in the original video, or the timeline scrolls while the Library stays fixed. The distinction now narrows the remaining source dramatically.
