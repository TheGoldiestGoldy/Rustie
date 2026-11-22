# SoundFont built-in instrument engine (rustysynth)

**Date:** 2026-11-21 · **Branch:** `beta/integration` · **Author:** lead (agent)

## 1. What I implemented

A working end-to-end built-in instrument rack powered by `rustysynth` (SoundFont `.sf2`
playback), with an Ableton-style preset browser and a playable on-screen/computer keyboard.

**Engine (`daw-engine`)**
- Added `rustysynth = "1.3.8"` to `crates/daw-engine/Cargo.toml`.
- New `crates/daw-engine/src/instruments.rs`:
  - `Instrument` trait (Send + Debug) for future samplers/synths.
  - `SoundFontInstrument` wrapping `rustysynth::Synthesizer`, rendering stereo per sample.
  - `SoundFontBank` (`Arc<SoundFont>`), `load_sound_font`, `build_synth`, `build_synth_with_tone`,
    `SoundFontTone`, `apply_tone`.
  - `SynthUpdate` + `SoundFontHandle`: lock-free `rtrb` ring used to swap a fully-initialized
    synthesizer into the audio thread (no graph rebuild, no allocation/lock/IO in the callback).
- `graph.rs`: new `Source::SoundFont` variant; `note_on/note_off/all_notes_off/pitch_bend/
  control_change/program_change` dispatch to the boxed instrument; MIDI/audio routing tests.
- `engine.rs`: new `EngineCommand::{PitchBend, ControlChange, ProgramChange}` routed through the
  existing lock-free command ring.

**Core model (`daw-core`)**
- New `crates/daw-core/src/instruments.rs`: `InstrumentCategory` (Pianos & Keys, Bass,
  Strings & Brass, Classic Synths, Drums & Percussion), `InstrumentPreset`, a 23-preset
  General MIDI catalog, `resolve_preset`, `BuiltinInstrument`.
- `model.rs`: `Track` gains an optional persisted `instrument: Option<BuiltinInstrument>` with
  serde defaults (old projects load as `None` → legacy sine synth).

**UI (`daw-ui`)**
- `studio.rs`: SoundFont library state, background `.sf2` loading (mpsc → UI → rtrb → audio),
  `compiled_graph_with_live` embeds `SoundFontInstrument` for MIDI tracks when a bank is loaded,
  fallback to sine when not.
- New `studio_instruments.rs`: bank discovery/loading, `assign_instrument` (live lock-free swap),
  `start_monitor_driver`/`ensure_monitor`/`trigger_note`, tone controls (CC7/10/1/64/91/93),
  on-screen mini-keyboard, computer-keyboard triggering (Z X C V B N M , . / white · A D G H J L ;
  black · [ ] octave), browser rack UI, `--probe-soundfont` helper.
- `studio_devices.rs`: new **Instrument** tab in the Devices window.
- `studio_browser.rs`: Instruments section now shows the categorized rack; clicking a preset
  assigns it to the selected MIDI track (or creates one).
- `studio_midi_input.rs`: hardware MIDI now decodes pitch bend, control change, and program change.
- `main.rs`: `--probe-soundfont PATH.sf2` headless verification flag.

**Assets**
- `assets/soundfonts/README.md`: open-license bank list (GeneralUser GS, FluidR3 GM, Salamander,
  Musyng Kite), GM mapping, license reminder. No bundled SoundFont data.

## 2. Current state / screenshots

No UI screenshot captured this pass (no headless screenshot harness was available); the change is
functional code, not a visual redesign. The running GUI launched cleanly for 7 seconds with no
stderr. The preset selector, tone sliders, and mini-keyboard are new controls in the existing
Devices → **Instrument** tab and the Library → **Instruments** browser.

## 3. Verification

- `cargo check --workspace` — **pass**.
- `cargo test --workspace` — **139 passed, 0 failed** (28 daw-core, 42 daw-engine, 6 rt-no-alloc,
  3 daw-plugin-host, 60 daw-ui).
