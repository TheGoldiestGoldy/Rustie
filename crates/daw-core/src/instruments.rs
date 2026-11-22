//! Curated built-in instrument rack catalog.
//!
//! These presets map to General MIDI (GM) bank/program numbers so a single
//! SoundFont (`.sf2`) bank can serve every instrument. Each MIDI track owns one
//! mono-timbral instrument on a single MIDI channel; drums use channel 9 and a
//! GM percussion kit. The catalog lives in `daw-core` so both the audio engine
//! and the desktop UI resolve presets from one source of truth.

use serde::{Deserialize, Serialize};

/// Ableton-style browser categories for the built-in rack.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum InstrumentCategory {
    PianosKeys,
    Bass,
    StringsBrass,
    ClassicSynths,
    DrumsPercussion,
}

impl InstrumentCategory {
    pub const ALL: [Self; 5] = [
        Self::PianosKeys,
        Self::Bass,
        Self::StringsBrass,
        Self::ClassicSynths,
        Self::DrumsPercussion,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::PianosKeys => "Pianos & Keys",
            Self::Bass => "Bass",
            Self::StringsBrass => "Strings & Brass",
            Self::ClassicSynths => "Classic Synths",
            Self::DrumsPercussion => "Drums & Percussion",
        }
    }
}

/// A single curated preset. `channel`/`program` are the GM values sent to the
/// SoundFont synthesizer; melodic presets use channel 0 and drums use channel 9.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InstrumentPreset {
    pub name: &'static str,
    pub category: InstrumentCategory,
    pub channel: u8,
    pub program: u8,
}

impl InstrumentPreset {
    /// Whether this preset plays the GM percussion channel. Percussion presets
    /// are monophonic-per-slot GM kits; their `program` selects the kit.
    pub fn is_drum(self) -> bool {
        self.channel == 9
    }
}

/// The complete curated rack. Program numbers follow the General MIDI Level 1
/// sound set; drums are GM percussion kits selected by program change on
/// channel 9. SoundFonts that omit a specific kit fall back to the standard
/// kit inside `rustysynth`.
pub const PRESETS: &[InstrumentPreset] = &[
    // Pianos & Keys
    InstrumentPreset {
        name: "Acoustic Piano",
        category: InstrumentCategory::PianosKeys,
        channel: 0,
        program: 0,
    },
    InstrumentPreset {
        name: "Upright Piano",
        category: InstrumentCategory::PianosKeys,
        channel: 0,
        program: 1,
    },
    InstrumentPreset {
        name: "Rhodes",
        category: InstrumentCategory::PianosKeys,
        channel: 0,
        program: 4,
    },
    InstrumentPreset {
        name: "Wurlitzer",
        category: InstrumentCategory::PianosKeys,
        channel: 0,
        program: 5,
    },
    InstrumentPreset {
        name: "Clavinet",
        category: InstrumentCategory::PianosKeys,
        channel: 0,
        program: 7,
    },
    // Bass
    InstrumentPreset {
        name: "Acoustic Bass",
        category: InstrumentCategory::Bass,
        channel: 0,
        program: 32,
    },
    InstrumentPreset {
        name: "Electric Bass",
        category: InstrumentCategory::Bass,
        channel: 0,
        program: 33,
    },
    InstrumentPreset {
        name: "Fretless Bass",
        category: InstrumentCategory::Bass,
        channel: 0,
        program: 35,
    },
    InstrumentPreset {
        name: "Slap Bass",
        category: InstrumentCategory::Bass,
        channel: 0,
        program: 36,
    },
    // Strings & Brass
    InstrumentPreset {
        name: "String Ensemble",
        category: InstrumentCategory::StringsBrass,
        channel: 0,
        program: 48,
    },
    InstrumentPreset {
        name: "Slow Strings",
        category: InstrumentCategory::StringsBrass,
        channel: 0,
        program: 49,
    },
    InstrumentPreset {
        name: "Trumpet",
        category: InstrumentCategory::StringsBrass,
        channel: 0,
        program: 56,
    },
    InstrumentPreset {
        name: "Trombone",
        category: InstrumentCategory::StringsBrass,
        channel: 0,
        program: 57,
    },
    InstrumentPreset {
        name: "French Horn",
        category: InstrumentCategory::StringsBrass,
        channel: 0,
        program: 60,
    },
    InstrumentPreset {
        name: "Brass Section",
        category: InstrumentCategory::StringsBrass,
        channel: 0,
        program: 61,
    },
    // Classic Synths
    InstrumentPreset {
        name: "Analog Lead",
        category: InstrumentCategory::ClassicSynths,
        channel: 0,
        program: 81,
    },
    InstrumentPreset {
        name: "Analog Pad",
        category: InstrumentCategory::ClassicSynths,
        channel: 0,
        program: 89,
    },
    InstrumentPreset {
        name: "FM Keys",
        category: InstrumentCategory::ClassicSynths,
        channel: 0,
        program: 5,
    },
    // Drums & Percussion (GM channel 9 kits)
    InstrumentPreset {
        name: "Standard Kit",
        category: InstrumentCategory::DrumsPercussion,
        channel: 9,
        program: 0,
    },
    InstrumentPreset {
        name: "Room Kit",
        category: InstrumentCategory::DrumsPercussion,
        channel: 9,
        program: 8,
    },
    InstrumentPreset {
        name: "Power Kit",
        category: InstrumentCategory::DrumsPercussion,
        channel: 9,
        program: 16,
    },
    InstrumentPreset {
        name: "Electronic Kit",
        category: InstrumentCategory::DrumsPercussion,
        channel: 9,
        program: 24,
    },
    InstrumentPreset {
        name: "TR-808 Kit",
        category: InstrumentCategory::DrumsPercussion,
        channel: 9,
        program: 25,
    },
];

