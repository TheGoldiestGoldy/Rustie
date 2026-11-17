# Known issues and reproduction guide

**Checked-in baseline:** `f578b9a` 2026-11-17. All entries are open until a measured test closes them.

## High priority

### AUDIO-001: Visual timeline appears ahead of actual audible MIDI

**Observed:** User reported pressing Play, seeing notes/playhead active, yet hearing the sequence only after roughly four notes had elapsed. Report predates/overlaps native ALSA backend and scheduling changes. No reliable post-change loopback verification exists.

**Reproduce:** Launch UI with default eight-note MIDI demo. Monitor transport and hear output from the intended physical speakers/interface. Start from cold stop, repeat across ALSA/PulseAudio/JACK only when each is physically routed.

**Collect:** DAW commit, audio backend, hardware device/sink profile, sample rate, buffer request, user-visible notes timeline, generated peak, predicted delay, true loopback output timestamp if available, xruns and queue overflow.

**Done when:** First note is audible in a consistent, measured time aligned with its on-screen event; repeat runs do not lose initial notes. Do not close based only on a nonzero generated audio peak.

### AUDIO-002: Engine produces samples but no audible hardware output

**Observed historically:** JACK callback produced nonzero frames but the separate graph had no physical sink; M-Audio AIR profile at one point exposed IEC958 instead of analog output.

**Reproduce:** Probe with explicit backend, inspect desktop audio routing and device profile, measure output on the actual intended port. Do not silently change OS audio settings during active recording or calls.

**Done when:** Explicit audible hardware test succeeds on the preferred device and routing can be selected/diagnosed without global system changes.

### UI-001: Visual density and workflow below professional DAW standard

**Observed:** User described the initial UI as unlike a professional DAW. Colors were acceptable; layout/usability and core workflows were not.

**Resolve:** [UI spec](UI_UX_SPEC.md), user-tested track controls, browser, reusable panes, piano roll and real Session launches. Verify usage at 1920x1080.

### SESSION-001: Session grid is not a clip launcher

**Present:** Session overview draws slots and selects clips. Independent clip trigger, launch quantization, scene playback, queued states and stop buttons are not implemented in baseline.

**Done when:** All semantics in [UI spec](UI_UX_SPEC.md) have deterministic timing tests and a real hardware smoke test.

### PLUGIN-001: Plugin browser can display names without ability to instantiate

**Present:** Host/IPC crates are placeholders. A concurrent browser agent added file discovery in an uncommitted working tree. Discovery must never imply a plugin loads.

**Done when:** CLAP worker/IPC integration executes a legal test plugin with crash recovery, project state and latency handling.

## Other limitations

- **MEDIA-001:** Audio source files stored as external paths; projects break if files move. Add relink / portable packaging.
- **MEDIA-002:** In-memory decode and linear sample-rate conversion are not suitable for multi-hour/high-fidelity production.
- **MIDI-001:** No physical MIDI input, real MIDI device selection or input monitoring.
- **RECORD-001:** No audio capture, armed recording, input routing or monitoring.
- **EDIT-001:** Full UI-level undo/redo, non-destructive clip trim and crossfades not complete.
- **EXPORT-001:** No production full-mix export/bounce path.
- **LATENCY-001:** No independent p95 onset, end-to-end or round-trip measurement; no quantitative xrun telemetry.
- **FILE-001:** Ableton `.als` import/export not implemented. JSON schema has no explicit version/migrations.

## Bug report checklist

Provide reproduction steps, expected vs actual results, commit SHA, OS/session, interface/backend, sample rate and buffer, minimal self-made project, sanitized logs and screenshots, crash backtrace if any, and whether the issue also exists without third-party plugins. Never upload confidential audio, device serials, personal paths or credentials.
