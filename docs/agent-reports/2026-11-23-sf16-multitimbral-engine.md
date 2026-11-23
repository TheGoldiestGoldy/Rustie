# sf-16 multitimbral SoundFont engine

## Delivered

`daw-engine` now exposes `Sf16Instrument`: one `rustysynth::Synthesizer` using
all 16 native MIDI channels with a UI-readable shadow state for each part.
The engine supports per-part bank/program, 14-bit volume/pan/expression/mod
wheel, reverb/chorus sends, hold, pitch-bend range and coarse/fine tuning RPN
state, note-held counts, channel/omni/off listening, and non-destructive
mute/solo. The synth is built only from a preloaded `SoundFontBank`; render and
MIDI paths allocate neither heap memory nor locks.

The generic graph instrument interface is now channel-aware. The engine and
hardware-MIDI path preserve the originating channel for notes, CCs, program
changes, sustain and pitch bend, so an sf-16 graph source can route each event
to its intended part. Existing single-part SoundFont instruments retain their
old behavior through the default trait implementation.

## Verification

- `cargo test --workspace` — 167 passed, 2 opt-in hardware tests ignored.
- `cargo clippy --workspace --all-targets -- -D warnings` — passed.
- `cargo fmt --all -- --check` and `git diff --check` — passed.
- The new real-time allocation guard constructs `Sf16Instrument` from the
  generated two-program `.sf2` fixture, sends a note on MIDI channel 5, and
  renders non-silent stereo output with allocations disabled.

## Scope boundary

This is the sf-16 engine and DAW MIDI plumbing (milestone M1 foundation), not
the complete dedicated hardware-style editor described in the design document.
The existing Devices > Instrument editor remains the single-track rack until a
separate project/UI model for a 16-part sf-16 instance is added. No production
SoundFont was added, no installation was performed, and speaker output was not
tested.

[executed on device: Johnnysstation (1200896c-45a0-4cb2-8dfa-479e25d803a4)]