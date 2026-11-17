use serde::{Deserialize, Serialize};

/// C=0, C♯=1, ... B=11. Absolute MIDI note numbers are always 0..=127.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PitchClass(u8);

impl PitchClass {
    pub const C: Self = Self(0);

    pub const fn new(value: u8) -> Option<Self> {
        if value < 12 { Some(Self(value)) } else { None }
    }

    pub const fn semitone(self) -> u8 {
        self.0
    }

    pub const fn from_midi(note: u8) -> Self {
        Self(note % 12)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ScaleMode {
    Major,
    NaturalMinor,
    HarmonicMinor,
    MelodicMinor,
    Dorian,
    Phrygian,
    Lydian,
    Mixolydian,
    Locrian,
    MajorPentatonic,
    MinorPentatonic,
    Blues,
}

impl ScaleMode {
    pub const ALL: [Self; 12] = [
        Self::Major,
        Self::NaturalMinor,
        Self::HarmonicMinor,
        Self::MelodicMinor,
        Self::Dorian,
        Self::Phrygian,
        Self::Lydian,
        Self::Mixolydian,
        Self::Locrian,
        Self::MajorPentatonic,
        Self::MinorPentatonic,
        Self::Blues,
    ];

    /// Ascending melodic minor is used for scale-aware editing.
    pub const fn semitone_offsets(self) -> &'static [u8] {
        match self {
            Self::Major => &[0, 2, 4, 5, 7, 9, 11],
            Self::NaturalMinor => &[0, 2, 3, 5, 7, 8, 10],
            Self::HarmonicMinor => &[0, 2, 3, 5, 7, 8, 11],
            Self::MelodicMinor => &[0, 2, 3, 5, 7, 9, 11],
            Self::Dorian => &[0, 2, 3, 5, 7, 9, 10],
            Self::Phrygian => &[0, 1, 3, 5, 7, 8, 10],
            Self::Lydian => &[0, 2, 4, 6, 7, 9, 11],
            Self::Mixolydian => &[0, 2, 4, 5, 7, 9, 10],
            Self::Locrian => &[0, 1, 3, 5, 6, 8, 10],
            Self::MajorPentatonic => &[0, 2, 4, 7, 9],
            Self::MinorPentatonic => &[0, 3, 5, 7, 10],
            Self::Blues => &[0, 3, 5, 6, 7, 10],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ScaleDefinition {
    pub root: PitchClass,
    pub mode: ScaleMode,
}

impl ScaleDefinition {
    pub const fn new(root: PitchClass, mode: ScaleMode) -> Self {
        Self { root, mode }
    }

    pub fn contains_pitch_class(self, pitch: PitchClass) -> bool {
        let relative = (pitch.semitone() + 12 - self.root.semitone()) % 12;
        self.mode.semitone_offsets().contains(&relative)
    }

    pub fn contains_midi_note(self, midi_note: u8) -> bool {
        midi_note <= 127 && self.contains_pitch_class(PitchClass::from_midi(midi_note))
    }

    /// For scale folding, index 0 is the lowest in-scale note in MIDI's 0..127 range.
    /// Out-of-scale notes are hidden rather than re-quantized.
    pub fn folded_index(self, midi_note: u8) -> Option<usize> {
        if !self.contains_midi_note(midi_note) {
            return None;
        }
        Some(
            (0..midi_note)
                .filter(|&n| self.contains_midi_note(n))
                .count(),
        )
    }

    pub fn midi_note_at_folded_index(self, index: usize) -> Option<u8> {
        (0_u8..=127)
            .filter(|&n| self.contains_midi_note(n))
            .nth(index)
    }

    pub fn visible_note_count(self) -> usize {
        (0_u8..=127).filter(|&n| self.contains_midi_note(n)).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn c_major_highlights_correct_pitch_classes() {
        let s = ScaleDefinition::new(PitchClass::C, ScaleMode::Major);
        for note in [60, 62, 64, 65, 67, 69, 71, 72] {
            assert!(s.contains_midi_note(note), "{note}");
        }
        for note in [61, 63, 66, 68, 70] {
            assert!(!s.contains_midi_note(note), "{note}");
        }
    }

    #[test]
    fn g_minor_and_blues_are_transposed_correctly() {
        let g = PitchClass::new(7).unwrap();
        let s = ScaleDefinition::new(g, ScaleMode::NaturalMinor);
        assert!(s.contains_midi_note(70)); // Bb
        assert!(!s.contains_midi_note(71)); // B natural
        let blues = ScaleDefinition::new(PitchClass::C, ScaleMode::Blues);
        assert!(blues.contains_midi_note(66)); // Gb (blue note)
    }

    #[test]
    fn fold_round_trips_for_every_root_and_mode() {
        for root in 0..12 {
            for mode in ScaleMode::ALL {
                let scale = ScaleDefinition::new(PitchClass::new(root).unwrap(), mode);
                let notes = (0_u8..=127)
                    .filter(|&n| scale.contains_midi_note(n))
                    .collect::<Vec<_>>();
                assert_eq!(notes.len(), scale.visible_note_count());
                for (index, note) in notes.iter().enumerate() {
                    assert_eq!(scale.folded_index(*note), Some(index));
                    assert_eq!(scale.midi_note_at_folded_index(index), Some(*note));
                }
                assert_eq!(scale.midi_note_at_folded_index(notes.len()), None);
            }
        }
    }

    #[test]
    fn out_of_scale_notes_are_not_rounded_into_key() {
        let s = ScaleDefinition::new(PitchClass::C, ScaleMode::Major);
        assert_eq!(s.folded_index(61), None);
        assert!(!s.contains_midi_note(255));
    }

    #[test]
    fn rejects_pitch_class_out_of_range() {
        assert_eq!(PitchClass::new(12), None);
        assert_eq!(PitchClass::new(11).unwrap().semitone(), 11);
    }
}
