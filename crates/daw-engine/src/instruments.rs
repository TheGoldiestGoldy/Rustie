//! Built-in instrument engine.
//!
//! `SoundFontInstrument` wraps a `rustysynth::Synthesizer` and implements the
//! [`Instrument`] trait so future samplers/synths can plug into the same track
//! architecture. SoundFont (`.sf2`) files are parsed off the audio thread and
//! the resulting synthesizer is handed to the real-time render loop through a
//! lock-free SPSC ring (`rtrb`). The audio callback never allocates, locks,
//! touches the filesystem, or blocks.

use std::{fmt, path::Path, sync::Arc};

use rtrb::{Consumer, Producer, RingBuffer};

/// A parsed SoundFont bank. Shared by reference across every track instrument
/// that uses the bank, so loading one `.sf2` never duplicates its sample data.
pub type SoundFontBank = Arc<rustysynth::SoundFont>;

/// Number of pending synthesizer swaps tolerated before a preset change is
/// reported as dropped. The audio thread drains one slot per rendered sample,
/// so this ring is effectively always empty in practice.
pub const SYNTH_UPDATE_CAPACITY: usize = 4;

/// Common interface for a built-in instrument voice. Implementations must be
/// `Send` (they are rendered on the audio thread) and `Debug` (the audio graph
/// derives `Debug`).
///
/// `render` must write exactly `left.len()` samples into both buffers, which are
/// always equal length and are never longer than a single render block.
pub trait Instrument: Send + fmt::Debug {
    fn render(&mut self, left: &mut [f32], right: &mut [f32]);
    fn note_on(&mut self, note: u8, velocity: u8);
    fn note_off(&mut self, note: u8);
    /// 14-bit pitch-bend wheel value, 0..=16383 (8192 = center).
    fn pitch_bend(&mut self, value: u16);
    fn control_change(&mut self, controller: u8, value: u8);
    fn program_change(&mut self, program: u8);
    fn all_notes_off(&mut self);
    fn reset(&mut self);

    /// Deliver a complete channel MIDI message. The default implementation
    /// keeps the original single-part instrument contract: callers that do
    /// not care about channel routing continue to receive the same messages
    /// as before. Multitimbral instruments override this method.
    fn midi_message(&mut self, _channel: u8, command: u8, data1: u8, data2: u8) {
        match command & 0xF0 {
            0x80 => self.note_off(data1),
            0x90 if data2 == 0 => self.note_off(data1),
            0x90 => self.note_on(data1, data2),
            0xB0 => self.control_change(data1, data2),
            0xC0 => self.program_change(data1),
            0xE0 => self.pitch_bend(u16::from(data1) | (u16::from(data2) << 7)),
            _ => {}
        }
    }
}

/// Number of independently addressable parts in the sf-16 engine.
pub const SF16_PARTS: usize = 16;

/// MIDI input mapping for a sf-16 part. A part normally listens to its
/// corresponding MIDI channel; `Omni` is useful for layered parts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sf16Listen {
    Channel(u8),
    Omni,
    Off,
}

/// UI-readable state mirrored from rustysynth's write-only channel API.
/// Values with a fine controller use the standard 14-bit MIDI representation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sf16PartState {
    pub bank: u8,
    pub program: u8,
    pub volume: u16,
    pub pan: u16,
    pub expression: u16,
    pub modulation: u16,
    pub reverb_send: u8,
    pub chorus_send: u8,
    pub bend_range_st: u8,
    pub fine_tune_ct: i8,
    pub coarse_tune_st: i8,
    pub hold: bool,
    pub mute: bool,
    pub solo: bool,
    pub listen: Sf16Listen,
}

impl Sf16PartState {
    fn defaults(part: usize) -> Self {
        Self {
            bank: 0,
            program: 0,
            volume: 100 << 7,
            pan: 64 << 7,
            expression: 127 << 7,
            modulation: 0,
            reverb_send: 40,
            chorus_send: 0,
            bend_range_st: 2,
            fine_tune_ct: 0,
            coarse_tune_st: 0,
            hold: false,
            mute: false,
            solo: false,
            listen: Sf16Listen::Channel(part as u8),
        }
    }
}

