# Slate Studio • Desktop prototype

## Starting the app

After building, launch from the repository root with:

```bash
./scripts/start-slate-studio.sh
```

The launcher uses the optimized release binary if present, or the debug binary.

## Playing music

The first launch includes eight notes of an original demo melody on the built-in sine synthesizer. Press **Play** or **Space** to hear it, and **Stop** or **Space** to rewind.

On the originally tested Linux workstation, **ALSA · Low latency** is now the preferred default, with fixed 128-frame requests. PulseAudio can also be selected, but its buffering performed poorly in timing tests. The AIR 192|6 is configured for Analog Stereo Duplex, and its analog output is the PulseAudio default. The separate PipeWire/JACK graph has no connected physical speaker ports.

## Audio files

Use **＋ Import audio**, drop a WAV/FLAC/MP3/OGG file onto the window, or paste an absolute file path. Imports create audio tracks with blue waveform previews. Track headers integrated into the arrangement support live volume, pan, mute, solo and deletion through the track context menu. Clips can be moved on a snap grid. Click a clip to inspect it, or click empty timeline space to seek.

Files are decoded off the audio callback to in-memory stereo buffers. This is not streaming and can consume substantial RAM on long recordings. Sample-rate conversion is currently basic linear resampling.

## MIDI and scale-aware piano roll

Select **＋ MIDI** to add a MIDI clip, or select the demo track. In the bottom **Clip editor**, click to add or remove a sixteenth-note MIDI event. Choose root, scale mode, and **Scale Fold** to hide notes outside the selected scale. Press Play to hear the MIDI instrument. There is no external MIDI keyboard input yet.

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

Session View is an **overview only**. It cannot independently launch clips yet. VST/CLAP plugins, live audio recording, warping, external MIDI ports, detailed automation lanes, and Ableton .als compatibility remain future milestones.

## Smoke test

From a terminal, run:

```bash
./scripts/start-slate-studio.sh --smoke-audio
```

This produces a short low-volume A4 MIDI note through the native backend and reports a nonzero output peak. The result does not guarantee audible external routing, round-trip latency or glitch-free playback.
