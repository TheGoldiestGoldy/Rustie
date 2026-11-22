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
}
