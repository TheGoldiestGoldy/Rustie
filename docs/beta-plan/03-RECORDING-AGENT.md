# Agent 02 | Capture, Recording, Monitor and Clock | WAVE 1 COMPLETE

> Wave 1 recording work is merged into `beta/integration` at `0c48dfb`. Do not rerun this prompt unchanged. Agent 07 may dispatch a narrow recording follow-up from a fresh integration SHA if integrated QA identifies a reproducible defect; physical mic/headphone acceptance remains an owner task.

You are the recording/latency engineering agent. Worktree ONLY:
  /home/developer/Projects/rust-daw-agent-recording (branch beta/recording).
Before editing confirm BASE_SHA, read AGENTS.md, docs/beta-plan/08-AGENT-CONTRACT.md, docs/FEATURE_MATRIX.md,
docs/agent-reports/2026-11-20-beta-five-blocker-hardening.md and docs/agent-reports/2026-11-18-output-clock-and-timeline-seek.md.

## Mission
Make a real microphone/guitar take reliable: input selection, arming, capture, waveform, monitoring, latency alignment, stop, project save/reopen and export. Keep playback and metronome stable. Fix evidence-backed problems, not hypothetical buffer tweaks.

## Main ownership
- crates/daw-ui/src/studio_recording.rs, studio_midi_recording.rs if necessary for record ownership; crates/daw-engine/src/monitor.rs, input capture modules and focused tests.
- Changes to driver.rs, engine.rs, studio.rs and graph.rs are cross-agent hotspots. Isolate them, keep patches minimal and hand explicit integration instructions to Agent 06. Do NOT touch plugin worker protocol or Session persistence.
- Never change default OS audio output, installed sound server, real recordings or input device selection without owner approval.

## Ordered tasks
1. Trace selected PulseAudio/JACK/ALSA capture source through fresh-input 512-frame gate, stream timestamp, sample rate, WAV writer, clip placement, waveform and project persistence. Verify no output monitor is mistaken for microphone; unplugged named input must error explicitly.
2. Add focused capture writer, boundary, buffer overrun and backpressure tests (0 dropped is a measurement, not a guarantee); output clear telemetry for frames written, queue loss, callback timestamps, input stream start and playback frame alignment.
3. Test >=10 seconds synthetic/loopback source recording to WAV; inspect sample count/duration and known transient/click placement. If permitted and available, run isolated real AIR 192|6 physical input test; clearly label predicted vs measured latency and device routing. Never save private mic content to repo.
4. Verify record-armed state transitions: start/stop/pause/seek while recording, source disappearance, abort and disk-write failure, track change, project save/reopen. Ensure partial takes are recovered or clearly invalidated, not silently presented as complete.
5. Verify monitoring is opt-in, bounded and feedback-resistant; do not double-monitor by default. Confirm recorded clip is playable and exported WAV is real non-silent audio when an input signal exists.
6. Finish missing recording metronome ergonomics only if safe: user-selectable one/two-bar count-in, no click baked into mic take unless physically captured, time signature and volume persisted. Do not make feature-complete claims without real listening.
7. Investigate reported playhead/audio onset gap by measuring engine command/sink callback and, if available, acoustic/loopback onset. Seek/pause/resume must not hang or rebuild needlessly. Preserve existing wrong-rate sink negotiation safety.

## Acceptance and evidence
- Actual sampled WAV validates length, channels, sample rate and header; actual UI saves/reopens a take, not just --probe-recording.
- Device loss and record start/stop create no hidden mic switching or callback deadlock; logs report dropped frames.
- Synthetic timing test includes expected vs actual transient frame delta; measured physical round-trip only if a real reference loopback was used.
- Timeline, MIDI play and metronome remain working in existing review project.
- Shared contract checks; tests include no-allocation guard relevant to audio callback.
- Any manual headphone/microphone test that cannot be run must remain a named BLOCKER FOR HUMAN ACCEPTANCE, never a green test.

## Deliverables
Commit on beta/recording. Write docs/agent-reports/YYYY-MM-DD-beta-recording-report.md including waveform/reopen evidence, timing table, source/device details excluding personal recordings, exact commands, failures, hot-file edits and integrator notes. No merge or push.