/// Construction-only settings. Changing one requires rebuilding the synth on
/// a non-realtime thread and then replacing the instrument at a safe boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sf16Settings {
    pub block_size: usize,
    pub maximum_polyphony: usize,
    pub enable_reverb_and_chorus: bool,
}

impl Default for Sf16Settings {
    fn default() -> Self {
        Self {
            block_size: 32,
            maximum_polyphony: 128,
            enable_reverb_and_chorus: true,
        }
    }
}

/// Lock-free, audio-thread-owned 16-part SoundFont engine. It has one
/// rustysynth instance (which already contains 16 MIDI channels), a shadow
/// state for the UI, and no heap allocation or locking in `render`/MIDI paths.
pub struct Sf16Instrument {
    synth: rustysynth::Synthesizer,
    parts: [Sf16PartState; SF16_PARTS],
    held: [[bool; 128]; SF16_PARTS],
    held_count: [u8; SF16_PARTS],
    rpn: [(u8, u8); SF16_PARTS],
}

impl fmt::Debug for Sf16Instrument {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Sf16Instrument")
            .field("parts", &self.parts)
            .field("held_count", &self.held_count)
            .finish_non_exhaustive()
    }
}

impl Sf16Instrument {
    /// Construct sf-16 from an already-loaded SoundFont. Loading/parsing the
    /// file remains explicitly off the realtime path via `load_sound_font`.
    pub fn new(
        sound_font: &SoundFontBank,
        sample_rate: u32,
        settings: Sf16Settings,
    ) -> Result<Self, String> {
        let mut synth_settings = rustysynth::SynthesizerSettings::new(sample_rate as i32);
        synth_settings.block_size = settings.block_size;
        synth_settings.maximum_polyphony = settings.maximum_polyphony;
        synth_settings.enable_reverb_and_chorus = settings.enable_reverb_and_chorus;
        let synth = rustysynth::Synthesizer::new(sound_font, &synth_settings)
            .map_err(|error| format!("initialize sf-16 synthesizer: {error}"))?;
        Ok(Self {
            synth,
            parts: std::array::from_fn(Sf16PartState::defaults),
            held: [[false; 128]; SF16_PARTS],
            held_count: [0; SF16_PARTS],
            rpn: [(127, 127); SF16_PARTS],
        })
    }

    pub fn part(&self, part: usize) -> Option<Sf16PartState> {
        self.parts.get(part).copied()
    }

    pub fn parts(&self) -> &[Sf16PartState; SF16_PARTS] {
        &self.parts
    }

    pub fn held_count(&self, part: usize) -> Option<u8> {
        self.held_count.get(part).copied()
    }

    pub fn set_master_volume(&mut self, volume: f32) {
        self.synth.set_master_volume(volume.clamp(0.0, 1.0));
    }

    pub fn master_volume(&self) -> f32 {
        self.synth.get_master_volume()
    }

    pub fn set_listen(&mut self, part: usize, listen: Sf16Listen) {
        if let Some(state) = self.parts.get_mut(part) {
            *state = Sf16PartState { listen, ..*state };
        }
    }

    pub fn set_program(&mut self, part: usize, bank: u8, program: u8) {
        let Some(state) = self.parts.get_mut(part) else {
            return;
        };
        state.bank = bank.min(127);
        state.program = program.min(127);
        let (bank, program) = (state.bank, state.program);
        self.send(part, 0xB0, 0, bank);
        self.send(part, 0xC0, program, 0);
    }

    pub fn set_mute_solo(&mut self, part: usize, mute: bool, solo: bool) {
        let Some(state) = self.parts.get_mut(part) else {
            return;
        };
        state.mute = mute;
        state.solo = solo;
        self.refresh_effective_volumes();
    }

    /// Process an ordinary channel MIDI message. Routing is resolved against
    /// `listen` first; a message can deliberately reach several Omni parts.
    pub fn process_midi_message(&mut self, channel: u8, command: u8, data1: u8, data2: u8) {
        if channel as usize >= SF16_PARTS {
            return;
        }
        for part in 0..SF16_PARTS {
            if self.listens_to(part, channel) {
                self.apply_to_part(part, command & 0xF0, data1 & 0x7F, data2 & 0x7F);
            }
        }
    }

