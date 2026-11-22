> Historical design/implementation notes. For current capabilities and gates, see [project status](PROJECT_STATUS.md) and [architecture](ARCHITECTURE.md).

# Next engineering milestones, prioritized

## P0: trustworthy playback/interaction
1. Reproduce the reported multi-note gap between visual playhead and audible feedback with backend-specific timestamp telemetry. Fix based on measured device buffered latency; do not solve by simply reducing visuals.
2. Add stable persistent SessionSlot models and project schema migration for old JSON files.
3. Enable sample-accurate quantized clip launch without stopping/restarting the entire audio driver; crossfade audio loops to remove clicks.
4. Implement complete note-focused undo/redo and multi-note selection with keyboard commands.

## P1: real production workflow
5. A true plugin host with isolated scanning and process lifecycle, start with CLAP and then licensed VST3; parameter/meter/latency/state serialization.
6. Browser tree/favorites and draggable audio/plug-in entries; filesystem watcher and persisted user sample locations.
7. Waveform zoom, region trimming, fade handles, non-destructive sample editor, rulers/loop brace, track reordering.
8. Velocity lane, MIDI multi-selection, lasso, legato and note quantize commands.
9. Metronome, time-signature UI, punch in/out, recording/arm/input monitoring, hardware MIDI.

## P2: finished art direction
10. True compositor-supported translucent glass/blur and measured performance of shader passes on low-end GPUs.
11. Custom Slate Studio icon set and type scale from redistributable fonts, responsive compact controls, HiDPI evaluation.
12. Keyboard-only and screen-reader access; end-to-end UI automation and screenshot regression tests.

## Testing
- Add tests that simulate two clip changes within an active session, audio zero crossings, MIDI note overlap, queue saturation, project save/load migration.
- Reproduce on ALSA, PipeWire/JACK, PulseAudio, and at 44.1/48/96 kHz before marketing low latency.
- Measure x-runs, output latency, visual frame cadence, memory growth after hours of looping.
