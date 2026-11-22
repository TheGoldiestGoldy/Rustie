# Rusty microphone workflow

**22 November 2026 · beta/integration · base bf52dee0003b**

## 1. What changed

| Area | Source | Behavior |
| --- | --- | --- |
| Project | `crates/daw-core/src/model.rs`, `lib.rs` | Saves the track’s physical channel choice alongside its PulseAudio source. Existing projects retain stereo on a two-channel source. |
| Capture and preview | `crates/daw-ui/src/studio_recording.rs`, `studio.rs` | The selected Audio track holds a single pinned input. REC reuses that stream when its settings match, applies the existing timing gate, and writes the chosen raw channel or pair. The WAV worker builds bounded waveform peaks. Device loss and stalls preserve a recoverable take through the normal finalizer. |
| Controls and feedback | `crates/daw-ui/src/studio_devices.rs`, `studio_views.rs`, `studio_session.rs`, `studio_track_management.rs` | Audio / MIDI offers Input 1 (Mono), Input 2 (Mono), and Stereo 1–2. The track meter distinguishes a selected input, signal, quiet input, and an unavailable device. REC shows elapsed capture time on the actual track, and its aligned region grows in Arrangement. The software monitoring checkbox starts off. |
| Playback verification | `crates/daw-ui/src/studio_export.rs` | Tests the reopened take through clip gain, solo, both pan extremes, group and master, and stereo export. The existing WAV decoder copies one-channel PCM to both sides at unity and preserves the stereo pair for imports. |

## 2. Native screenshots

- [AIR channel menu](../screenshots/2026-11-22/mic-channel-selection.png)
- [Input 1 showing signal before REC](../screenshots/2026-11-22/mic-input-meter.png) and Input 2 quiet (local QA artifact; not distributed)
- REC at 2.1 seconds (local QA artifact; not distributed) and [REC at 7.2 seconds](../screenshots/2026-11-22/mic-recording.png), both with Devices closed
- Reopened take exported (local QA artifact; not distributed) and imported stereo playing on the mono-input track (local QA artifact; not distributed)

## 3. Verification

### Automated

| Check | Result |
| --- | --- |
| `cargo test --workspace` | 159 passed, including eight realtime allocation checks. Two private PulseAudio tests are ignored by default. |
| `cargo fmt --all -- --check`, `git diff --check` | Passed on the scoped microphone patch. |
| `cargo clippy --workspace --all-targets -- -D warnings` | Passed on the scoped microphone patch. |
| `cargo build --release -p daw-ui -p daw-plugin-host` | Passed. The QA bundle verifies both executable hashes and the source patch. |
| `rusty --probe-recording-writer` | Exactly 441,000 frames at 44.1 kHz, with a zero-frame reference-transient error. |
| Private PulseAudio discovery | The inherited opt-in test passed 75 discoveries, rendered Audio / MIDI, then verified demo playback. The virtual output was fed continuously during the test. |
| Private PulseAudio lifecycle | 75 source and channel switches with one active capture stream. Removing the pinned source during REC surfaced an error and recovered one 4,719-frame mono clip. The alternate source opened and the capture connection retired on app drop. |

### Native desktop

The AIR 192|6 test used the normal native window attached to the real PulseAudio server from an isolated display. I selected the physical AIR source and toggled Inputs 1 and 2 in Devices. Input 1 showed live activity with Devices closed and the output engine idle. Input 2 was quiet. During REC the track timer and waveform advanced from 2.1 to 7.2 seconds. Stop created one 496,860-frame mono WAV at 44.1 kHz, lasting 11.27 seconds with zero dropped input frames. The logged aligned start was frame 133 and serialized as six project ticks.

I soloed the track at centered pan, saved, quit, and opened the project in a new process. Rusty’s own AIR PulseAudio playback stream measured identical left and right PCM at −22.39 dBFS peak per channel. The stereo export matched the overlapping raw mono samples exactly at the serialized clip start and copied them to both channels. Importing a stereo WAV through File → Import Audio on the same Mono 1 track preserved approximately 331 Hz on the left and 733 Hz on the right, at input file peaks of 0.019 and 0.049.

The private PulseAudio fixture also exercised the channel menu through the native GUI with contrasting tones. Input 1 wrote 331 Hz into a one-channel WAV. Input 2 wrote 733 Hz into a one-channel WAV. Stereo wrote both tones in their original channels. Pause finalized the first take, and pressing Stop afterward left the clip count unchanged. The private fixture used the ALSA Pulse PCM for output because native CPAL Pulse playback on an idle null sink stalled in the baseline diagnostic. Real AIR output used native PulseAudio.

The final scoped executable also passed a native GUI smoke with the private server’s output continuously fed. I selected Mono 1, closed Devices, recorded, stopped, and saved one 102,039-frame clip with zero drops.

### Routing and gain on the earlier take

| Native playback into the private PulseAudio monitor | Left peak | Right peak |
| --- | ---: | ---: |
| Original stereo take, unity | −19.88 dBFS | −76.33 dBFS |
| Original left copied to both sides, unity | −19.88 dBFS | −19.88 dBFS |
| Original left copied to both sides, +14 dB | −5.87 dBFS | −5.87 dBFS |

The original left channel in the raw WAV measured −19.88 dBFS peak and −37.02 dBFS RMS. The owner’s earlier successful copy combined centering and amplification. The later original take reached 0 dBFS on the left, so a fixed playback boost would risk overload. The existing recordings were read only.

### Owner listening

The owner confirmed the meter moved before REC and saved a 13.84-second Mono 1 take. The scoped executable validated the saved project and decoded the one-channel, 44.1 kHz WAV. Its peak was −15.70 dBFS. Clear spoken playback in both ears is pending owner confirmation.

## 4. Limits

The agent’s hardware take captured mostly room sound. PulseAudio playback-stream PCM and exported WAV data verify output routing and levels. Physical headphone clarity and acoustic round-trip alignment need the owner check. The private null-sink callback behavior is a separate output-clock limitation.

## 5. Remaining acceptance

Open the saved owner check project, solo the mic track and replay it at centered pan. Record whether the take is clear in each ear.

## 6. Git and artifacts

The working branch remains `beta/integration` at `bf52dee0003b`. The inherited uncommitted PulseAudio discovery changes are integrated into the workflow. Existing reports and concurrent Arrangement seek edits were preserved. No commit, push, or app-menu install was made. A detached QA worktree excluded the concurrent seek edits from the final mic-only build and automated results.

- Scoped QA executable (local QA artifact; not distributed) and bundle checksums (local QA artifact; not distributed)
- Saved AIR QA project (local QA artifact; not distributed) and exported mix (local QA artifact; not distributed)
- [Capture report](2026-11-22-mic-capture-and-meter.md), [playback report](2026-11-22-mic-routing-and-level.md), and [track UI report](2026-11-22-mic-track-ui.md)

## 7. Handoff

The executable reports the base revision because the mic changes are uncommitted. The manifest identifies the exact executable and source patch. Use the scoped binary and its adjacent worker for future mic-only checks. The versioned desktop launcher still points to its prior installed build.
