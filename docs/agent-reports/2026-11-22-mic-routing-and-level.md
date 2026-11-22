# Rusty microphone routing and level verification

## 1. Implementation

- `crates/daw-core/src/model.rs` and `crates/daw-core/src/lib.rs` define a saved physical-channel choice for an Audio track. Stereo is the legacy default. A source with one channel uses Mono 1 by default. The capture implementation consumes this choice. Playback follows the saved media file's channel layout, independently of the track's current recording input.
- `crates/daw-ui/src/studio_export.rs` exercises the production audio decoder, Arrangement graph and WAV export for a reopened take. The test covers unity gain, both pan extremes, track solo, group gain, separate clip gain and stereo import.

## 2. Recorded audio evidence

The earlier 487,864-frame AIR recording and the owner's centered copy were read with FFmpeg. The earlier recording gives these results in dBFS:

| Condition | Peak L | Peak R | RMS L | RMS R |
| --- | ---: | ---: | ---: | ---: |
| Original stereo at unity | −19.88 | −76.39 | −37.02 | −102.78 |
| Original left centered at unity | −19.88 | −19.88 | −37.02 | −37.02 |
| Original left centered with 14 dB of gain | −5.88 | −5.88 | −23.02 | −23.02 |

The matched-gain render preserves the original left level and makes the same sample available on the right. The separate gain render matches the levels of the copy the owner could hear. The second 317,246-frame AIR recording measured 0.00/−65.35 dBFS peak and −13.21/−88.45 dBFS RMS. Its left signal was substantially louder and the channel imbalance was still present. The existing recordings and derivative were only read.

## 3. Verification

| Command or procedure | Result |
| --- | --- |
| `cargo test -p daw-core --quiet` | 29 passed. |
| `cargo test -p daw-core recording_input_persistence_tests` | 2 passed. |
| `cargo test -p daw-core old_projects_remain_compatible_without_effects_or_click_metadata` | 1 passed. |
| `cargo test -p daw-ui reopened_take_keeps_routing_and_gain_independent_through_track_and_master` | 1 passed. After reopen, the soloed mono fixture reached each master channel at the original left amplitude. Left and right pan each isolated the corresponding output. A separate +6.02 dB clip gain was reflected in playback and export, and the source sample remained at unity. The imported stereo fixture retained its different left and right samples. |
| `cargo fmt --all -- --check` and `git diff --check` | Passed at the time of the focused test. |
| FFmpeg float PCM decode, followed by per-channel peak and RMS calculation | Compared the original stereo, `pan=stereo\|c0=c0\|c1=c0`, and that same pan followed by `volume=14dB`. |

## 4. Limits

The figures are digital sample measurements. This pass did not independently operate the native desktop or listen to the hardware output. The owner's audible-copy result combined two changes, so it does not establish whether either change alone would satisfy the owner.

## 5. Remaining acceptance

Capture a new ten-second AIR 192|6 take, solo it at neutral pan and confirm listening at the headphones. Save, reopen, replay and export the same project. Report desktop interaction and owner listening separately.

## 6. Git and files

Work began on `beta/integration` at `bf52dee0003b`. This pass changed the two core files and the export test file listed above, plus this report. No commit or push was made by this agent. The test fixtures are generated in the temporary directory and removed on a passing run. The report contains aggregate sample values without personal audio.

## 7. Handoff

Use the saved track input channel to select capture. Keep audio clip rendering based on the source file's channels. This preserves stereo imports and makes mono takes center at neutral pan through the existing decoder and mixer. The integration handoff tracks native and owner acceptance.
