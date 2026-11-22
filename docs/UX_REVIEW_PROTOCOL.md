# Rustie / Slate Studio UX Review Protocol

**Build:** 17 November 2026 developer review build. **Goal:** evaluate discoverability, workflow, responsiveness, correctness and user trust, not pretend that unfinished production features exist.

## Launch and safety

Open **Rustie UX Review** from the GNOME application launcher, or run:

```bash
~/Projects/rust-daw/scripts/start-ux-review.sh
```

The launcher generates a new, synthetic test song only on first run at `~/Music/Rustie UX Review/Rustie UX Review.slate.json`. Every subsequent launch opens the same editable review song. It **does not overwrite** prior project edits or WAVs. The two WAV loops are original synthetic test audio, not music samples from user projects. Use `Ctrl+S` to save, and `File > Save as…` to create snapshots. Rename or back up the review JSON before regenerating a clean test. Do not delete real music folders. The normal Slate Studio launcher remains unchanged and still opens its three-track starter demo.

**Startup prerequisites:** GUI must open with title *Slate Studio*, project *Rustie UX Review | Safe Test Song*, four tracks, two distinct clips per track, and the left browser. For missing sound note selected audio backend/output path. Do not change system-wide audio profiles or disturb active calls to perform this test.

## Test script (20-30 minutes)

| ID | Exact task | What to observe / expected behavior |
| --- | --- | --- |
| UX-01 | Launch from application menu, inspect screen before clicking | Does a new reviewer understand what is playing, track/clip structure, editor tabs, Play and Save? Are labels legible? |
| UX-02 | Press Play, Stop, Home/return-to-start, Space; adjust BPM | Sound and playhead should be synchronized. Record any perceptible delay, missing first notes, clipping or freeze, not merely whether the master meter moves. |
| UX-03 | Select the first Melody clip. Drag the **top edge of the bottom piano editor upward**, release, shrink again | Piano editor height should stay wherever dragged, without rubber-banding; surrounding timeline remains navigable. Previously reported regression. |
| UX-04 | Select the Chords clip; scroll piano vertically; toggle Fold, Highlight and In key; change root/mode | Existing chord notes should be visible at usable zoom; fold filters without deleting notes. Keyboard spans full MIDI note range. Does the scale chooser make sense? |
| UX-05 | Double-click/click empty piano grid to create a note; drag note body, drag its right edge, duplicate, delete, adjust selected velocity | Edited notes should appear at the intended pitch and musical time, snap predictably and change actual playback after restart. Undo/redo is intentionally not implemented. |
| UX-06 | On Arrangement, zoom, scroll horizontally, drag a clip and use track volume, pan, mute and solo | Explore whether expected actions are easy to find and use without fighting the UI. Listen for audible mixer effect. |
| UX-07 | Switch to Session (Tab); play scene 1 then trigger scene 2 while playback is running; launch an individual clip and stop one track | Live launch should queue on a future bar, not restart the whole transport. Watch for gaps, timing instability or incorrect scene progress. Session slots currently share Arrangement clips. |
| UX-08 | Explore the browser's Instruments, Plugins, Samples and All sections; search a plugin name | Installed plugins may be **listed but disabled**. They do not load yet; report only confusing labelling or broken browser navigation, not the known missing host. |
| UX-09 | Change MIDI note, track volume or clip location; Ctrl+S; quit and reopen Rustie UX Review | Project should reload the saved MIDI and mixer/clip edits. Real sample files remain on disk beside JSON. Note any unsaved-work warnings missing or changes lost. |
| UX-10 | Make the window narrower; use shortcuts, switching tabs, scrolling, selecting and editing at compact size | Report clipped controls, poor hit targets, keyboard confusion, inaccessible content, stuck scrolling and visual hierarchy problems. |

## Do not attempt as acceptance tests yet

- Third-party CLAP/VST3/LV2 plugin loading or GUI editing: **not implemented**.
- Microphone/instrument audio recording and MIDI keyboard input: **not implemented**.
- Bounce/export of finished WAV/MP3: **not implemented**.
- Robust Ctrl+Z undo, crash recovery, comping, warping, sophisticated effects and automation: **not implemented**.

These are roadmap gaps; distinguish them from UI regressions.

## Reporting feedback

Use the adjacent `UX_REVIEW_FEEDBACK.md` in your Music review folder, or send observations in the chat in ordinary language. For each issue include **test ID, exact action, expected result, actual result, severity**, and whether it repeats. If relevant mention the selected audio backend and whether sound actually reached headphones. Screenshots/videos and exact labels help, but are optional.

Severity convention: **P0** data loss or crash, **P1** blocks the tested workflow, **P2** confusing/intermittent friction, **P3** cosmetics/polish. Report what you enjoyed, too. Positive results help protect good workflows from accidental redesign.
