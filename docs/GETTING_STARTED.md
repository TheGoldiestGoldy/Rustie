# Slate Studio • Desktop prototype

## Starting the app

Open GNOME Activities and search for **Slate Studio (Native DAW)**. Or launch it with:

```bash
~/Projects/rust-daw/scripts/start-slate-studio.sh
```

The launcher uses the optimized release binary if present, or the debug binary.

## Playing music

The first launch includes eight notes of an original demo melody on the built-in sine synthesizer. Press **Play** or **Space** to hear it, and **Stop** or **Space** to rewind.

On the initial Linux test workstation, **ALSA · Low latency** is now the preferred default, with fixed 128-frame requests. PulseAudio can also be selected, but its buffering performed poorly in timing tests. The AIR 192|6 is configured for Analog Stereo Duplex, and its analog output is the PulseAudio default. The separate PipeWire/JACK graph has no connected physical speaker ports.

## Audio files

Use **＋ Import audio**, choose an indexed sample in the left browser, or drop a WAV/FLAC/MP3/OGG file onto the window. Imports create audio tracks with blue waveform previews. Track headers integrated into the arrangement support live volume, pan, mute, solo and deletion through the track context menu. Clips can be moved on a snap grid. Click a clip to inspect it, or click empty timeline space to seek.

Files are decoded off the audio callback to in-memory stereo buffers. This is not streaming and can consume substantial RAM on long recordings. Sample-rate conversion is currently basic linear resampling.

## Recording a microphone

1. Select an Audio track. Open its input controls and choose the PulseAudio interface and physical input. On the AIR 192|6, **Input 1 (Mono)** and **Input 2 (Mono)** record one channel at its incoming level. **Stereo 1–2** preserves the pair.
2. Close Devices and speak into the mic. The selected track’s input meter shows signal before recording. Check the interface gain if the input is ready and the meter stays low. Enable software monitoring separately if you want to hear the live mic through Rusty.
3. Press **REC**. Follow the elapsed capture time and growing waveform on the recording track. **Stop** or **Pause** finalizes the WAV and places it in Arrangement.
4. To check the take by itself, solo the track, center its pan, return to the clip, and press **Play**. A mono take plays in both channels at centered pan. Save the project, and use **File → Export Arrangement to WAV** for a stereo mixdown.

## MIDI and scale-aware piano roll

Select **＋ MIDI** to add a MIDI clip, or select the demo track. In the bottom **Clip editor**, click empty grid space to draw a note, drag notes to move them, drag their right edges to resize, or right-click/Delete to remove them. Choose root and scale mode; **Highlight** colors in-key notes, **Fold** hides out-of-scale rows, and **In key** prevents drawing outside the scale. Adjust note length, snap grid (1/4 to 1/32), duplicate a note or edit velocity. Press Play to hear the MIDI instrument. There is no external MIDI keyboard input yet.

## Save / Open

**Save** and **Save As** create JSON project documents, containing tracks, clip paths, note events, BPM, level/pan, and scales. Source audio is referenced by its existing file path, not embedded in the project; keep source files in place. **Open** loads these JSON projects.

The current project format now persists track mute and solo settings but does not bundle sample caches. It is not an Ableton .als file.

## Shortcuts

- Space: Play/Stop (unless editing text)
- Tab: Arrangement/Session overview (unless editing text)
- Shift+Tab: Clip/Device detail (unless editing text)
- Ctrl+F: Browser search
- Ctrl+S: Save
- Ctrl+O: Open

## Session / Live grid
Switch to **Session** with Tab or the view button. Click a clip's ▶ to launch it, click **▶ Scene** to play a row, or use track **■** and **Stop all** to stop playback. Click an empty slot immediately after the last clip in a track to create a new MIDI clip or import an audio file. Active clips show a progress line. **Current limitation:** live clip/scene changes now queue to a future bar with 128-frame gain ramps, but have not been qualified for glitch-free gigs. Session slots still share the same clips as Arrangement; changing views restarts playback.

## Browser and plugins
The Library indexes samples, local SoundFont banks and discovered plug-in bundles. Click a sample to import it, or choose a General MIDI bank and preset in Devices → Instrument. Use Devices → Plug-ins to inspect and assign a supported CLAP or VST3 class. Arrangement export requires bypassing enabled live inserts and importing printed audio when the mix needs those parts. See the [feature matrix](FEATURE_MATRIX.md) and [Beta 1 decision](BETA1_RELEASE_DECISION.md) for tested combinations and current limits.

## Smoke test

From a terminal, run:

```bash
~/Projects/rust-daw/scripts/start-slate-studio.sh --smoke-audio
```

This produces a short low-volume A4 MIDI note through the native backend and reports a nonzero output peak. The result does not guarantee audible external routing, round-trip latency or glitch-free playback.


## UX reviewer build

Run `./scripts/start-ux-review.sh` or use the **Rustie UX Review** GNOME app entry. The launcher creates (only when absent) a reproducible 4-track, two-scene test project in `~/Music/Rustie UX Review`, checks its JSON/PCM decode, and loads the saved project directly with `--project`. Follow `docs/UX_REVIEW_PROTOCOL.md` for ten tasks and use `UX_REVIEW_FEEDBACK.md` inside the review folder to collect findings. This does not add missing recording/plugin/export functions.


## Track management / mixer: UX Round 2

Right-click a track name in Arrangement or Session. Create a new MIDI or empty audio track, rename a track, group it into a new bus, move another track into that bus, or remove a group without deleting children. A right-click on an audio track offers importing a clip into that track. Deleting a track now requires confirmation because graphical undo is not implemented. The **Mixer** lower-editor tab offers VOL, PAN, M and S for individual and group channels, with a real master peak meter, and appears automatically when switching to Session. Use `docs/UX_REVIEW_ROUND2.md` to assess scrolling/drift and mixing.


## UX review Round 3

The new Mixer is a vertical-fader console in the lower editor; track context menus now work by right-clicking clips and blank track lanes, not just track names. If the Library appears to move sideways, check whether the pane boundary changes: Library width is deliberately controlled by ‹/› beside LIBRARY to avoid the self-expanding panel observed in a user screencast. See `docs/UX_REVIEW_ROUND3.md`.
