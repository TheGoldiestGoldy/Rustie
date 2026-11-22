# Rusty development integration · 21 November 2026

Verified source `028e2b1a8cd530af99fcc5e0910a930b075e1167`. This build is a development preview. See the [Beta 1 release decision](BETA1_RELEASE_DECISION.md).

- Shift-click Arrangement clips to move, copy, paste, duplicate, split or delete them together. Each batch forms one undo step. Session clips continue to use their own slots.
- Audio tracks remember the recording device by its PulseAudio source ID. An unavailable saved source requires a new selection. Older saved device labels are accepted when they uniquely identify an input.
- Devices → Instrument accepts local General MIDI SoundFont banks, with preset selection, a mouse keyboard and computer keys while the instrument tab is open. Tracks without a SoundFont assignment play the existing sine instrument. Bring your own bank.
- Isolated live instruments receive note channel, sustain and pitch bend where the plug-in supports them. Explicit Save binds the loaded VST3 class and captures native state. Arrangement export prompts for bypassed live inserts and printed audio when the mix needs those parts.
- The locked Linux build requires Rust 1.89 or later. The matched desktop UI and worker were built and checked with Rust 1.100.0 in an isolated profile.