    pub fn panic(&mut self) {
        for part in 0..SF16_PARTS {
            self.send(part, 0xB0, 120, 0);
            self.held[part].fill(false);
            self.held_count[part] = 0;
        }
    }

    pub fn reset_part(&mut self, part: usize) {
        if part >= SF16_PARTS {
            return;
        }
        self.synth.reset_all_controllers_channel(part as i32);
        self.parts[part] = Sf16PartState::defaults(part);
        self.held[part].fill(false);
        self.held_count[part] = 0;
        self.rpn[part] = (127, 127);
        self.send(part, 0xB0, 0, 0);
        self.send(part, 0xC0, 0, 0);
        self.apply_part_state(part);
    }

    fn listens_to(&self, part: usize, channel: u8) -> bool {
        matches!(self.parts[part].listen, Sf16Listen::Omni)
            || matches!(self.parts[part].listen, Sf16Listen::Channel(value) if value == channel)
    }

    fn send(&mut self, part: usize, command: u8, data1: u8, data2: u8) {
        self.synth.process_midi_message(
            part as i32,
            i32::from(command),
            i32::from(data1),
            i32::from(data2),
        );
    }

    fn apply_to_part(&mut self, part: usize, command: u8, data1: u8, data2: u8) {
        match command {
            0x80 => self.set_held(part, data1, false),
            0x90 if data2 == 0 => self.set_held(part, data1, false),
            0x90 => self.set_held(part, data1, true),
            0xB0 => self.update_controller(part, data1, data2),
            0xC0 => self.parts[part].program = data1,
            _ => {}
        }
        // Mute and solo are sf-16 policy rather than native MIDI controls.
        // Preserve the underlying CC7 value but never forward a bypassing CC7.
        if !(command == 0xB0 && (data1 == 7 || data1 == 39)) {
            self.send(part, command, data1, data2);
        }
        if command == 0xB0 && (data1 == 7 || data1 == 39) {
            self.apply_effective_volume(part);
        }
    }

    fn update_controller(&mut self, part: usize, controller: u8, value: u8) {
        let state = &mut self.parts[part];
        match controller {
            0 => state.bank = value,
            1 => state.modulation = (state.modulation & 0x7F) | (u16::from(value) << 7),
            33 => state.modulation = (state.modulation & !0x7F) | u16::from(value),
            7 => state.volume = (state.volume & 0x7F) | (u16::from(value) << 7),
            39 => state.volume = (state.volume & !0x7F) | u16::from(value),
            10 => state.pan = (state.pan & 0x7F) | (u16::from(value) << 7),
            42 => state.pan = (state.pan & !0x7F) | u16::from(value),
            11 => state.expression = (state.expression & 0x7F) | (u16::from(value) << 7),
            43 => state.expression = (state.expression & !0x7F) | u16::from(value),
            64 => state.hold = value >= 64,
            91 => state.reverb_send = value,
            93 => state.chorus_send = value,
            101 => self.rpn[part].0 = value,
            100 => self.rpn[part].1 = value,
            6 => match self.rpn[part] {
                (0, 0) => state.bend_range_st = value.min(24),
                (0, 1) => {
                    state.fine_tune_ct =
                        (((i16::from(value) << 7) - 8192) / 82).clamp(-100, 100) as i8
                }
                (0, 2) => state.coarse_tune_st = (i16::from(value) - 64).clamp(-24, 24) as i8,
                _ => {}
            },
            38 if self.rpn[part] == (0, 1) => {
                let coarse = i16::from(state.fine_tune_ct) * 82 + 8192;
                state.fine_tune_ct =
                    ((coarse + i16::from(value) - 8192) / 82).clamp(-100, 100) as i8;
            }
            120 | 123 => {
                self.held[part].fill(false);
                self.held_count[part] = 0;
            }
            121 => {
                let mute = state.mute;
                let solo = state.solo;
                *state = Sf16PartState {
                    mute,
                    solo,
                    ..Sf16PartState::defaults(part)
                };
                self.rpn[part] = (127, 127);
            }
            _ => {}
        }
    }

