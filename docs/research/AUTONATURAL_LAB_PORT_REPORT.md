# Autonatural Lab current-Rustie port report

**Date:** 2026-11-21
**Lab:** `/home/developer/Projects/rustie-autonatural-lab/`
**Main source base:** `f578b9a44509ac54aebf2f052478c7c593af64d1` plus the main repository's uncommitted and untracked working-tree contents present at sync time.

## Changes

- Preserved a complete pre-port backup at `/home/developer/Projects/rustie-autonatural-lab-backup-20261121-074038/`.
- Copied the current main Rustie working-tree source into the isolated Lab, excluding the Lab build target, pinned Autonatural binaries, launcher, Lab-specific documentation, and Lab `AGENTS.md`.
- Confirmed representative engine and project-model files are byte-identical between main and Lab after the copy.
- Ported the offline Autonatural integration into the current `Studio` structure: dedicated state, render polling, top panel controls, new tuned-track import, timeline offset alignment, and A/B mute switching.
- Kept Autonatural as a separate process using the pinned `bin/nt`; did not claim CLAP hosting.
- Added `--lab-smoke` to the current DAW entry point.
- Kept project saves and autosaves inside `~/.local/share/autonatural-lab/`; external projects open read-only and must be saved as a Lab copy. Canonical-path and symlink checks protect project writes.
- Updated the window title to `Rustie Autonatural Lab | Alpha v0.1.0`, changed the in-app brand header, and updated the application-menu label.
- Retained separate XDG config/data/cache directories in the launcher.

## Verification

- `cargo test --workspace`: 90 passed, 0 failed (19 core, 36 engine, 3 real-time/no-allocation, 3 plugin-host, 29 UI/Lab).
- `cargo clippy --workspace --all-targets -- -D warnings`: passed.
- `cargo build --workspace --release`: passed.
- `cargo fmt --all -- --check`: passed.
- End-to-end `--lab-smoke` using `Autonatural/eval/fixtures/demo_vocal_like_48k.wav` at start offsets 0 and 1 second: both passed. Each rendered 96,000 stereo frames at 48 kHz, decoded through the current Rustie engine, and confirmed the input bytes were unchanged.
- Launched through `gtk-launch rustie-autonatural-lab-alpha`. PID 88975 resolved through `/proc` to `/home/developer/Projects/rustie-autonatural-lab/target/release/daw-ui`. The executable is not the main Rustie binary.
- Release executable SHA-256: `718c2c6be4b5cba579ee552f3fc8a60fc9f8bef423ac52ba217d0b83b568b4af`.
- Main Rustie HEAD remained `f578b9a44509ac54aebf2f052478c7c593af64d1`; its working-tree status count remained 66. No main-repo files were edited by this port.

## Not verified

- Manual UI operation, actual device playback, and headphone A/B listening.
- Human-vocal naturalness and full Autonatural G1-G11 quality gates.
- Live Autonatural CLAP plugin hosting. The CLAP bundle is included for external hosts only.
- Long-session stability and all user workflows on physical audio hardware.
