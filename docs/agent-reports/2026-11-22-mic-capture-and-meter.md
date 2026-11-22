# Rusty microphone capture and meter engineering report

## 1. Implementation

`crates/daw-ui/src/studio_recording.rs` keeps one pinned PulseAudio input stream on the selected Audio track. It reads the source’s native channel map and selects the requested physical channel locally. The input meter works with the stream unarmed. The WAV queue and optional software monitor start after the take is armed and reject stale input using the existing capture timing gate. The default software monitor setting is off. A mono take stores one channel at its original amplitude.

Capture progress counts accepted frames. The WAV worker builds peaks in 2,048 bins and merges neighboring bins as the take grows. Stop, Pause and input failure use the existing writer drain and aligned Arrangement clip finalization. A device change closes the prior socket and waits for its capture callback to retire before opening a replacement.

## 2. Current state and screenshots

The primary integration report contains native desktop screenshots. This report covers the capture implementation and focused tests.

## 3. Verification

| Check | Procedure | Result |
| --- | --- | --- |
| Focused automated tests | `cargo test -p daw-ui recorder_tests -- --nocapture` | 14 passed before the isolated Pulse test was added. Distinct float samples arrived in the modeled left and right lanes. Assertions covered the unarmed meter, rejected pre-roll, raw sample preservation in all three modes, and bounded waveform compaction. |
| Clippy | `cargo clippy -p daw-ui --all-targets -- -D warnings` | Passed. |
| Physical short capture | `cargo run -p daw-ui -- --probe-recording` | AIR 192 6 Analog Stereo through PulseAudio delivered 15,081 frames at 44.1 kHz, zero dropped. The stereo float WAV ran 342 ms and was removed. Predicted input/output timing offset was +0.39 ms. |
| Private PulseAudio lifecycle | `studio::recorder_tests::private_pulse_switches_inputs_and_recovers_take_after_disconnect` | The ignored test passed in 4.33 seconds when run with its private fixture. Across 75 source and channel changes, one client capture stream remained active. Recording reused it. Unloading the source produced an error and one recovered mono clip of 4,719 frames with zero dropped. The other source opened afterward. No Rusty client remained after Studio dropped. |

## 4. Limitations

The short hardware probe exercised capture and file writing. The primary integration report tracks the separate native ten-second take, save and replay workflow, and owner listening gate.

## 5. Remaining work

Use the installed build and have the owner verify the soloed mono take at neutral pan through both headphone channels.

## 6. Git and files

Work performed on `beta/integration` from `bf52dee0003b`. This agent edited `crates/daw-ui/src/studio_recording.rs` and added this report. No commit or push by this agent.

## 7. Reproducing the private server check

Run the ignored test with a disposable HOME and a private PulseAudio socket whose paths start with `/tmp/rusty-capture-lifecycle.`. The server needs a 44.1 kHz, two-channel `qa_null` null sink and two `module-remap-source` instances named `qa_synthetic_input` and `qa_alt_input`, both fed from `qa_null.monitor`. Configure the ALSA default PCM as PulseAudio targeting the private `qa_null` and feed the sink continuously with `pacat --playback --raw --rate=44100 --format=float32le --channels=2 --latency-msec=30 --device=qa_null /dev/zero`. The test removes the private `qa_alt_input` module mid-take.
