//! Musical-domain primitives. This crate never runs untrusted plugins and has no audio I/O.
//! Mutable edit history stays off the eventual real-time audio callback.
#![forbid(unsafe_code)]

pub mod commands;
pub mod instruments;
pub mod model;
pub mod plugin_state;
pub mod scales;
pub mod session;
pub mod time;

pub use commands::{Command, CommandError, RenameProject, UndoStack};
pub use instruments::{
    BuiltinInstrument, DEFAULT_PRESET, InstrumentCategory, InstrumentPreset, PRESETS,
    resolve_preset,
};
pub use model::{
    AudioClip, BuiltinEffect, BuiltinEffectKind, ClapLiveInsert, Clip, ClipKind, MidiClip,
    MidiNote, Project, RecordInputChannels, Track, TrackKind,
};
pub use plugin_state::{
    FORMAT_CLAP, FORMAT_VST3, MAX_PLUGIN_STATE_BYTES, PLUGIN_STATE_VERSION, PluginStateBlob,
};
pub use scales::{PitchClass, ScaleDefinition, ScaleMode};
pub use time::{MusicalTime, PPQ, Tempo, TempoMap, TimeError, TimeSignature};
