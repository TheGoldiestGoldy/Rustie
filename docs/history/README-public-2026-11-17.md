# Rustie / Slate Studio
### Native Rust digital audio workstation • early developer preview

> **Status, 17 November 2026:** An experimental Linux desktop DAW built as a Rust workspace. It can display/edit an arrangement, produce MIDI demo audio and play imported audio through native Linux output backends. It is **not yet a production-ready DAW**. No VST3/CLAP hosting, external MIDI input, audio recording, Ableton project import, or fully functional Session clip launching exists in the checked-in baseline.

This repository documents and develops **Slate Studio** (working product name), a low-latency, native-first DAW for Linux, with a dense arrangement workflow, music-theory-aware MIDI editor and two complementary workflows: **Arrangement** (linear timeline) and **Session** (clips/scenes for live performance). The design direction is near-black, glass-inspired, minimal and practical. General DAW patterns may be learned from existing software; proprietary art, branding and source are not copied.

**Evidence boundary:** The committed baseline inspected for this documentation is `f578b9a` (2026-11-17). Other agents are concurrently implementing the browser and UI; their uncommitted changes are deliberately **not** represented here as merged features. See [Project status](../PROJECT_STATUS.md).

## Quick start (Linux)

Requires Rust stable supporting edition 2024 (**rustc >= 1.85**), Cargo, a desktop environment, and system development libraries for ALSA/JACK/PulseAudio and GUI display. On Arch/CachyOS, start with:

```bash
sudo pacman -S --needed base-devel rustup pkgconf alsa-lib libpulse libxkbcommon wayland libx11
rustup default stable
git clone https://github.com/TheGoldiestGoldy/Rustie.git slate-studio
cd slate-studio
cargo build --release -p daw-ui
cargo run --release -p daw-ui
```

For JACK development headers use an appropriate provider for your existing sound stack (for example PipeWire-JACK or standalone JACK), **not both**. Package names and requirements vary by distro; use `cargo build` diagnostics rather than replacing your audio stack. Do not copy any user's home directory into the project.

The repository-local launcher also works after a build: `./scripts/start-slate-studio.sh`. Linux is the only currently tested OS; Windows and macOS are not claimed supported.

### Test and lint

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

**Audio diagnostics:** `cargo run -p daw-engine --bin daw-audio-probe` lists output devices. `-- --silent 48000 pulse` opens a silent native output stream, and `-- --tone 48000 pulse` generates a short low-volume tone. Backend alternatives: `alsa` and `jack`. **A nonzero generated peak does not establish audible speaker output.** Do not alter a system-wide sound profile during calls/recordings. See [Audio notes](../AUDIO_ENGINE.md).

## Feature status

| Area | Checked-in baseline | Status |
| --- | --- | --- |
| Native Linux GUI | `eframe` / `egui` with transport, arrangement and detail panel | Prototype |
| Arrangement | Track headers, editable audio/MIDI clips, drag repositioning, selection and seek ruler | Prototype |
| Session / live grid | Grid overview and clip selection | **No independent clip launching yet** |
| MIDI editor | Piano-roll note editing, note-grid choices, root/mode, scale highlighting/fold | Prototype |
| Sound engine | CPAL output via native ALSA / PulseAudio / JACK, simple built-in 16-voice sine synth | Functional foundation |
| Audio clips | Decode WAV/FLAC/MP3/OGG using Symphonia; in-RAM stereo sample buffers | Prototype |
| Mixing | Gain/pan/mute/solo control, graph sends/returns and declared-latency PDC primitives | Partial |
| Save/open | JSON project with referenced absolute audio paths | Prototype; not portable bundles |
| Sample/plugin browser | Active concurrent UI work; baseline feature is not yet merged | In progress |
| CLAP / VST3 / LV2 hosting | Discovery concept only; plugin-host and IPC crates are placeholders | Not implemented |
| Live recording / external MIDI | No working input path | Not implemented |
| Ableton Live .als import/export | Parser crate placeholder; format handling not implemented | Not implemented |
| Warping, automation lanes, true clip-launch quantization, export mix | Future work | Not implemented |

Do not confuse a backend test, UI control, or a crate name with complete user-visible functionality.

## Repository map