    fn set_held(&mut self, part: usize, note: u8, held: bool) {
        let slot = &mut self.held[part][note as usize];
        if *slot == held {
            return;
        }
        *slot = held;
        if held {
            self.held_count[part] = self.held_count[part].saturating_add(1);
        } else {
            self.held_count[part] = self.held_count[part].saturating_sub(1);
        }
    }

    fn any_solo(&self) -> bool {
        self.parts.iter().any(|part| part.solo)
    }

    fn effective_volume(&self, part: usize) -> u16 {
        let state = self.parts[part];
        if state.mute || (self.any_solo() && !state.solo) {
            0
        } else {
            state.volume
        }
    }

    fn apply_effective_volume(&mut self, part: usize) {
        let volume = self.effective_volume(part);
        self.send(part, 0xB0, 7, (volume >> 7) as u8);
        self.send(part, 0xB0, 39, (volume & 0x7F) as u8);
    }

    fn refresh_effective_volumes(&mut self) {
        for part in 0..SF16_PARTS {
            self.apply_effective_volume(part);
        }
    }

    fn apply_part_state(&mut self, part: usize) {
        let state = self.parts[part];
        self.send(part, 0xB0, 1, (state.modulation >> 7) as u8);
        self.send(part, 0xB0, 33, (state.modulation & 0x7F) as u8);
        self.send(part, 0xB0, 10, (state.pan >> 7) as u8);
        self.send(part, 0xB0, 42, (state.pan & 0x7F) as u8);
        self.send(part, 0xB0, 11, (state.expression >> 7) as u8);
        self.send(part, 0xB0, 43, (state.expression & 0x7F) as u8);
        self.send(part, 0xB0, 64, if state.hold { 127 } else { 0 });
        self.send(part, 0xB0, 91, state.reverb_send);
        self.send(part, 0xB0, 93, state.chorus_send);
        self.apply_effective_volume(part);
    }
}

impl Instrument for Sf16Instrument {
    fn render(&mut self, left: &mut [f32], right: &mut [f32]) {
        self.synth.render(left, right);
    }

    fn note_on(&mut self, note: u8, velocity: u8) {
        self.process_midi_message(0, 0x90, note, velocity);
    }

    fn note_off(&mut self, note: u8) {
        self.process_midi_message(0, 0x80, note, 0);
    }

    fn pitch_bend(&mut self, value: u16) {
        self.process_midi_message(0, 0xE0, value as u8 & 0x7F, (value >> 7) as u8 & 0x7F);
    }

    fn control_change(&mut self, controller: u8, value: u8) {
        self.process_midi_message(0, 0xB0, controller, value);
    }

    fn program_change(&mut self, program: u8) {
        self.process_midi_message(0, 0xC0, program, 0);
    }

    fn all_notes_off(&mut self) {
        self.panic();
    }

    fn reset(&mut self) {
        self.synth.reset();
        for part in 0..SF16_PARTS {
            self.parts[part] = Sf16PartState::defaults(part);
            self.held[part].fill(false);
            self.held_count[part] = 0;
            self.rpn[part] = (127, 127);
        }
    }

    fn midi_message(&mut self, channel: u8, command: u8, data1: u8, data2: u8) {
        self.process_midi_message(channel, command, data1, data2);
    }
}

/// A fully initialized replacement prepared off the render thread.
struct SynthUpdate {
    synth: rustysynth::Synthesizer,
    channel: u8,
}

/// Control-thread handle to a live instrument. Push replacement synths here;
/// the matching consumer lives on the audio thread inside [`SoundFontInstrument`].
pub struct SoundFontHandle {
    producer: Producer<SynthUpdate>,
    retired: Consumer<rustysynth::Synthesizer>,
}

impl SoundFontHandle {
    /// Replace the running synthesizer (e.g. after the user selects a different
    /// preset or bank). Called only from a non-realtime thread.
    pub fn replace(&mut self, synth: rustysynth::Synthesizer, channel: u8) -> Result<(), String> {
        self.reclaim_retired();
        self.producer
            .push(SynthUpdate { synth, channel })
            .map_err(|_| "instrument swap queue is full; try again".to_string())
    }

