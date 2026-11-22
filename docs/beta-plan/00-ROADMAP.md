# Rusty Beta 1 | Finish-line roadmap

**Last reconciled:** 21 November 2026
**Canonical integration worktree:** `/home/developer/Projects/rust-daw-agent-integration`
**Target:** A usable Linux DAW for Beta 1.

## Current repository state

- The [coordinator's approved baseline](01-BASELINE-COORDINATOR.md) is `1ae60bb2106b595e7e46c33911753ee45eb6d6f6`. The original dirty `master` checkout remains outside this cycle.
- Wave 1 Recording, Editing and Plugins are merged. The deliberate Wave 2 base is `e6b82a79feeac4158f1918b3f5598916d5bb177c`.
- Wave 2 Session merged at `03782a7`, and Devices/UI merged at `89fb080`. The follow-up integrated the latest Recording, Editing, Session and plug-in handoffs and the approved SoundFont work. The current verified source is `028e2b1a8cd530af99fcc5e0910a930b075e1167`.
- Beta 1 remains **NO-GO**. The mandatory gate table, tested combinations and reproducible defects are in the [release decision](../BETA1_RELEASE_DECISION.md) and [integration report](../agent-reports/2026-11-21-beta-integration-followup-report.md).

## Delivery status

| Wave | State | Evidence or remaining work |
|---|---|---|
| 0 Baseline | Complete | Approved base and isolated worktrees |
| 1 Core | Integrated | Recording, Editing and Plugins |
| 2 Workflow | Integrated | Independent Session and render-clock launch, compact Devices UI |
| 3 Integration | Audited, NO-GO | 154 passing tests, native and virtual-audio checks, release defects recorded |
| Plug-in follow-up | Open | Scope and reproduction in the current [dispatch sheet](10-AGENT-DISPATCH.md) |
| 4 Owner acceptance | Open | Physical output, take alignment, headphones and current GUI |

## Beta 1 definition of done

All are mandatory for a GO decision:

1. Create/open/save/reopen projects; clips, routing, effects and supported plugin state survive cold restart; missing assets/plugins fail safely without data loss.
2. Audible MIDI and imported WAV; reliable play/pause/stop/seek, BPM/time signature, mute/solo/pan/group routing and metronome; no frozen playhead or unexplained drift.
3. Choose a physical mic/input, arm and record at least 10 seconds of voice/guitar, stop, see waveform, listen, save/reopen and export WAV. Verify duration and alignment; report predicted and measured latency separately.
4. Practical non-destructive audio trim/split/move/copy/duplicate, MIDI note/velocity editing, undo/redo, snapping and shortcuts. No silent history loss.
5. Tested live CLAP/VST3 effects and at least one external MIDI instrument; bounded worker failure, safe fallback, plugin identity/state handling and an explicit tested compatibility matrix. Never claim universal VST3 support.
6. Working compact browser/Devices UI, reorder/bypass, persisted EQ3/EQ8/Vocal Reverb/Guitar Reverb controls; no fake or unconnected parameters.
7. Independent persistent Session slots/scenes, quantized launch/stop, atomic scene behavior and queue-failure safety.
8. Full test/Clippy/format/release gate, 10+ minute synthetic playback/recording stress, realtime no-allocation guard and owner GUI/hardware smoke.
9. Reproducible Rusty launcher and matched UI/worker build, legal notices, checksum, sample project and skippable first-run/help flow.

## Scope boundaries and hard rules

- Beta 1 is a reliable first playable DAW, not full Ableton parity. Warp/time-stretch, automation lanes, advanced sidechains, large commercial-plugin compatibility, full MIDI CC/controller mapping, LV2 hosting and GPU glass effects are later work unless a real blocker changes priorities.
- Realtime callbacks must not allocate, lock, block on IPC, perform file I/O, spawn processes or call plugin code.
- No agent may touch the dirty original `master` checkout, share a writable worktree, delete another worktree, alter OS audio settings, save private recordings in Git, or push/publish.
- Every claim must be labelled PASS, FAIL or NOT TESTED with its actual evidence. Only Agent 07 may issue the final Beta 1 GO/NO-GO decision.