```text
crates/
  daw-core/           Music time (960 PPQ), tempo/scales, project model, edit commands
  daw-engine/         Audio graph, MIDI sine voice, transport, CPAL driver, decoder, RT tests
  daw-ui/             Native desktop application (Arrangement, Session overview, piano editor)
  daw-parser/         Placeholder for project interchange / Ableton .als experiments
  daw-host-client/    Placeholder for sandboxed-plugin IPC
  daw-plugin-host/    Placeholder plugin worker executable
docs/                 Architecture, feature matrix, product spec, bugs, decisions, tests
scripts/              Local developer launch and maintenance helpers
.github/              CI, issue templates, pull request conventions
```

Read [Codebase map](../CODEBASE_MAP.md), [Architecture](../ARCHITECTURE.md), [Roadmap](../ROADMAP.md), and [Developer guide](../DEVELOPMENT.md).

## Product direction

* **Arrangement:** serious horizontal timeline with ruler, looping, snapping, clip editing, track groups, automation, meters and an integrated mixer.
* **Session:** launchable clip slots/scenes with quantization, stop buttons, recording state and shared track/device model. Displaying a grid is phase one, **playback semantics come later**.
* **Browser:** left dock with searchable sections for sounds, samples, locations, instruments, effects and plugins. Installed plugin entries remain explicitly disabled until real hosting works.
* **Piano roll:** mouse-friendly note creation/resize/move/velocity; root/scale picker, highlight, fold and optional scale-lock. Do not silently retune the user's notes.
* **Visuals:** charcoal/near-black surfaces, subtle translucent layers, simple high-contrast controls; no direct copying of Ableton's logos, UI artwork or branded language.
* **Performance:** audio callback must not allocate, lock, block on disk, call D-Bus or rebuild graphs. Lower latency must be demonstrated with measured output timings, not marketing.

See the detailed [UI/UX specification](../UI_UX_SPEC.md).

## Important current limitation: playback timing

The initial hands-on test reported a pronounced difference between the visible transport/playhead and audible MIDI playback. The Nov 17 commit introduced native ALSA preference and playback scheduling changes, but **this issue needs retesting on actual speakers**. A device callback prediction and generated output amplitude are not round-trip latency measurements. Known issues and reproduction steps: [docs/KNOWN_ISSUES.md](../KNOWN_ISSUES.md).

## Collaboration and agent contributions

Multiple coding agents may work concurrently. Do **not** force push, edit the same file in parallel, or declare a feature complete from a screenshot. Work in separate branches/worktrees, identify baseline commit, run relevant checks, and submit small PRs with proof. The canonical handoff template is in [docs/AGENT_HANDOFF.md](../AGENT_HANDOFF.md). See [CONTRIBUTING.md](../../CONTRIBUTING.md).

## Safety, IP and privacy

This project targets **original implementation inspired by generic DAW interaction patterns**. Do not commit proprietary assets, commercial plugin binaries, purchased sample packs, vocals/sessions, machine-specific paths, credentials, telemetry containing usernames, or personal audio. This repository is currently public, so audit each change before publishing. See [SECURITY.md](../../SECURITY.md) and [docs/LEGAL_AND_ASSETS.md](../LEGAL_AND_ASSETS.md).

The Cargo manifests declare `MIT OR Apache-2.0`; standard license texts accompany the source. Review contributor rights and third-party notices before distributing binaries. No affiliation with Ableton or other DAW vendors is claimed.

## Documentation index

| Document | Purpose |
| --- | --- |
| [PROJECT_STATUS](../PROJECT_STATUS.md) | Verified features vs work in progress |
| [CODEBASE_MAP](../CODEBASE_MAP.md) | Crate ownership and important implementation files |
| [AUDIO_ENGINE](../AUDIO_ENGINE.md) | Thread model, backends, device caveats and timing |
| [UI_UX_SPEC](../UI_UX_SPEC.md) | Product/design acceptance criteria |
| [PROJECT_FORMAT](../PROJECT_FORMAT.md) | JSON persistence and interoperability boundaries |
| [ROADMAP](../ROADMAP.md) | Priorities and exit conditions |
| [KNOWN_ISSUES](../KNOWN_ISSUES.md) | Reproduction procedures and blockers |
| [TEST_MATRIX](../TEST_MATRIX.md) | Verifications and missing measurements |
| [AGENT_HANDOFF](../AGENT_HANDOFF.md) | Safe multi-agent integration |
| [LEGAL_AND_ASSETS](../LEGAL_AND_ASSETS.md) | Licensing, trademark, sample and plugin rules |
| [DEVELOPMENT](../DEVELOPMENT.md) | Setup and contribution conventions |
| [CHANGELOG](../../CHANGELOG.md) | Historical baseline, not fabricated release notes |

**Maintainer note:** This document describes a technical snapshot, not a release or product guarantee.
