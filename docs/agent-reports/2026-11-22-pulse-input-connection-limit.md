# PulseAudio input discovery and playback check

## What changed

The Audio / MIDI panel opened a CPAL PulseAudio host on each redraw. During the reported failure the PulseAudio journal logged its 64-connection limit and rejected even pactl. The Rusty process held 57 socket descriptors. After the user closed Rusty, pactl worked again and listed the AIR 192|6.

Input discovery now uses a short PulseAudio source query with explicit socket shutdown. The panel caches the result for two seconds and presents connection errors separately from an empty physical input list. Recording resolves the current source again when started. Output driver selection and transport logic were left as implemented.

## Current state

The local build was launched from the integration target folder with the normal session environment. The app menu still points to the versioned QA installation. The source is based on bf52dee with local changes. The window displays that base revision.

## Verification

The workspace tests, strict Clippy, release UI and worker build, and Git whitespace checks passed. A private PulseAudio test performed 75 input discoveries in the same process, confirmed output monitors were excluded, rendered the Audio / MIDI controls, and started the ordinary demo playback method. It observed nonzero PCM and a progressing playhead before the four-second end of the demo. The real server listed the AIR 192|6 after the original window closed. Physical microphone capture and listening were not performed.

## Known issues and next checks

Confirm the real interface appears in the local build's input dropdown, and record and listen to a short take. The specific fast-playhead symptom was not independently reproduced on hardware. If it recurs with the new build, capture the active driver, device and callback rate. The Beta 1 gate remains closed.

## Git and handoff

Changed crates/daw-ui/src/studio.rs, studio_devices.rs and studio_recording.rs, including an opt-in regression test for an isolated PulseAudio server. No commit or push. The local executable is at ~/.cache/rustie-target-integration/release/daw-ui. Use the matched nearby daw-plugin-host binary.