/// Resolve a preset name (case-insensitive) to its GM channel/program pair.
/// Returns `None` when the name is not part of the curated catalog.
pub fn resolve_preset(name: &str) -> Option<InstrumentPreset> {
    PRESETS
        .iter()
        .copied()
        .find(|preset| preset.name.eq_ignore_ascii_case(name.trim()))
}

/// Fallback preset used when a track has no instrument yet (or a stored preset
/// is no longer part of the catalog).
pub const DEFAULT_PRESET: InstrumentPreset = InstrumentPreset {
    name: "Acoustic Piano",
    category: InstrumentCategory::PianosKeys,
    channel: 0,
    program: 0,
};

/// Persisted instrument assignment for a MIDI track. `bank` is the SoundFont
/// file name (not a path) found in `assets/soundfonts/`; `preset` is a curated
/// catalog name. Old projects omit this field and fall back to the built-in
/// sine synth until a SoundFont is loaded.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuiltinInstrument {
    pub bank: String,
    pub preset: String,
}

impl BuiltinInstrument {
    pub fn new(bank: impl Into<String>, preset: impl Into<String>) -> Self {
        Self {
            bank: bank.into(),
            preset: preset.into(),
        }
    }

    /// Resolve to a catalog entry, falling back to `DEFAULT_PRESET` when the
    /// stored name is unknown (e.g. an older project referencing a removed preset).
    pub fn resolve(&self) -> InstrumentPreset {
        resolve_preset(&self.preset).unwrap_or(DEFAULT_PRESET)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_preset_has_a_distinct_name_and_valid_channel() {
        for preset in PRESETS {
            assert!((0..16).contains(&preset.channel), "{} channel", preset.name);
            assert!(
                (preset.is_drum() && preset.channel == 9)
                    || (!preset.is_drum() && preset.channel == 0),
                "{} must be channel 0 or 9",
                preset.name
            );
            assert!(preset.program <= 127, "{} program", preset.name);
        }
        let mut names: Vec<_> = PRESETS.iter().map(|p| p.name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), PRESETS.len(), "duplicate preset names");
    }

    #[test]
    fn preset_resolution_is_case_insensitive_and_unknown_names_fall_back() {
        assert_eq!(resolve_preset("rhodes").unwrap().program, 4);
        assert_eq!(resolve_preset("  ACoustic Piano ").unwrap().program, 0);
        assert!(resolve_preset("does not exist").is_none());
        let unknown = BuiltinInstrument::new("Any.sf2", "does not exist");
        assert_eq!(unknown.resolve(), DEFAULT_PRESET);
        assert_eq!(unknown.resolve().channel, 0);
    }

    #[test]
    fn drums_use_the_percussion_channel() {
        let kit = resolve_preset("Standard Kit").unwrap();
        assert!(kit.is_drum());
        assert_eq!(kit.channel, 9);
    }
}
