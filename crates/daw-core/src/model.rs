use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::scales::ScaleDefinition;
use crate::time::{MusicalTime, Tempo, TempoMap, TimeSignature};

/// Data-only model. These types are not the audio-thread's processing graph.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Project {
    pub name: String,
    pub tempo_map: TempoMap,
    pub time_signature: TimeSignature,
    pub global_scale: Option<ScaleDefinition>,
    pub tracks: Vec<Track>,
}

impl Project {
    pub fn new(name: impl Into<String>, initial_tempo: Tempo) -> Self {
        Self {
            name: name.into(),
            tempo_map: TempoMap::new(initial_tempo),
            time_signature: TimeSignature::default(),
            global_scale: None,
            tracks: Vec::new(),
        }
    }

    pub fn track_mut(&mut self, id: u64) -> Option<&mut Track> {
        self.tracks.iter_mut().find(|track| track.id == id)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrackKind {
    Audio,
    Midi,
    Return,
    Master,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Track {
    /// Must be unique within a project; ID assignment is owned by the editor.
    pub id: u64,
    pub name: String,
    pub kind: TrackKind,
    pub clips: Vec<Clip>,
    pub volume: f32,
    pub pan: f32,
    /// Session mixer state. Defaults preserve pre-mute/solo JSON projects.
    #[serde(default)]
    pub muted: bool,
    #[serde(default)]
    pub solo: bool,
}

impl Track {
    pub fn new(id: u64, name: impl Into<String>, kind: TrackKind) -> Self {
        Self {
            id,
            name: name.into(),
            kind,
            clips: Vec::new(),
            volume: 1.0,
            pan: 0.0,
            muted: false,
            solo: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Clip {
    pub id: u64,
    pub start: MusicalTime,
    pub duration: MusicalTime,
    pub kind: ClipKind,
    pub scale_override: Option<ScaleDefinition>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ClipKind {
    Audio(AudioClip),
    Midi(MidiClip),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AudioClip {
    pub source: PathBuf,
    pub warped: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MidiClip {
    pub notes: Vec<MidiNote>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MidiNote {
    pub pitch: u8,
    pub start: MusicalTime,
    pub duration: MusicalTime,
    pub velocity: u8,
    pub muted: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_initializes_without_audio_io() {
        let mut p = Project::new("Untitled", Tempo::default());
        p.tracks.push(Track::new(42, "Piano", TrackKind::Midi));
        assert_eq!(p.track_mut(42).unwrap().name, "Piano");
        assert!(p.track_mut(17).is_none());
        assert_eq!(p.time_signature, TimeSignature::default());
    }
}
