# Roadmap: evidence before release dates

The roadmap has acceptance gates, not a promised v1 date. Rusty does not need complete commercial-DAW feature parity to be useful, but it does need a dependable recording-to-export workflow.

| Stage | Outcome | Remaining acceptance |
|---|---|---|
| Beta 1 — make and save a song | Core recording, MIDI/audio editing, Session, effects, save/reopen and export work together | Resolve startup failures; complete physical input/output, listening and alignment; confirm supported live-insert print workflow; finish packaging and notices |
| Beta 2 — make it dependable | Repeatable creator workflows across supported environments | Device/rate recovery, measured latency and underruns, long audio streaming, long Session/plugin runs, clean install/upgrade and at least two Linux hardware/distro combinations |
| v1 — usable production release | A documented, supportable workflow for real music projects | Automation, practical warp/time stretch, sends/returns and usable PDC, MIDI/stem interchange, migrations/recovery, versioned packages and compatibility evidence |
| Research alongside the DAW | Determine whether Autonatural earns an integrated role | Real-vocal listening and quality gates, reversible workflow testing and measured time-to-result |

## Immediate priorities

1. Finish the mic/seek acceptance record, then verify their combined source and matched desktop build.
2. Reproduce and resolve delayed or stalled output startup, keeping callback timing separate from physical latency.
3. Complete a real interface session: record, listen, edit, save, cold reopen and export, with reference-transient alignment.
4. Define a verified way to print supported live inserts; keep export limitations visible until it exists.
5. Finish distribution notices, contributor-rights confirmation and repeatable packaging.

## v1 release test

Produce, record, edit, mix, reopen and export a real project longer than ten minutes with supported hardware and plug-ins, without data loss. Publish measured latency and deadline/underrun results, verify old-project migration and missing-device recovery, exercise plug-in faults, and reproduce installation and regression checks.

The detailed engineering plan remains in [Beta 1 roadmap](beta-plan/00-ROADMAP.md) and [post-Beta path to v1](beta-plan/09-POST-BETA-TO-V1.md). Some planned items now have implementations; the acceptance gates, not unchecked historical lists, determine release status. Autonatural remains a separate research alpha outside the documented v1 gate.