- `cargo clippy --workspace --all-targets -- -D warnings` — **pass** (0 warnings).
- `cargo fmt --all -- --check` — **pass**; `git diff --check` — **clean**.
- `cargo build --release -p daw-ui` — **pass**; `./target/release/daw-ui --version` prints
  `Rusty v0.1.0-dev`.
- **Real SoundFont audio path verified headlessly** with GeneralUser GS v1.471:
  `./target/release/daw-ui --probe-soundfont /tmp/.../v1.471.sf2` → `peak=0.100986` (non-silent
  Acoustic Piano render, 0.036s load). This proves parse → synth → note → render reaches
  non-silent PCM, not speaker output.
- GUI smoke: `timeout 7 ./target/release/daw-ui` stayed alive (exit 124) with empty log.

**Not tested:** physical speaker output, on-screen-keyboard mouse feel, computer-keyboard hold
behavior in the running GUI, and all 23 presets' timbres (only Acoustic Piano program 0 was
probe-rendered). The build machine currently has no `.sf2` in the search paths, so the running
app shows the "No SoundFont bank installed" fallback until a bank is dropped in.

## 4. Known issues and honest limitations

- Real GM timbre fidelity depends on the user's chosen bank; the rack is GM-mapped, so a
  non-GM bank (e.g. Salamander piano only) will sound wrong on non-piano presets.
- `rustysynth` 1.3.8 does not expose runtime ADSR/filter editing, so the rack exposes the tone
  controls it honors (volume/pan/modulation/sustain/reverb/chorus) rather than a fake ADSR UI.
- Preset changes swap the synth and drop currently-held notes on that instrument (documented
  behavior); sustain-pedal state is not carried across a swap.
- The demo starter tracks are still the legacy sine synth unless a bank is present; with a bank,
  unassigned MIDI tracks resolve to the default Acoustic Piano preset.

## 5. What remains and recommended next priorities

1. Drop `GeneralUser GS` (or FluidR3 GM) into `assets/soundfonts/`, relaunch, and confirm each
   category through speakers using the Devices → Instrument mini-keyboard.
2. Human test computer-keyboard hold/release and mouse mini-keyboard feel.
3. Persist the per-track tone profile in the project model if desired.
4. Consider a compact built-in synth (non-SoundFont) for the "Classic Synths" category with real
   ADSR/filter, which `rustysynth` cannot provide.

## 6. Git and files

Branch `beta/integration`; **nothing committed or pushed.**

Modified: `Cargo.lock`, `crates/daw-core/src/lib.rs`, `crates/daw-core/src/model.rs`,
`crates/daw-engine/Cargo.toml`, `crates/daw-engine/src/engine.rs`,
`crates/daw-engine/src/graph.rs`, `crates/daw-engine/src/lib.rs`, `crates/daw-ui/src/main.rs`,
`crates/daw-ui/src/studio.rs`, `crates/daw-ui/src/studio_browser.rs`,
`crates/daw-ui/src/studio_devices.rs`, `crates/daw-ui/src/studio_midi_input.rs`.

New: `crates/daw-core/src/instruments.rs`, `crates/daw-engine/src/instruments.rs`,
`crates/daw-ui/src/studio_instruments.rs`, `assets/soundfonts/README.md`.

## 7. Instructions for the next agent

- To test immediately: place a GM `.sf2` in `assets/soundfonts/`, rebuild
  `cargo build --release -p daw-ui`, run it, open **Devices → Instrument**, pick a preset, and
  play the mini-keyboard or computer keys. Headless: `--probe-soundfont PATH`.
- The `SoundFontHandle`/`SynthUpdate` lock-free swap is per-node; `assign_instrument` rebuilds
  the graph only when transitioning from the sine fallback (first bank load) — keep that
  distinction if extending preset switching.
- Do not commit or push unless the owner authorizes it.
