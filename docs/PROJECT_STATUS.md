# Current project status — 23 November 2026

**Development preview. Beta 1 remains NO-GO. Both requested agent updates are not fully accepted.**

## 23 November experimental instrument branch (draft, not merged)

The separate instrument branch adds two foundations:

- **sf-16:** a 16-channel SoundFont engine with per-part program, controller, tuning, sustain, pitch-bend and mute/solo state. The engine and MIDI plumbing are implemented; a dedicated 16-part editor is not.
- **LNF-1:** an original 32-voice piano-style instrument and native Devices panel. Its 20 parameters persist per track and update active sources through atomics. It is not sample-parity with the referenced GPL plug-in and includes no third-party samples.

The combined branch independently passed **173 workspace tests**, with **two opt-in PulseAudio tests ignored**, plus formatting, strict Clippy and a release UI build. This is code-level evidence only: manual LNF-1 GUI acceptance, physical MIDI, speaker/headphone listening and hardware latency were not tested. General host automation/undo and LNF-1 telemetry remain unfinished. Beta 1 remains **NO-GO**.

## Latest workstreams

| Workstream | Implementation | Verification and remaining work |
|---|---|---|
| Microphone workflow | Persisted Mono 1 / Mono 2 / Stereo input choice, input meter before REC, capture-driven timer and waveform, centered mono playback | Agent implementation/handoff completed; headphone acceptance still open. Scoped mic build: 159 passing tests, eight realtime allocation guards, two opt-in PulseAudio tests ignored by default. Native AIR recording/save/reopen/replay/export and stereo preservation exercised. Owner confirmed pre-REC meter movement. Clear spoken playback in both headphones remains unconfirmed. |
| Arrangement click-to-seek | Working-tree code adds release-click seeking on ruler/empty lanes/clip bodies, pinned ruler, tempo-map coordinates and queued seeks without reopening the stream | Agent stopped at a usage limit before completing compiled-app/audio acceptance and final handoff. Included as unfinished development work, not a completed fix. Repeated-seek latency, real pointer exclusions, scrolling/zoom and heard MIDI/audio position still require acceptance. |
| Publication | Current source snapshot, public-facing pitch, documentation, gallery and history | Combined snapshot checks are recorded in [verification](VERIFICATION.md). Publication does not install a new desktop build or close release gates. |

The [mic workflow report](agent-reports/2026-11-22-mic-workflow.md) separates native interaction, PCM measurements and owner listening. Its mic-only build deliberately excluded concurrent seek edits. Its 159-test result must not be attributed to the combined snapshot.

## What we have established

The 21 November integration demonstrated native Arrangement editing, independent Session scenes, native effects persistence, SoundFont audition, named CLAP/VST3 classes and save/cold-reopen behavior. It passed 154 tests, including eight realtime allocation guards. A virtual capture ran 615.816 seconds with zero reported dropped frames and exported after reopening.

The 22 November mic candidate captured an 11.27-second mono AIR take with zero dropped input frames. Solo playback PCM was identical left/right at centered pan; exported overlapping samples matched the mono source. Imported stereo retained its two channels. The owner subsequently confirmed the meter and saved another mono take. These facts do not replace headphone listening or analogue alignment measurements.

## Source identity

The publication preserves the existing GitHub `main` history rooted at public commit `d20a4f3f555ce9d0f0725b4a85f290b753e9b2f1`. Application source was copied from `beta/integration` base `bf52dee0003b` plus the observed mic and unfinished seek working-tree changes into an isolated publication checkout. The original development worktree was not reset, committed or installed by this documentation pass.

The exact source-file hashes are in [source snapshot manifest](source-snapshot-sha256.json). Historical local commit IDs identify the original development history; those Git objects are not necessarily ancestors of the public snapshot. Screenshots identify their own candidate or baseline and are not falsely attributed to the publication build.

## Still open

Output startup reliability, owner headphone clarity, reference-transient hardware alignment, measured analogue latency and backend XRUN/deadline evidence; complete live-insert print/export workflow; broader plug-in/device and long-session recovery coverage; distribution and contributor notices. [Release decision](BETA1_RELEASE_DECISION.md) · [Limitations](KNOWN_LIMITATIONS.md) · [Roadmap](ROADMAP.md)
