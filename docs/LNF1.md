# LNF-1 native piano device

Status: foundational implementation (23 November 2026). The LNF-1 is an original Rusty device inspired by the attached product brief's interaction model, not a port of Lost N' Found Piano.

## What is implemented

- A native, selectable LNF-1 source for any MIDI track. The persisted instrument marker is `bank = "lnf-1"`; it never invokes SoundFont discovery. All 20 parameter values are stored with that assignment and old projects remain compatible.
- An original 32-voice acoustic/electric, piano-style oscillator engine with quietest-voice stealing, CC1 modulation, CC64 sustain, all-notes-off, deterministic detune, stretch, envelope, muffle, drive, stereo width, simple allpass ambience and smoothed output gain.
- The documented 20 stable parameter IDs, defaults, ranges and five-page encoder mapping. The `tone` page changes between acoustic muffle controls and electric modulation/rate.
- A Devices → LNF-1 panel: acoustic/electric selection, pages 1–5, black OLED-like page graphics, colour-coded drag encoders (Shift = fine, double-click = reset), and a visible two-octave flat key grid for auditioning.
- Audio-facing state is constructed before the render callback. `Lnf1Instrument::render` is allocation-free by construction: voices and ambience buffers are fixed-size fields. The UI publishes values through `Lnf1ParamStore` atomics; the render thread snapshots them without locks, I/O, allocation, or graph rebuilds.

## Deliberate boundary

The module does **not** ship Lost N' Found Piano, mda Piano, mda EPiano, or their samples. Those assets/code were not imported because the source plug-in described in the brief is GPL-3.0 and no separately reviewed sample provenance was supplied. This means this implementation is not a sound-parity port.

The device settings are saved per track and restored after a cold reopen. Parameter changes update an active LNF-1 source immediately through atomics. Rusty's general host automation/gesture system and dedicated UI telemetry are not yet connected to the device, so controls should not be described as DAW automation-ready.

## Architecture

```
Devices → LNF-1 panel → project `lnf1_params` + atomic Lnf1ParamStore
                                                    ↓
MIDI / on-screen keyboard ─────────────────────► Lnf1Instrument (audio thread)
                                          ↓
                                 stereo output + ambience
```

`crates/daw-engine/src/lnf1.rs` contains the engine and stable parameter contract. `crates/daw-ui/src/studio_lnf1.rs` draws and handles the device panel. `studio.rs` recognizes the `lnf-1` marker during graph construction and creates the native source.

## Next implementation milestone

1. Add parameter-edit undo/redo plus explicit automation gestures and sample-accurate smoothing for every continuous control.
2. Add telemetry for voice dots, level meters and LFO phase, then test the panel with native pointer interaction and screenshots.
3. Only after licensing review, decide whether to build a separately licensed sample-data pipeline; validate sample-rate and sound parity independently.

[executed on device: Johnnysstation (1200896c-45a0-4cb2-8dfa-479e25d803a4)]