    /// Drop replaced synthesizers on the control thread.
    pub fn reclaim_retired(&mut self) {
        while self.retired.pop().is_ok() {}
    }
}

impl fmt::Debug for SoundFontHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SoundFontHandle").finish_non_exhaustive()
    }
}

/// A mono-timbral SoundFont instrument rendered on the audio thread.
pub struct SoundFontInstrument {
    synth: rustysynth::Synthesizer,
    channel: u8,
    updates: Consumer<SynthUpdate>,
    retired: Producer<rustysynth::Synthesizer>,
}

impl fmt::Debug for SoundFontInstrument {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SoundFontInstrument")
            .field("channel", &self.channel)
            .field("synth", &self.synth)
            .finish()
    }
}

/// Build a synthesizer for a GM channel/program pair from an already-parsed
/// SoundFont. The sample data is shared via `Arc`, so many track instruments
/// reuse one bank without copying its waveform.
pub fn build_synth(
    sound_font: &Arc<rustysynth::SoundFont>,
    sample_rate: u32,
    channel: u8,
    program: u8,
) -> Result<rustysynth::Synthesizer, String> {
    let mut settings = rustysynth::SynthesizerSettings::new(sample_rate as i32);
    settings.maximum_polyphony = 96;
    settings.enable_reverb_and_chorus = true;
    let mut synth = rustysynth::Synthesizer::new(sound_font, &settings)
        .map_err(|error| format!("initialize SoundFont synthesizer: {error}"))?;
    synth.process_midi_message(i32::from(channel), 0xC0, i32::from(program), 0);
    Ok(synth)
}

/// Per-track synth tone, expressed as GM control-change values. The SoundFont
/// itself provides the amplitude/pitch envelopes, so the rack exposes the
/// synthesis parameters that `rustysynth` honors (volume, pan, modulation,
/// sustain, reverb and chorus send).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SoundFontTone {
    pub volume: u8,     // CC7
    pub pan: u8,        // CC10
    pub modulation: u8, // CC1
    pub sustain: u8,    // CC64 (0 or 127)
    pub reverb: u8,     // CC91
    pub chorus: u8,     // CC93
}

impl Default for SoundFontTone {
    fn default() -> Self {
        Self {
            volume: 100,
            pan: 64,
            modulation: 0,
            sustain: 0,
            reverb: 40,
            chorus: 0,
        }
    }
}

/// Apply tone control changes directly to a running synthesizer. Called only
/// from the audio thread through the MIDI command queue.
pub fn apply_tone(synth: &mut rustysynth::Synthesizer, channel: u8, tone: SoundFontTone) {
    for (controller, value) in [
        (7, tone.volume),
        (10, tone.pan),
        (1, tone.modulation),
        (64, tone.sustain),
        (91, tone.reverb),
        (93, tone.chorus),
    ] {
        synth.process_midi_message(i32::from(channel), 0xB0, controller, i32::from(value));
    }
}

/// Build a synthesizer and immediately apply a tone profile, so a swapped or
/// freshly-embedded instrument starts with the track's current settings.
pub fn build_synth_with_tone(
    sound_font: &Arc<rustysynth::SoundFont>,
    sample_rate: u32,
    channel: u8,
    program: u8,
    tone: SoundFontTone,
) -> Result<rustysynth::Synthesizer, String> {
    let mut synth = build_synth(sound_font, sample_rate, channel, program)?;
    apply_tone(&mut synth, channel, tone);
    Ok(synth)
}

/// Parse a `.sf2` SoundFont from disk. This reads and allocates, so callers must
/// invoke it on a background/non-realtime thread only.
pub fn load_sound_font(path: &Path) -> Result<Arc<rustysynth::SoundFont>, String> {
    let mut file =
        std::fs::File::open(path).map_err(|error| format!("open {}: {error}", path.display()))?;
    let sound_font = rustysynth::SoundFont::new(&mut file)
        .map_err(|error| format!("parse {}: {error}", path.display()))?;
    Ok(Arc::new(sound_font))
}

