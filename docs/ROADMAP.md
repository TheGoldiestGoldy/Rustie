# Roadmap and definition of done

Priorities reflect the product goal and the present codebase, **not promised dates**. Each item is complete only with passing tests and hands-on workflows.

## P0: stabilize usable music-making loop

- [ ] Eliminate/measure audible onset lag relative to UI playhead: capture callback timestamps, device predicted delay, actual loopback timing where possible. A 128-frame buffer setting alone is not evidence.
- [ ] Verify an eight-note demo plays all eight notes at the expected times on user hardware. Test cold start, repeat play/stop, seek and backend switching.
- [ ] Merge and validate searchable, resizable left browser (samples/instruments/FX/plugins/places); keep unloaded plugins disabled.
- [ ] Make the piano roll actually easy to edit: add/delete, move, resize, select, zoom, velocity, 1/4..1/32 grid, scale root/mode/highlight/fold.
- [ ] Keep Arrangement clip selection and dragging separate from seek ruler; verify mixer controls.
- [ ] Save/reopen a multi-track JSON project without silently dropping edits or mixer state.
- [ ] CI: fmt, Clippy, tests; zero exposed personal data or binary assets.
- [ ] Fix hard-coded absolute path in launcher; document supported Linux libraries.

**Exit:** A musician can launch, make a two-track MIDI/audio sketch, edit it, hear it promptly, save, close, reopen and continue with no unexplained loss.

## P1: actual Session workflow

- [ ] Shared track / device / clip model between Arrangement and Session.
- [ ] Per-slot play/stop control, scene play, active/queued indicators.
- [ ] Quantization: immediate, 1/4, 1 bar, 2 bars (specify clock semantics and tests).
- [ ] Session vs Arrangement arbitration rules, with visible takeover indicator.
- [ ] Global loop region and transport loop controls.
- [ ] Clip duplication, copy/paste, undo/redo integrated into main editor.
- [ ] Better controller/focus, keyboard shortcuts and right-click menus.

**Exit:** Session clips can start/stop musically in time, scenes act predictably and existing Arrangement playback is not corrupted.

## P2: recording and sound quality

- [ ] Microphone/interface input via native driver; record-arm, monitoring, metronome and overdub rules.
- [ ] MIDI keyboard input and basic clock/device management.
- [ ] True latency and xrun measurement; settings UI with device/buffer/sample-rate report.
- [ ] Streaming long samples from disk, higher-quality resampling, file relink and portable project packaging.
- [ ] Automation lanes, editable envelopes and gain smoothing, plugin-ready graph lifetime.
- [ ] Common editing tools: trim/fade, waveform zoom, loop, split, multi-select, groups, meters, export WAV.

## P3: plugin hosting and isolation

- [ ] CLAP research/proof of concept with isolated worker, explicit shared-memory IPC protocol.
- [ ] Worker crash recovery and host fallback with bounded audio impact.
- [ ] Plugin catalog with plugin name/vendor/format and user-controlled scanning; versioned cache.
- [ ] Plug-in parameter automation, preset/state persistence, reported latency, sidechain and bypass.
- [ ] VST3 hosting only after reviewing SDK/license obligations; LV2 assessment separately.
- [ ] Do not bundle or distribute commercial plugin executables.

## P4: interchange and polish

- [ ] Native versioned project format with migration tests and media packaging.
- [ ] Read-only subset of Ableton `.als` format behind clear compatibility disclaimers, corpus of legally obtained files.
- [ ] Evaluate safe round-trip preservation for supported XML parts, never imply lossless conversion.
- [ ] Glass-like UI polish with GPU/CPU profiling, scalable HiDPI layout and accessibility.
- [ ] Additional Linux distributions, platform packaging, reproducible release flow.

## Non-goals for MVP

- A cloned Ableton interface or imported proprietary assets.
- Claims of commercial compatibility without independently tested artifacts.
- Disabling audio safety/quality tests to meet a speed target.
- Production VST host, cross-platform parity or third-party plugin distribution in the first release.

Project management rule: link milestone checkboxes to actual PRs/tests, not verbal agent assurances.
