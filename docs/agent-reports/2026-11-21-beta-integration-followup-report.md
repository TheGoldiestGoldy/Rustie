# Rusty Beta 1 integration follow-up

**21 November 2026 · NO-GO**

- Host and worktree `the Linux test workstation` · `/home/developer/Projects/rust-daw-agent-integration` · `beta/integration`.
- Coordinator baseline `1ae60bb2106b595e7e46c33911753ee45eb6d6f6`. The completed Wave 1 checkpoint was `e6b82a79feeac4158f1918b3f5598916d5bb177c`.
- Entry source `1b717f971ddfe5c9534ea117d77f89e49de3cba8`. Verified source `028e2b1a8cd530af99fcc5e0910a930b075e1167`.
- Release decision and full roadmap evidence are in the [Beta 1 gate table](../BETA1_RELEASE_DECISION.md).

## Merged history

| Change | Integration commit | Specialist parent or source |
|---|---|---|
| Owner-approved SoundFont work | `f6943e1e3a6ce35b6cf433c741bc9fe5237569cb` | Inherited worktree snapshot. Tracked patch, archive and tree ID matched the original before advancing the canonical branch. |
| Plug-in hardening | `741906aa7a1726a71aac8ffea6b8778f66bdde12` | `0ed065ad6938526a0085dc0e52321acdb0b9c353`. Combined external worker and SoundFont note, sustain and pitch routes. |
| Session re-verification | `5370eb1383f7fae73ee09742f53c988fca0b5cc2` | `3dd5fc9f55ae2eff5bd4b92a3250fe2c6a32c0ad`. Committed report only. Two local document deletions in the Session worktree were excluded. |
| Recording follow-up | `bec5b8b7ff49bb9dddc32845434cc1903f223526` | `385a94cccf90d0fcc1f376d29d1ab19125fd1669`. Pinned capture, per-track source identity, legacy label recovery and the Arrangement clip-limit guard. |
| Editing follow-up | `3638ee068f14cba177f692b1518688b8adf1a30e` | `26a7e23c02cb77bf946f46bd53c0e49afb4c58c6`. Batch edit/history with standalone Session copy, paste, duplicate and delete. |

All current specialist tips are ancestors of the integrated source. The [original Wave 2 report](2026-11-21-beta-integration-report.md) records the earlier merges. Version 1 native-state envelopes are still readable. The VST3 startup packet now reports the actual class, and live instrument MIDI packets encode channel and event kind. UI and worker were built and launched from one revision. JSON gained optional SoundFont and input-ID fields. Old projects without a SoundFont assignment retain the sine source. A loaded legacy input label must uniquely identify the source before the app arms.

The SoundFont swap now returns retired synths to the UI thread for deallocation. A pending swap is consumed before the next MIDI event. Regression tests exercise a real two-program generated bank under the allocation and deallocation guard. This bank contains mathematical waveforms and is a test fixture. No external SoundFont is packaged. Computer keys are scoped to the Instrument editor and release the original note when the octave changes, the tab closes or focus moves.

## Native app and state

- [Batch Arrangement split at 1540×940](../screenshots/beta1-arrangement-batch-1540.png)
- [Session scene 2 playing at 1540×940](../screenshots/beta1-session-playing-1540.png)
- [Persisted FX edits at 980×640](../screenshots/beta1-fx-chain-980.png)
- [SoundFont instrument and mini keyboard](../screenshots/beta1-soundfont-1540.png)
- [Arrangement export guard](../screenshots/beta1-export-guard-1540.png)
- [Virtual recording in progress after ten minutes](../screenshots/beta1-virtual-recording-10min.png)
- [Take waveform after cold reopen](../screenshots/beta1-virtual-take-reopened.png)

The native test used Xwayland and a private PulseAudio server. Tutorial Skip, Library import, Save dialog, ruler seek, 127 BPM, 6/8, batch split and undo/redo were exercised with pointer and key input. The edited review project retained eight Session slots and ten Arrangement clips. Scene 2 looped all four cells on the Quarter grid. The 980×640 window saved a reordered chain with both reverbs and EQ3 bypassed. A cold start with an explicitly assigned non-default bank resolved the saved instrument. Computer-key Z, octave change and release produced a virtual peak of 0.02274. The mini piano produced 0.02795 and returned to within one 16-bit count of silence. The headless SoundFont probe peaked at 0.053004 with the generated two-waveform bank.

The native Save and cold Save of a test project containing three plug-ins produced identical project JSON. CLAP Surge XT saved 50,310 bytes. VST3 Surge XT saved 52,030 bytes and VST3 Surge XT Effects saved 1,150 bytes. The VST3 IDs on both inserts and state blobs were populated on the first Save. Choosing Arrangement export with enabled live inserts showed the bypass/printed-audio warning and created zero WAV files. A fresh project with three starter MIDI tracks and an imported 48 kHz stereo WAV saved through the native dialog and exported an 8.500-second stereo float mix at 44.1 kHz.

## Verification

Cargo ran with `CARGO_TARGET_DIR=/home/developer/.cache/rustie-target-integration`, four jobs, and `RUSTY_BUILD_REVISION=028e2b1a8cd5`. The integrated build used Rust 1.100.0. Raw command and probe logs (local QA artifact; not distributed) and isolated fixtures (local QA artifact; not distributed) are local QA artifacts.

