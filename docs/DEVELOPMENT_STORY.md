# Development story

This chronology summarizes the dated reports. Earlier limitations describe their own revisions; they must not be read as the current feature list.

| Date | Work and feedback | What it established |
|---|---|---|
| 16 November 2026 | Musical time/model and native audio engine foundation | Native callback/render paths, scale/time tests and initial realtime allocation guards |
| 17 November | Midnight Ember interface, piano roll, browser and Session; reviewer workflow; group routing and mixer | A working native UI, actual MIDI starter patterns and concrete owner feedback |
| 17 November | Autonatural DSP/CLAP experiment and separate Lab | A distinct research path for note-centered vocal correction; quality remained open |
| 18 November | Video traced horizontal drift to Library width; output clock diagnosis and empty-lane seek | Fixes based on observed behavior; initial sample-rate/callback evidence |
| 19 November | Hardware MIDI audition, native effects, metronome, physical capture and offline CLAP rendering | Real device/plugin paths, with physical listening explicitly separated |
| 20 November | Isolated live CLAP/VST3 effects and instruments, state work, matched versioned launcher, first beta integration | Multiple functional subsystems integrated; scoped follow-up work remained |
| 21 November | Independent Session persistence, batch editing, pinned recording, SoundFont instruments, plug-in hardening | Integrated baseline passed 154 tests; over ten minutes of virtual recording; release stayed NO-GO |
| 21 November | Autonatural Lab port | Current-source Lab workflow with unchanged-source synthetic render evidence |
| 22 November | Owner heard imported WAVs and a prepared copy, reopened sessions, but reported inaudible mic takes and weak recording/seek feedback | The next work was driven by actual use: channel selection, input meter, capture preview and click-to-seek |
| 22 November | Mic and seek follow-ups | See [current status](PROJECT_STATUS.md) for completion and remaining acceptance |

## Lessons from the work so far

A waveform means samples were captured; it does not tell the musician whether routing and level are useful. The mic investigation separated left/right mapping from added gain. A copied take with +14 dB was not evidence that channel mapping alone solved perceived loudness.

A moving playhead is not a measurement of sound at the headphones. Virtual sink captures and render tests can narrow a fault, while physical onset, recording alignment and subjective quality require different checks.

A native interface can still expose misleading or incomplete workflows. The project therefore keeps feedback such as drifting layout, missing mixer controls and absent capture indicators alongside the engineering changes they motivated.

AI-assisted development needs integration ownership: scoped work, preserved parallel changes, a known source snapshot, a matching UI/worker build and a new combined check. Separate successful agent reports do not automatically certify the merged program.

## Explore the record

[Agent reports](agent-reports/README.md) · [Verification ledger](VERIFICATION.md) · [Historical README snapshots](history/README.md) · [Beta planning](beta-plan/00-ROADMAP.md)