impl SoundFontInstrument {
    /// Create a new instrument for `channel`/`program`, and return it together
    /// with the control handle used to swap presets later. `sound_font` must
    /// already be parsed (see [`load_sound_font`]).
    pub fn new(
        sound_font: Arc<rustysynth::SoundFont>,
        sample_rate: u32,
        channel: u8,
        program: u8,
    ) -> Result<(Self, SoundFontHandle), String> {
        let synth = build_synth(&sound_font, sample_rate, channel, program)?;
        Ok(Self::with_synth(synth, channel))
    }

    /// Wrap an already-built synthesizer (e.g. one produced by
    /// [`build_synth_with_tone`]) and create its live-swap channel.
    pub fn with_synth(synth: rustysynth::Synthesizer, channel: u8) -> (Self, SoundFontHandle) {
        let (producer, consumer) = RingBuffer::new(SYNTH_UPDATE_CAPACITY);
        let (retire, reclaim) = RingBuffer::new(SYNTH_UPDATE_CAPACITY);
        (
            Self {
                synth,
                channel,
                updates: consumer,
                retired: retire,
            },
            SoundFontHandle {
                producer,
                retired: reclaim,
            },
        )
    }

    fn drain_updates(&mut self) {
        while !self.retired.is_full() {
            let Ok(update) = self.updates.pop() else {
                break;
            };
            let previous = std::mem::replace(&mut self.synth, update.synth);
            self.channel = update.channel;
            // This callback is the only producer. The consumer can only
            // increase the space already checked above.
            self.retired
                .push(previous)
                .expect("reserved retirement slot");
        }
    }
}

impl Instrument for SoundFontInstrument {
    fn render(&mut self, left: &mut [f32], right: &mut [f32]) {
        self.drain_updates();
        self.synth.render(left, right);
    }

    fn note_on(&mut self, note: u8, velocity: u8) {
        self.drain_updates();
        self.synth.process_midi_message(
            i32::from(self.channel),
            0x90,
            i32::from(note),
            i32::from(velocity),
        );
    }

    fn note_off(&mut self, note: u8) {
        self.drain_updates();
        self.synth
            .process_midi_message(i32::from(self.channel), 0x80, i32::from(note), 0);
    }

    fn pitch_bend(&mut self, value: u16) {
        self.drain_updates();
        let value = value.clamp(0, 16_383);
        let lsb = (value & 0x7F) as i32;
        let msb = ((value >> 7) & 0x7F) as i32;
        self.synth
            .process_midi_message(i32::from(self.channel), 0xE0, lsb, msb);
    }

    fn control_change(&mut self, controller: u8, value: u8) {
        self.drain_updates();
        self.synth.process_midi_message(
            i32::from(self.channel),
            0xB0,
            i32::from(controller & 0x7F),
            i32::from(value & 0x7F),
        );
    }

    fn program_change(&mut self, program: u8) {
        self.drain_updates();
        self.synth.process_midi_message(
            i32::from(self.channel),
            0xC0,
            i32::from(program & 0x7F),
            0,
        );
    }

    fn all_notes_off(&mut self) {
        self.drain_updates();
        let channel = i32::from(self.channel);
        self.synth.process_midi_message(channel, 0xB0, 64, 0);
        self.synth.note_off_all_channel(channel, true);
    }

