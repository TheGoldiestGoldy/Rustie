# Rusty Beta 1 | Agent dispatch sheet

Both waves and the current specialist follow-ups are merged. Start any follow-up from the latest `beta/integration` checkout. The tested source is `028e2b1a8cd530af99fcc5e0910a930b075e1167`. The sole integrator retests and updates the [release decision](../BETA1_RELEASE_DECISION.md).

## Scoped follow-ups

1. **P0 · PulseAudio startup.** Reproduce on private Null Output sinks at 44.1 and 48 kHz using `RUSTIE_PROBE_RATE=RATE rusty --probe-transport 128 1000`, the 2.5 s retry and the comparison script (local QA artifact; not distributed). Compare previous and current source. Check why the first callback often fails to progress for up to two seconds and verify repeated cold starts at 64, 128 and 512 requested frames. Keep PulseAudio profile and device defaults intact on the owner's session. Preserve the existing low-latency and recording paths.
2. **P0 · Owner acceptance.** On the resulting development build, play the review song and a stereo WAV at the AIR 192|6 actual sink rate. Check transport, Session and editing transitions. With physical-input permission, record at least ten seconds of voice or guitar, listen through headphones, see the waveform, save, cold reopen and export. Measure alignment against a reference transient and record any device XRUN or scheduler evidence.
3. **P0 · Packaging.** Confirm contributor rights and add project and included dependency notices. Build the desktop UI and companion worker together from one clean source. Verify the version, both SHA-256 checks, starter project and first-run flow.
4. **P1 · Live mixdown and compatibility.** Preserve the current transparent export guard. Agree on and verify a supported way to print the active track or complete live device chain using its saved native state. For LSP VST3, reproduce the ambiguous retired class metadata with the installed bundle. Verify that any compatibility changes preserve exact current-class binding and extend the long multi-slot worker test.

See the [integrator brief](07-INTEGRATION-QA-AGENT.md) and [current evidence](../agent-reports/2026-11-21-beta-integration-followup-report.md).
