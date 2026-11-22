# Agent report: Autonatural Lab alpha, 17 November 2026

> **Status:** Partially functional, source-integrated experimental offline alpha.
> **Workspace:** `/home/developer/Projects/rustie-autonatural-lab`.
> **Purpose:** A separate, safe-to-test copy of Rustie to preview the Autonatural pitch-correction DSP on vocal recordings.

## 1. What I implemented

| Area | Changed files | What actually works |
| --- | --- | --- |
| Isolated Rustie build | Copied `~/Projects/rust-daw` to `~/Projects/rustie-autonatural-lab` excluding target/Git; built separate release `daw-ui` | Main DAW working tree and executable untouched |
| New UI | `crates/daw-ui/src/{main.rs,studio.rs,studio_views.rs,studio_autonatural.rs}` | Named Lab window, no demo synth session, control strip with key, strength, start, length, render/cancel, original/tuned A/B |
| DSP processing | `studio_autonatural.rs` | Calls pinned Rust DSP CLI executable `bin/nt` in a separate OS process after FFmpeg conversion; output WAV imported to a new track |
| A/B | `studio_autonatural.rs` | Mute/unmute original/tuned track via existing mixer gain updates, seek to preview start after rendering |
| Safety | `studio_autonatural.rs`, `studio.rs` | Source not written; independent output; DSP cancellation + timeout, project-save canonical path guard and symlink escape refusal |
| Packaging | `scripts/start-autonatural-lab.sh`, `packaging/autonatural-lab.desktop`, `bin/nt`, `bin/Autonatural.clap` | Fixed native engine, separate XDG config; desktop shortcut installed |
| Documentation | `README-AUTONATURAL-LAB.md`, `docs/ALPHA_FEATURE_MATRIX.md`, `AGENTS.md`, this report | Instructions and evidence |

**Important distinction:** this integration runs the same offline Autonatural algorithm, **not the CLAP plugin inside the Rustie plugin host**. Actual CLAP hosting is absent from Rustie. The CLAP binary is included for other hosts and is not loaded by the Lab.

## 2. Current state and screenshots

- Isolated Lab release binary exists at `target/release/daw-ui`.
- An earlier isolated GUI build survived a timed 12-second launch with no logged crash. The title capture attempt failed because the screenshot utility could not address the Wayland window.
- **No final GUI screenshot / mouse-click visual inspection.** Do not substitute the existing Rustie screenshots for a Lab screenshot.
- User should inspect Lab strip and A/B button operations on their own PC.

## 3. Verification

- `cargo fmt --all -- --check`: passed.
- `cargo test --workspace`: **44 tests** (14 core, 20 engine, 2 real-time/no-allocation, 8 UI/lab); all passed. Other crates have zero tests.
- `cargo clippy --workspace --all-targets -- -D warnings`: passed.
- `cargo build --release -p daw-ui`: passed (isolated target).
- Headless end-to-end `--lab-smoke` with synthetic 48 kHz fixture: **passed** with start 0 s and 1 s. The renderer output 96,000 stereo frames for the two-second preview, decoded by Rustie, and the original input SHA-256 stayed unchanged.
- 30-second DSP synthetic benchmark ~0.69 seconds wall time; RSS ~39 MB, on current workstation.
- CLAP plugin independently validated in previous Autonatural pass: validator 0.4.1 showed 34 pass, 10 skip, 0 fail. CLAP not hosted inside Lab.
- **Not tested:** measured speaker/headphone sound, subjective quality, GUI A/B mouse interaction, full-length human vocal processing, crash recovery under malformed real files or all user workflows.

## 4. Known issues/limitations

- No proven naturalness, formant invariance or full DSP G1-G11 quality gates.
- Pitch correction can select the wrong chromatic neighbor; user must set actual key. No automatic key detector, pitch roll or manual region overrides.
- Audio files are initially decoded fully into memory by Rustie. No recording, input monitoring, host-side CLAP chain or true live correction.
- Render jobs may take minutes on very long/difficult files; while the DSP process has 5-minute timeout and cancellation, FFmpeg conversion is currently blocking on the background thread.
- A/B track alignment is computed from timeline tempo and chosen integer seconds; not sample-accurate for complex tempo maps or selected non-first clip in multi-clip tracks. Use fresh one-clip vocal imports for initial testing.
- Listener needs to match original and tuned perceived loudness for meaningful evaluation.
- Lab save destinations are constrained; external projects can be opened for review but not overwritten.
- Subprocess separation reduces crash coupling but is **not** a security sandbox against malicious audio input or resource exhaustion.

## 5. Next priorities

1. **First user test:** import a dry solo vocal, set musical key, render a 15/30-second excerpt, A/B while listening. Record exact timestamps for pitch misreads, clicks, vibrato flattening, formant/voice quality and phrasal transitions.
2. Extend real-vocal and synthetic evaluation with F0 confidence, voiced/noisy segmentation, click detection and vibrato correlations. Improve TD-PSOLA and region planning based on measured failure cases; rerender A/B.
3. Instrument and verify native Lab UI clicks, low-latency audio monitoring, and valid save/reopen with a real project; add actual GUI screenshot.
4. Only after quality passes: evaluate `clack-host` as a proper CLAP hosting backend and 80-200 ms live Natural mode.

## 6. Git, files and artifacts

- Base Rustie branch at copy time: `master`; base tree contained uncommitted work by other agents. Copied source as a **snapshot** using rsync excluding .git and target.
- Lab copy has **no separate Git repository**, so no lab commit/branch/push; independent provenance is this report.
- Main Rustie working tree and compiled binary unchanged. No GitHub commit/push.
- Local installed shortcut: `~/.local/share/applications/autonatural-lab.desktop`.
- Private generated media: `~/.local/share/autonatural-lab/smoke/` (synthetic only), future real vocal outputs under `~/.local/share/autonatural-lab/renders/` (excluded from release archive).
- Release ZIP: `~/Downloads/Rustie-Autonatural-Lab-alpha-2026-11-17.zip`.
- No personal raw recordings bundled or published.

## 7. Next-agent handoff

- Read `README-AUTONATURAL-LAB.md`, `docs/ALPHA_FEATURE_MATRIX.md`, `crates/daw-ui/src/studio_autonatural.rs`, pinned Autonatural DSP `~/Projects/Autonatural/spec/DSP_ALGORITHM.md` and `docs/PLUGIN_VALIDATION.md`.
- Do not edit `~/Projects/rust-daw`; it has parallel development, including DAW audio startup latency defects. Work in the Lab snapshot.
- Always keep the original recordings, save paths and initial audition session isolated. Check the process task after cancellation.
- Rerun tests/Clippy/release build, and `AUTONATURAL_LAB_SMOKE_SOURCE=... ./scripts/start-autonatural-lab.sh --lab-smoke` before proposing a new alpha release.
- Write the next per-agent report with what changed, specific measurable audio tests, current known failures and recommendations. No subjective claims without user listening.
