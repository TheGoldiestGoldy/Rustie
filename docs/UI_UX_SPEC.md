# Slate Studio: product UI/UX specification

**Purpose:** Original, practical DAW with familiar workflow conventions, not a pixel-for-pixel clone of any product. Product identity is **Slate Studio** (working name). This spec is aspirational unless a capability is listed as implemented in [PROJECT_STATUS](PROJECT_STATUS.md).

## Design tokens and philosophy

- Near-black base and slightly lighter differentiated surfaces. Current prototype uses RGB values such as (10,13,18) for panels and (5,7,11) for deepest background; preserve legible contrast.
- "Liquid glass" as a restrained impression: subtly translucent floating panels, crisp border highlights, muted gradients, softly rounded surfaces. **Never trade editability or metering clarity for blur**.
- Density comparable to a pro DAW at 1080p; keyboard-first power workflows, but discoverable mouse targets.
- Use original symbols and controls. Do not copy proprietary skins, artwork, Ableton marks or paid icon sets.
- A unified spacing/typography/token system; scale and high-DPI support. Error and disabled states must be visually explicit.
- Avoid permanent animations in real-time views, especially CPU-heavy blur. Monitor egui paint and update latency.

## Shell layout

```text
┌──────────────────────────── Top: menu, transport, BPM, time, CPU/output ──────────────────────┐
│ Left browser      │ Main editor: Arrangement  OR  Session grid                            │
│ [Search]          │ track controls | ruler/clip slots | scrollbar/zoom                   │
│ All/Instruments   │ ──────────────────────────────────────────────────────────────────    │
│ FX/Plugins        │                                                                      │
│ Samples/Places    │                                                                      │
│                  ├──────────────────────────────────────────────────────────────────────┤
│                  │ Bottom details: [Clip] [Devices] [Mixer?]                            │
│                  │ scale, MIDI editor / waveform or device chain                         │
└──────────────────┴──────────────────────────────────────────────────────────────────────┘
```

The dock panels must be resizable, remember user intent when closed/reopened and never obscure essential transport controls.

## Left browser: acceptance

- Sections: All, Instruments, Effects, Plugins, Samples, Places (optionally Packs/Presets later).
- Search globally and within section. Display count, file type and installed/disabled state.
- User-selected sample locations; bounded indexing with symlink loop protection and filtering. Scanning must run outside frame render and never starve audio.
- Sample click/double-click can preview safely, drag to track imports / places clips in a later phase.
- Plugin listing is **not plugin execution**. Disabled entry and tooltip until a genuine CLAP/VST3/LV2 host exists.
- Avoid scanning user home recursively without explicit consent; no samples or plugin binaries in source control.

## Arrangement: acceptance

- Musical ruler/tempo, zoom/snap and meaningful grid hierarchy (bar / beat / subdivisions).
- Audio waveform previews and MIDI mini-note previews on clips.
- Clip select, drag, trim, duplicate, split, loop, rename and context menu with fully reversible operations.
- Track headers with audio/MIDI type, name, color, volume, pan, mute, solo, eventual record-arm.
- Right actions: click empty ruler seeks; selecting/editing mixer or clip must **not** seek unexpectedly.
- Respect up to project's explicit capacity limits; show error instead of silent truncation.
- UI playhead should be synchronized with the actual scheduled device playback time, not merely processing ahead.
- Enforce consistent loop start/end and ghosted clip drag previews; no audible clicks on edits.

## Session / Live Grid: acceptance

- Each column is a track, each row a scene (or explicitly documented transposition).
- A clip slot has state: empty, stopped, queued-to-launch, playing, queued-to-stop, recording.
- Clip launches must be timed against the musical clock and quantization boundary, not egui frame times.
- A scene launch schedules compatible slots together. Stop buttons are explicit and no "select = play" ambiguity.
- Arrangement/Session ownership and re-entry rules are transparent. A scene must not duplicate active track audio.
- Keyboard/controller focus, horizontal/vertical scrolling and a meaningful way to create/move slots.
- **Baseline:** Session is just a selectable overview. Do not mark the above done without functional launch tests.

## MIDI piano roll: acceptance

- Root selector C through B and 12 named scale modes; individual clip scale can override global scale.
- Highlight in-scale rows/keys and root; **fold** should map every visible row back to the correct MIDI pitch.
- Scale lock optional, with obvious on/off state; never change existing note pitch silently when toggled.
- Grid 1/4, 1/8, 1/16, 1/32, snap settings, note length controls and note velocity.
- Direct create/delete/select/move/resize; keyboard shortcuts, audition toggle, horizontal/vertical zoom.
- Visual hierarchy for note velocity, selected notes, note-off and muted notes.
- Test C major, F# minor, chromatic and all black-key cases, folded and unfolded.

## Device chain and mixer: acceptance

- Reorderable device chain that reflects real compiled DSP and survives save/reload.
- Clear distinction between real devices, built-in sine source and unavailable plugins.
- Volume/pan/mute/solo consistent between Arrangement and Session; stable per-track IDs.
- Master meters (peak and eventually true peak), headphone routing, latency reporting with method.
- Automatable controls eventually stored in project format and sample-accurately scheduled.

## Accessibility and performance

- Minimum 44 px touch target where practical without hurting desktop density; keyboard operability.
- WCAG-reasonable contrast for primary text and on/off state; motion/blur fallback.
- Focus rings, labels, tooltips and non-color cues for record/solo/mute.
- No blocking filesystem scans, expensive decode or plugin scan on paint/audio callback.
- Keep render-thread and UI-thread benchmarks independently measured.

## Acceptance evidence for UI PRs

For each surface, supply screenshots at 1920x1080 plus one smaller width, interaction recording/log of actual clicks, automated checks where possible, dark-theme contrast audit, performance observations, and a list of still-disabled controls. Screenshots alone are **not** proof of functionality.
