# ADR 0001: Native-first Rust DAW architecture

- **Status:** Implemented baseline, pending production validation
- **Date:** 2026-11-16
- **Scope:** Core/engine/UI direction

## Context

The goal is to create an original Linux digital audio workstation with a music-theory-aware editor, timeline and eventual clip grid, without depending on Wine to host the base application.

## Decision

Use a Rust workspace with a domain-only `daw-core`, callback-oriented `daw-engine`, native `eframe/egui` UI, and separate future crates for plugin hosting and project interchange. Use `CPAL` for native output, bounded `rtrb` queues and preallocated buffers in the audio callback. Keep MIDI/project musical time at 960 PPQ.

## Consequences

- Rust safety and explicit thread boundaries provide a testable foundation.
- Linux audio paths must be measured on real devices; CPAL alone cannot guarantee predictable end-to-end latency.
- Backend and GUI choices are engineering constraints, not a guarantee of cross-platform support.
- Isolated plugin hosting, recording, native project format evolution, transport sync and full Session launching remain future work.
- See `docs/AUDIO_ENGINE.md` and `docs/PROJECT_STATUS.md` for actual implementation evidence.
