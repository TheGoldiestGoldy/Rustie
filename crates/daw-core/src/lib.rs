//! Musical-domain primitives. This crate never runs untrusted plugins and has no audio I/O.
//! Mutable edit history stays off the eventual real-time audio callback.
#![forbid(unsafe_code)]

pub mod commands;
pub mod model;
pub mod scales;
pub mod time;

pub use commands::{Command, CommandError, RenameProject, UndoStack};
pub use model::{AudioClip, Clip, ClipKind, MidiClip, MidiNote, Project, Track, TrackKind};
pub use scales::{PitchClass, ScaleDefinition, ScaleMode};
pub use time::{MusicalTime, PPQ, Tempo, TempoMap, TimeError, TimeSignature};