| Command or procedure | Result |
|---|---|
| `cargo fmt --all -- --check` | PASS |
| `cargo test --workspace` | PASS, 154 tests including eight realtime allocation/deallocation checks |
| `cargo clippy --workspace --all-targets -- -D warnings` | PASS |
| `cargo build --release -p daw-ui -p daw-plugin-host` | PASS |
| `git diff 1ae60bb --check` | PASS |
| `scripts/install-rusty-local.sh`, `rusty --version`, `sha256sum -c SHA256SUMS` | PASS in a private profile. Both executables matched. The native UI used the matched installed worker. |
| `rusty --check-project PATH` | PASS for recognized legacy and freshly created projects. Missing WAV returned 2 with its missing path. |
| `rusty --probe-recording-writer` | PASS. 441,000 stereo float frames at 44.1 kHz. The transient was at expected frame 88,200 (zero-frame delta). |
| Pinned capture through the private PulseAudio source | PASS. The playback clock reached 10:15.20 with 205,490 callbacks. Capture retained 27,157,478 stereo frames at 44.1 kHz (615.815828 s), with zero input drops and a −20.0 dBFS peak. The saved take retained its source ID, displayed its waveform after cold reopen and exported to a 616.320317 s stereo float WAV. The Arrangement had scored material in the opening bars and the generated input signal ran for the whole take. |
| `RUSTIE_PROBE_RATE=RATE rusty --probe-transport BUFFER 1000` | FAIL for repeatable start reliability. At least one 1 s failure at every tested rate/buffer combination. Clock-matched null sinks ran at 44.1 and 48 kHz, with 64, 128 and 512 requested frames. |
| `RUSTIE_PROBE_RATE=RATE rusty --probe-transport 128 2500` | All four matching virtual probes eventually advanced. First Play was observed after 845.53–1,996.72 ms. Observed seek was 1.06–3.15 ms. These are callback timings. |
| AIR 192\|6 speakers, physical input and acoustic alignment | NOT TESTED. This run used generated media and private virtual audio. |
| Backend XRUN/scheduler deadline counts | NOT TESTED. The current probes expose callback progress, input loss and worker fallback counts. |

## Live plug-in compatibility

Commands were `--probe-live-clap`, `--probe-live-vst3`, `--probe-live-clap-instrument` and `--probe-live-vst3-instrument`, with `RUSTIE_SAMPLE_RATE` set to 44100 and 48000. Successful paced probes produced PCM in both runs at each rate and used the 512-frame bridge.

| Format/role | Installed plug-in | Class | 44.1 / 48 kHz |
|---|---|---|---|
| CLAP effect | LSP Limiter Stereo 1.0.38 | `in.lsp-plug.limiter_stereo` | 400 blocks, 0/0 dry fallback |
| VST3 effect | Surge XT Effects 1.3.4 | `ABCDEF019182FAEB566D624153465854` | 400 blocks, 0/0 dry fallback |
| CLAP instrument | Surge XT 1.3.4 | `org.surge-synth-team.surge-xt` | 550 blocks, 0/0 missed |
| VST3 instrument | Surge XT 1.3.4 | `ABCDEF019182FAEB566D624153675854` | 550 blocks, 0/0 missed |
| Installed LSP VST3 bundle | LSP plug-ins | Bundle metadata | FAIL. One retired ID maps to conflicting classes. Inspection returns 1. |

SIGKILL, SIGSTOP and SIGABRT were injected into the exact owned effect worker after activation, in both formats. All six workers were reaped in 1.22–1.31 s from probe start. CLAP remained at peak 0.15000 with 255–261 fallback blocks. VST3 remained at peak 0.15878 with 297–301 fallback blocks. The deliberate failure crossed the probe's fallback threshold and returned 2. Missing bundle and unknown class UI probes also returned 2. Both workers rejected a 262,145-byte native-state file, and the VST3 worker rejected a nonexistent explicit class. The workspace includes tests for malformed/wrong-identity state, queue saturation, and bounded recovery. The native save/reopen verification addressed active plug-ins. Long multi-slot external-instrument stress and physical MIDI remain open.

## Remaining release blockers

1. **PulseAudio startup.** Run `RUSTIE_PROBE_RATE=44100 rusty --probe-transport 128 1000` twice on a private 44.1 kHz PulseAudio Null Output. Repeat at 48 kHz with its own sink. Compare the cold and immediate retry timing and callback count with the steps in the comparison script (local QA artifact; not distributed). The previous `1b717f971ddf` build exhibited the same intermittent failure. The longer window confirms a delay where the complete probe succeeded. This leaves audible onset and device behavior open.
2. **Physical acceptance.** At the AIR 192|6 actual sink rate, listen to the starter synth and a stereo imported WAV, operate transport and Scene switches, and record at least ten seconds of voice or guitar with the owner. Save, cold reopen, listen to the take and export, and measure reference-transient alignment. Record hardware XRUN/deadline counts where available.
3. **Release notices.** Add complete license and dependency notices after the contributor-rights check. The locked Linux graph now declares its measured metadata floor of Rust 1.89. The Linux build itself was verified on 1.100.0. The local dependency metadata inventory includes names, versions and declared licenses. Locked Linux dependency metadata (local QA artifact; not distributed).
4. **Live mixdown.** Confirm the intended way to print the complete live plug-in chain with saved native state. The current guard prevents a misleading export. Further LSP VST3 support requires resolving the ambiguous retired-class metadata while preserving current-class binding.

The integration added no proprietary plug-in binaries, private projects or recordings. The new binary Git paths are test-only math-generated SoundFont data and synthetic-project app captures. A path scan and a credential-pattern scan found no additions in those categories. No public build, release artifact or push was made.
