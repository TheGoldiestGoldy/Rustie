# What is different—and what still has to be earned

## Compared with established DAWs

Rusty's Arrangement, Session grid, piano roll, browser and mixer use familiar music-production conventions. Clip launching and linear arrangement are not new inventions: Ableton documents both workflows in its [Session manual](https://www.ableton.com/en/manual/session-view/) and [Arrangement manual](https://www.ableton.com/en/manual/arrangement-view/). Rusty is not claiming feature parity with established DAWs.

| Dimension | Rusty's deliberate direction | Current evidence | What would establish an advantage |
|---|---|---|---|
| Linux workflow | Native Rust/egui app; native audio paths; Rusty does not require Wine | Compiled desktop application and Linux device probes | Reliable installation, device recovery and low-friction recording across machines |
| Recording clarity | Physical channel choices, input signal before REC, capture-driven track feedback | 22 November mic workflow, native captures and PCM checks | Repeated owner listening and first-use success without coaching |
| Vocal expression | Autonatural applies region-level pitch offsets with a tolerance band | Separate offline Lab, synthetic render/import and unchanged-source checks | Level-matched real-vocal listening, preservation/artifact gates and useful workflow studies |
| Implementation | Rust core, bounded command/capture paths, isolated plug-in workers | Source and targeted tests, named plug-in and fault probes | Measured deadlines/latency and long-session recovery across supported systems |
| Development transparency | Publish evidence and unresolved gates with the source | Reports, screenshots, NO-GO decision and verification ledger | Continued accuracy and reproducible results over releases |
| Interface | Original Midnight Ember visual language with familiar conventions | Actual native screenshots | Musicians completing tasks faster and with fewer errors |

These are design choices and opportunities, not exclusive features. Native Linux support, open development and process isolation are not unique industry-wide. Rust does not automatically guarantee glitch-free audio. A distinctive appearance does not establish a better recording workflow.

Autonatural's focus on pitch centers is also not a claim of first invention. Melodyne explicitly provides independent editing of [pitch center](https://helpcenter.celemony.com/M5/doc/melodyneStudio5/en/M5tour_ToolPitch_2?env=cubase) and [modulation/drift](https://helpcenter.celemony.com/M5/doc/melodyneAssistant5/en/M5tour_ToolModulationDrift_2). Rusty's opportunity is the quality and convenience of the complete experience if the research succeeds.

## Compared with the idea of a “vibe-coded project”

“Vibe-coded” is an informal label, not a measurable quality category. Rusty uses AI coding assistance openly. We are not asserting that other AI-assisted projects lack engineering discipline, or that generating more code proves product value.

The relevant distinction is the standard applied to each change:

| Question | Evidence we keep |
|---|---|
| Does the feature exist beyond a mockup? | Rust source, compiled native UI and real captures |
| Did it process audio? | Decoded/captured PCM, channel comparisons and named plug-in probes |
| Did a person hear the intended result? | A separate owner-listening gate, never inferred from PCM |
| Can it survive edits and reopen? | Project/undo regressions and native save/cold-reopen exercises |
| Is callback work bounded? | Fixed queues, off-thread I/O and realtime allocation guards |
| What failed? | Specific defect reports and a release NO-GO when gates stay open |
| Can someone reproduce it? | Build commands, source identifiers, test fixtures and dated reports |

The project has already exposed why that standard matters: a plausible waveform did not establish useful mic playback, successful startup did not establish prompt transport, and passing tests did not close physical listening acceptance.

## Claims we are deliberately not making

No assertion of being the first Rust DAW, faster than every competitor, universally compatible with VST3, ready to replace a professional studio, or proven to preserve every singer's timbre. No blanket judgment about the quality of other “vibe-coded” software.

Competitor references were checked on 22 November 2026. They establish familiar existing capabilities, not a comprehensive market survey. Our performance and usability claims must be earned by the [roadmap gates](ROADMAP.md).
