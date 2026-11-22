# Rusty Beta 1 release decision

**NO-GO · 21 November 2026**

Verified source `028e2b1a8cd530af99fcc5e0910a930b075e1167` on `beta/integration`. The PulseAudio transport sometimes took close to two seconds to start in isolated, clock-matched tests. This also reproduced on the previous build. Physical AIR 192|6 acceptance and distribution notices remain open. The release gate remains closed.

## Mandatory roadmap gates

| # | Criterion | Status | Evidence |
|---|---|---|---|
| 1 | Create, open, save and reopen projects with safe asset and plug-in recovery | **NOT TESTED in full** | Started a new song with three MIDI tracks, imported stereo WAV through Library and saved through the native dialog. Legacy and new formats passed preflight. Native state, clip and Session data survived cold reopen. Missing WAV preflight returned a specific error. Full autosave recovery was not repeated on this revision. |
| 2 | Audible MIDI/WAV transport, mixer routing, metronome and stable playhead | **FAIL** | The current build failed to advance within the 1 s probe at each 44.1/48 kHz and 64/128/512 test combination on at least one attempt. At 128 frames, a 2.5 s retry observed first advance in 845.53–1,996.72 ms. Native Session played through the virtual sink. Physical output and timing await owner checks. |
| 3 | Physical take of at least ten seconds, monitoring, waveform, save/reopen, export and alignment | **PARTIAL** | The AIR 192\|6 native test captured an 11.27-second take with zero drops, showed capture progress, and passed save, reopen, digital playback and export. The owner confirmed pre-REC metering and saved a 13.84-second mono take. Headphone clarity and acoustic reference-transient alignment remain open. [Mic workflow report](agent-reports/2026-11-22-mic-workflow.md) |
| 4 | Non-destructive audio/MIDI editing, undo/redo, snapping and shortcuts | **PASS** | Native Shift-click split across MIDI and WAV at 127 BPM in 6/8, with saved counts 8 → 10 → 8 → 10. Widget tests covered drag capture, source offsets and note/velocity edits. Audible continuity awaits listening. |
| 5 | Live CLAP/VST3 effects and external instrument, state and bounded failure | **PASS for tested combinations** | Four named combinations passed paced probes at both rates. Two Surge instruments and the Surge VST3 effect survived cold reopen with identical state. Both formats preserved dry PCM and reaped faulted workers. Arrangement export requires bypass or printed audio. The installed LSP VST3 bundle still fails class inspection. |
| 6 | Compact Devices and Library, persisted native EQs/reverbs | **PASS** | Native 980×640 Devices saved EQ8 gain, reordered the chain, bypassed EQ3 and kept both reverbs. Native SoundFont mouse and computer-key audition produced PCM on the private sink. |
| 7 | Persistent independent Session slots and atomic quantized launches | **PASS** | Four slots switched to scene 2 on the Quarter grid. Saved Arrangement and Session counts stayed independent. Tests cover queue saturation, rejection, cancellation, loop phase and Session clipboard behavior. |
| 8 | Quality gate, 10+ minute synthetic playback/recording, realtime guard and owner smoke | **NOT TESTED in full** | 154 workspace tests, eight realtime allocation checks, format, strict Clippy, release build and Git checks passed. Virtual capture recorded 615.816 seconds with zero dropped frames, survived a cold reopen, and exported to a 616.320-second WAV. Owner smoke and backend XRUN/deadline measurement remain open. |
| 9 | Reproducible launcher, matched worker, legal notices, checksums, sample project and help | **FAIL** | Both executable checksums passed, and the isolated launcher, sample project and tutorial Skip worked. The manifest reflects the locked Linux Rust 1.89 requirement. Contributor-rights confirmation and complete project and dependency notices remain open. |

The [integration report](agent-reports/2026-11-21-beta-integration-followup-report.md) gives exact commands, measurements, screenshots and merge history. The [follow-up handoff](beta-plan/10-AGENT-DISPATCH.md) lists remaining checks and defect reproductions.
