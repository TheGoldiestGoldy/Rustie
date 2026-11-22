# User feedback: Rustie UX Review Round 1

Date: 2026-11-17. Notes supplied by the reviewer directly, **not** independently hardware-reproduced by the engineering agent.

| Original user test | Reported result |
| --- | --- |
| 1: Transport/playback | Passed with flying colors |
| 2: Piano resizing/editing | Passed smoothly and liked |
| 3: Session tab | Works flawlessly |
| 4: Mixer | Individual channel volume and pan work, but a separate recognizable mixer was not found in Session |

## Reported issues

1. Entire software or viewport appears to creep rightwards intermittently during work, perhaps related to note-following/scroll feedback. Engineering observation: all three egui editing canvases used `ScrollSource::ALL`, which permits background drag-to-scroll; this is a **plausible root cause, not a proven reproduction**. Round 2 disables drag-to-scroll but allows explicit scrollbar/wheel scrolling. Ask whether app window or canvas shifts if it persists.
2. Cannot discover right-click actions to add, group, rename or manage tracks. Round 2 adds shared track context menus to Arrangement, Session and Mixer plus actual group return-bus routing, an empty audio-track option, and delete confirmation.
3. No separate mixer visible in Session. Round 2 adds a dedicated lower Mixer tab, auto-shown on entry to Session with an explicit Mixer button.

## Regression policy

Do not modify playback, tempo, piano move/resize semantics or quantized Session launching without reproducing a concrete defect. New code should preserve the four reported successes.
