# Rustie | After Beta 1: path to a maintainable v1.0

This file extends 00-ROADMAP.md. Beta 1 means the user can make and save a song; v1.0 means the core workflows are dependable and portable enough to ship. Neither means complete parity with commercial DAWs.

## Stage B: Beta 2 reliability and creator workflow
Entry: Beta 1 GO decision with remaining defect ledger.
- Recording: nondestructive comp/takes, punch-in/out, input meter, ergonomic arm/monitor, waveform redraw/performance and measured end-to-end latency on 2+ devices.
- Engine: disk-streamed large audio, high-quality resample, reliable glitch-free graph swap, durable XRUN/deadline telemetry, runtime rate/device-change recovery.
- Editing: multi-clip selection, fades/crossfades, common MIDI CC/sustain, precise time selection, clip gain, sample waveform zoom and robust history.
- Session: independent persisted slot/scene model proven under long sessions; UI-independent musical event scheduler.
- Plugins: CLAP/VST3 program/preset save, multiple buses where supported, parameter automation (with explicit plugin feature negotiation), MIDI note timing, worker resilience and latency compensation.
- QA: repeated stress runs, 2+ actual Linux distributions/hardware combinations, clean install and upgrade test. Release only against published and reproducible evidence.

## Stage C: v1.0 usable-production features
Entry: Beta 2 regression/compatibility gates pass.
- Track automation lanes and parameter mapping for built-in FX/plugins; state and undo project round-trips.
- Practical musical time stretch / warp modes with documented DSP limitations, transient correctness, rate adjustment and real quality listening tests. Do not claim artifact-free for all content.
- Comprehensive mixer: sends/returns, usable PDC, mute/solo/group semantics, metering and export/normalization/dither options.
- Interop: MIDI import/export, consolidated stems, bundled safe demo project, stable native project schema/migrator. Ableton .als optional and **not** v1 gate unless corpus/licensing support is proven.
- Product delivery: signed/versioned release packages for targeted Linux formats, CI build/test, crash reporting opt-in/privacy policy, plugin crash containment, backup/autosave recovery, license/SBOM audit and documented compatibility support policy.
- Documentation: full quick-start, keyboard reference, skippable onboarding, hover descriptions, real workflows and video/screenshot evidence.

## Stage D: post-v1 stretch goals
LV2 native hosting; broad vendor compatibility and custom plugin editors; deeper MIDI controller surface support; advanced sidechains and comping; true GPU blur/glass; cross-platform builds. Each is a separately gated project, not a reason to block a reliable v1.

## Later agent dispatch after Beta 1
Use the same six roles and branch rules, but begin with an integrator-led defect/prioritization pass. Recommended batches:
1. **Engine/recording + Plugin/automation agents** in separate worktrees. Merge and independently validate realtime limits and project schema.
2. **Editor/warp + Session/UX agents** from the new merged baseline, with tests against old saves. Merge.
3. **Packaging/reliability lead (Integrator)** alone: compatibility/device matrix, clean install, stress, backup/restore, release candidate, GO/NO-GO.

Do not automatically spawn post-Beta agents before Beta 1 results, because the actual failure ledger determines priority. The coordinator should author fresh narrowly scoped prompts from real remaining defects rather than extrapolating claims.

## v1.0 release gate
1. User can produce, record, edit, mix, reopen and export a 10+ minute project with actual hardware and plugin instances without data loss.
2. End-to-end latency, XRUN/deadline statistics and supported channel/sample-rate matrix are independently measured; known limitations are published.
3. Saved projects from every supported release load with deterministic migrations and safe missing-plugin handling.
4. Host survives supported plugin crash/hang, invalid state and device loss without audio-thread deadlock, hangs or data corruption.
5. Automated CI + manual GUI/hardware validation are reproducible. Regressions are tracked and prioritized; release binary reproducible and legally cleared.
6. A compatibility table replaces unqualified “full VST/VST3 support.” A supported format is not a promise that every third-party implementation works.