    fn reset(&mut self) {
        self.synth.reset();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A test instrument that records MIDI and emits a constant stereo signal,
    /// used to verify the [`Instrument`] contract and graph wiring without a
    /// real SoundFont file.
    #[derive(Debug, Default)]
    struct MockInstrument {
        pub notes_on: u32,
        pub notes_off: u32,
        pub programs: Vec<u8>,
        pub control_changes: Vec<(u8, u8)>,
        pub pitch_bends: Vec<u16>,
        pub amplitude: f32,
    }

    impl Instrument for MockInstrument {
        fn render(&mut self, left: &mut [f32], right: &mut [f32]) {
            for (l, r) in left.iter_mut().zip(right.iter_mut()) {
                *l = self.amplitude;
                *r = self.amplitude;
            }
        }
        fn note_on(&mut self, _note: u8, _velocity: u8) {
            self.notes_on += 1;
            self.amplitude = 0.25;
        }
        fn note_off(&mut self, _note: u8) {
            self.notes_off += 1;
            self.amplitude = 0.0;
        }
        fn pitch_bend(&mut self, value: u16) {
            self.pitch_bends.push(value);
        }
        fn control_change(&mut self, controller: u8, value: u8) {
            self.control_changes.push((controller, value));
        }
        fn program_change(&mut self, program: u8) {
            self.programs.push(program);
        }
        fn all_notes_off(&mut self) {
            self.amplitude = 0.0;
        }
        fn reset(&mut self) {
            self.amplitude = 0.0;
        }
    }

    #[test]
    fn instrument_trait_contract_routes_midi_and_renders() {
        let mut instrument = MockInstrument::default();
        instrument.note_on(60, 100);
        assert_eq!(instrument.notes_on, 1);
        let mut left = [0.0_f32; 4];
        let mut right = [0.0_f32; 4];
        instrument.render(&mut left, &mut right);
        assert!(left.iter().all(|s| *s == 0.25));
        instrument.note_off(60);
        assert_eq!(instrument.notes_off, 1);
        instrument.program_change(5);
        instrument.control_change(7, 100);
        instrument.pitch_bend(8192);
        assert_eq!(instrument.programs, vec![5]);
        assert_eq!(instrument.control_changes, vec![(7, 100)]);
        assert_eq!(instrument.pitch_bends, vec![8192]);
        instrument.all_notes_off();
        instrument.render(&mut left, &mut right);
        assert!(left.iter().all(|s| *s == 0.0));
    }

    #[test]
    fn load_sound_font_reports_missing_file_without_panicking() {
        let missing = Path::new("/nonexistent/rusty-no-such-bank.sf2");
        assert!(load_sound_font(missing).is_err());
    }

    fn sf16_fixture() -> SoundFontBank {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/qa-two-programs.sf2");
        load_sound_font(&path).expect("generated two-program SoundFont fixture")
    }

    #[test]
    fn sf16_routes_channels_and_mirrors_controller_state() {
        let bank = sf16_fixture();
        let mut sf16 = Sf16Instrument::new(&bank, 48_000, Sf16Settings::default()).unwrap();

        sf16.process_midi_message(3, 0x90, 60, 101);
        assert_eq!(sf16.held_count(3), Some(1));
        assert_eq!(sf16.held_count(2), Some(0));

        sf16.process_midi_message(3, 0xB0, 7, 88);
        sf16.process_midi_message(3, 0xB0, 39, 12);
        sf16.process_midi_message(3, 0xB0, 10, 17);
        sf16.process_midi_message(3, 0xC0, 9, 0);
        let state = sf16.part(3).unwrap();
        assert_eq!(state.volume, (88 << 7) | 12);
        assert_eq!(state.pan, 17 << 7);
        assert_eq!(state.program, 9);

        sf16.process_midi_message(3, 0x80, 60, 0);
        assert_eq!(sf16.held_count(3), Some(0));
    }

    #[test]
    fn sf16_supports_omni_layers_mute_solo_and_rpn_shadowing() {
        let bank = sf16_fixture();
        let mut sf16 = Sf16Instrument::new(&bank, 48_000, Sf16Settings::default()).unwrap();
        sf16.set_listen(0, Sf16Listen::Omni);
        sf16.process_midi_message(7, 0x90, 67, 110);
        assert_eq!(sf16.held_count(0), Some(1));
        assert_eq!(sf16.held_count(7), Some(1));

        sf16.set_mute_solo(0, true, false);
        sf16.set_mute_solo(7, false, true);
        assert!(sf16.part(0).unwrap().mute);
        assert!(sf16.part(7).unwrap().solo);

        // RPN 0,0: pitch-bend range; standard null RPN ends the transaction.
        sf16.process_midi_message(7, 0xB0, 101, 0);
        sf16.process_midi_message(7, 0xB0, 100, 0);
        sf16.process_midi_message(7, 0xB0, 6, 12);
        sf16.process_midi_message(7, 0xB0, 101, 127);
        sf16.process_midi_message(7, 0xB0, 100, 127);
        assert_eq!(sf16.part(7).unwrap().bend_range_st, 12);
    }
}
