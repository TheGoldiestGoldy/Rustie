# Rusty microphone controls and recording display

## 1. What I implemented

| Area | Source | Change |
| --- | --- | --- |
| Audio / MIDI Devices | `crates/daw-ui/src/studio_devices.rs` | Choose the device and physical channels for an Audio track. The source list still uses the shared two-second cache. The monitoring checkbox remains independent of metering. |
| Arrangement | `crates/daw-ui/src/studio_views.rs` | The Audio track header shows its input state and peak with Devices closed. The active track shows REC and elapsed time. Its preview starts at the capture alignment position and grows with captured frames. |
| Session and Mixer | `crates/daw-ui/src/studio_session.rs`, `crates/daw-ui/src/studio_track_management.rs` | Audio channel strips show the same input state and REC clock. Session also shows the active take envelope in the channel header. |

## 2. Current state and real screenshots

I reviewed native AIR 192|6 screenshots with the physical channel chooser open, the Input 1 signal and Input 2 quiet states with Devices closed, and the REC preview at 2.1 and 7.2 seconds. The track REC label and waveform extent advanced between captures. The screenshots exposed a wrapped IN button, which now uses tighter padding. The integrated handoff contains the native GUI evidence.

## 3. Verification

| Check | Command | Observed result |
| --- | --- | --- |
| Format | `cargo fmt --all -- --check` | Passed |
| Track UI regressions | `cargo test -p daw-ui track_lane_context_tests` | 5 passed. The new egui case found the unavailable input state on the selected Audio track and the inactive meter on the other track, with Devices closed. It also checked that IN renders on one line under the app theme. |
| Source whitespace | `git diff --check` | Passed |
| Native GUI, release build, Clippy | Integrated verification | See the integrated handoff. |
| Owner listening | Physical headphones | Open. The UI regression supplies no listening evidence. |

## 4. Known limitations

The selected Audio track holds the pre-recording meter. During REC the capture track holds the meter even when a different track is selected. The preview draws the bounded peak envelope for display, and clip audio uses the saved WAV.

## 5. What remains

Confirm from the native window that the track meter responds to the owner's mic before REC and the waveform grows during an AIR 192|6 take. Confirm playback in both ears at center pan with the take track soloed. Record listening separately from digital checks.

## 6. Git and files

- Branch `beta/integration`, base `bf52dee0003b` at the start of the work.
- This pass changed the four UI files above and this report. No separate UI commit or push was made.
- The report contains test output and synthetic UI state details. No private media is included.

## 7. Next-agent handoff

Click **IN** on an Audio track in Arrangement to open its Devices settings. Choose the AIR source and a physical channel. Close Devices and inspect the meter below **PAN**. Start REC and follow the elapsed label and preview on that same track. See the integrated handoff for capture, playback, and hardware details.
