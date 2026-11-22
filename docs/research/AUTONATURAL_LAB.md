# Rustie Autonatural Lab Alpha

**Updated:** 21 November 2026
**Lab directory:** `~/Projects/rustie-autonatural-lab/`
**Main Rustie source:** `~/Projects/rust-daw/` (not modified by this port)
**Launcher:** Applications → **RUSTIE AUTONATURAL LAB • ISOLATED ALPHA v0.1.0**
**Command:** `~/Projects/rustie-autonatural-lab/scripts/start-autonatural-lab.sh`

This Lab now uses the current Rustie working-tree source snapshot copied from the main repository on 21 November 2026. Its Git base revision was `f578b9a44509ac54aebf2f052478c7c593af64d1`; the source copy also includes the uncommitted and untracked work that was present in the main tree at copy time. The Lab has its own executable, build target, settings/cache/data directories, project directory, and Autonatural CLI.

Autonatural remains an **experimental offline alpha**, not production-ready live pitch correction. It runs the pinned Rust DSP CLI as a separate process. The bundled `bin/Autonatural.clap` is for external CLAP hosts; this Lab does not load it as a plugin. Rustie's current plugin-host work is included, but this Autonatural workflow is still an offline render pipeline.

## First vocal test

1. Launch the Lab from the application menu. The window title identifies **Rustie Autonatural Lab** and its alpha version.
2. Import a dry solo vocal using **Import vocal…** or the DAW's audio import. WAV is recommended; FFmpeg-readable FLAC/MP3 and other formats are converted for rendering.
3. Select the audio clip. Set the actual musical key/scale if known. `chromatic` is the safe fallback, not automatic key detection.
4. Start with Tightness 50, a 30-second preview, and Start at 0 seconds.
5. Press **Render selected vocal**. Rendering happens in a worker thread and separate process. Cancel is available during the DSP stage; the render has a five-minute timeout.
6. On success, a new tuned audio track is added at the preview's corresponding timeline position. The source audio is not rewritten.
7. Press Play and compare **A · Original** and **B · Tuned**. The buttons switch track mute states through the Rustie mixer.
8. Adjust Tightness or preview position and render again. The prior preview track is removed from the project when replaced; the rendered file remains in the Lab renders folder.
9. Choose Full take for longer material. Save projects into the Lab's private project folder.

For meaningful A/B listening, level-match the two tracks. Correction can change peaks and perceived loudness. This is not automatic key detection, and the quality of the DSP has not yet passed comprehensive human-vocal quality gates.

## Isolation and reversibility

- Main Rustie source and binary were not edited or replaced. The Lab is a separate copy.
- The desktop launcher resolves to the Lab release executable and sets separate XDG config/data/cache directories.
- Autonatural output and temporary project data live under `~/.local/share/autonatural-lab/`.
- Lab project saves and autosaves are restricted to the Lab data directory. Projects opened from elsewhere are treated as read-only; use Save As to create a Lab copy. Canonical path and symlink checks reject project-save escapes.
- Original vocal files are read-only inputs. FFmpeg creates a separate temporary WAV, and the pinned `bin/nt` writes a new render.
- The DSP runner is a pinned copy, not the mutable Autonatural development executable.
- The renderer runs out of process, but this is not an OS security sandbox. FFmpeg conversion cancellation is not exposed in the UI.
- The standard Rustie audio backend selection is retained. No system audio configuration or installed main Rustie build was changed.
- A timestamped backup of the pre-port Lab directory was kept alongside the Lab folder.

## Verification on 21 November 2026

- `cargo test --workspace`: **90 passed, 0 failed** (19 core, 36 engine, 3 real-time/no-allocation, 3 plugin-host, 29 UI/Lab; other crates have no tests).
- `cargo clippy --workspace --all-targets -- -D warnings`: passed.
- `cargo build --workspace --release`: passed.
- `cargo fmt --all`: run after source changes; a final `--check` is part of packaging verification.
- End-to-end `--lab-smoke` using `Autonatural/eval/fixtures/demo_vocal_like_48k.wav`: passed. It decoded 96,000 stereo frames at 48 kHz (two seconds) and verified the input file bytes were unchanged.
- Desktop entry was launched with `gtk-launch`; `/proc` confirmed the new process executable is the Lab's `target/release/daw-ui`, not main Rustie.
- The release executable SHA-256 at build time: `718c2c6be4b5cba579ee552f3fc8a60fc9f8bef423ac52ba217d0b83b568b4af` (recompute after any subsequent rebuild).

## Still unverified / limitations

- Actual user-operated GUI workflow, audio playback, and headphone A/B listening on this updated build.
- Real sung-vocal evaluation across phoneme boundaries, formant preservation, vibrato, sibilants, and varied registers. Synthetic fixtures are not evidence of natural-sounding results.
- Comprehensive Autonatural quality gates G1-G11 and reliable automatic key detection.
- Real-time Autonatural CLAP hosting inside Rustie. The bundled CLAP file is not loaded by this Lab.
- Measured end-to-end audio latency, long-session stability, and crash-recovery behavior under malformed real-world projects/audio.

Treat this as an experimental build. Keep backups of sessions and source audio, as with any alpha DAW.
