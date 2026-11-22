use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::instruments::BuiltinInstrument;
use crate::plugin_state::PluginStateBlob;
use crate::scales::ScaleDefinition;
use crate::time::{MusicalTime, Tempo, TempoMap, TimeSignature};

/// Data-only model. These types are not the audio-thread's processing graph.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Project {
    pub name: String,
    pub tempo_map: TempoMap,
    pub time_signature: TimeSignature,
    pub global_scale: Option<ScaleDefinition>,
    #[serde(default)]
    pub metronome_enabled: bool,
    /// Persisted metronome level, independent of the master mixer.
    /// Missing values in older projects default to the original full level.
    #[serde(default = "default_metronome_volume")]
    pub metronome_volume: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<crate::session::Session>,
    pub tracks: Vec<Track>,
}

fn default_metronome_volume() -> f32 {
    1.0
}

impl Project {
    pub fn new(name: impl Into<String>, initial_tempo: Tempo) -> Self {
        Self {
            name: name.into(),
            tempo_map: TempoMap::new(initial_tempo),
            time_signature: TimeSignature::default(),
            global_scale: None,
            metronome_enabled: false,
            metronome_volume: default_metronome_volume(),
            session: Some(crate::session::Session::default()),
            tracks: Vec::new(),
        }
    }

    pub fn track_mut(&mut self, id: u64) -> Option<&mut Track> {
        self.tracks.iter_mut().find(|track| track.id == id)
    }

    /// Reject malformed group routing before a loaded project is played.
    /// The current UI supports flat group buses, not nested group networks.
    pub fn validate_groups(&self) -> Result<(), &'static str> {
        for track in &self.tracks {
            if track.kind == TrackKind::Group
                && (track.group_id.is_some() || !track.clips.is_empty())
            {
                return Err("group buses cannot be nested or contain clips");
            }
            if let Some(group) = track.group_id
                && (track.kind == TrackKind::Group
                    || !self
                        .tracks
                        .iter()
                        .any(|parent| parent.id == group && parent.kind == TrackKind::Group))
            {
                return Err("track references a missing or invalid group bus");
            }
        }
        Ok(())
    }

    /// A group bus is allowed to own MIDI/audio tracks, never another group.
    pub fn group_of(&self, track: &Track) -> Option<&Track> {
        let id = track.group_id?;
        self.tracks
            .iter()
            .find(|candidate| candidate.id == id && candidate.kind == TrackKind::Group)
    }

    /// Children of a soloed group are audible; soloing one child suppresses
    /// unrelated tracks, including siblings. Parent mute always overrides solo.
    pub fn track_audible(&self, track: &Track) -> bool {
        let parent = self.group_of(track);
        if track.muted || parent.is_some_and(|group| group.muted) {
            return false;
        }
        let any_solo = self.tracks.iter().any(|t| t.solo);
        !any_solo || track.solo || parent.is_some_and(|group| group.solo)
    }

    /// The group's gain and pan are applied at the actual group graph node;
    /// this is only the direct source-level gain, not the full bus gain.
    pub fn source_gain(&self, track: &Track) -> f32 {
        if self.track_audible(track) {
            track.volume
        } else {
            0.0
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrackKind {
    Audio,
    Midi,
    /// A real mix bus. Other tracks may route into it using `group_id`.
    Group,
    Return,
    Master,
}

/// Channel selection within a physical recording source. Legacy projects use
/// both inputs on a stereo source and the single input on a mono source.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum RecordInputChannels {
    #[default]
    Stereo,
    Mono1,
    Mono2,
}

impl RecordInputChannels {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Stereo => "Stereo 1–2",
            Self::Mono1 => "Input 1 (Mono)",
            Self::Mono2 => "Input 2 (Mono)",
        }
    }

    pub const fn options(channel_count: usize) -> &'static [Self] {
        match channel_count {
            0 => &[],
            1 => &[Self::Mono1],
            _ => &[Self::Mono1, Self::Mono2, Self::Stereo],
        }
    }

    /// Resolve the legacy stereo default to the source's sole input when
    /// opening a mono device.
    pub const fn for_source(self, channel_count: usize) -> Self {
        match (self, channel_count) {
            (Self::Stereo, 1) => Self::Mono1,
            _ => self,
        }
    }

    pub const fn supported_by(self, channel_count: usize) -> bool {
        match self {
            Self::Mono1 => channel_count >= 1,
            Self::Mono2 | Self::Stereo => channel_count >= 2,
        }
    }

    fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Track {
    /// Must be unique within a project; ID assignment is owned by the editor.
    pub id: u64,
    pub name: String,
    pub kind: TrackKind,
    pub clips: Vec<Clip>,
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub session_slots: std::collections::BTreeMap<u64, Clip>,
    pub volume: f32,
    pub pan: f32,
    /// Optional routing to a non-nested group bus; old projects omit this.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group_id: Option<u64>,
    /// Session mixer state. Defaults preserve pre-mute/solo JSON projects.
    #[serde(default)]
    pub muted: bool,
    #[serde(default)]
    pub solo: bool,
    /// Built-in insert effects, in signal-flow order. Old sessions deserialize
    /// as a transparent empty rack.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub effects: Vec<BuiltinEffect>,
    /// Persisted capture source for this Audio track. `None` asks the app to
    /// choose the preferred physical input when a take starts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub record_input: Option<String>,
    /// Stable PulseAudio source name. Legacy projects may only store a label.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub record_input_id: Option<String>,
    #[serde(default, skip_serializing_if = "RecordInputChannels::is_default")]
    pub record_input_channels: RecordInputChannels,
    /// Optional isolated CLAP/VST3 insert. Bundle/class and optional opaque
    /// native state survive save/reopen, subject to plugin format support.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub live_clap: Option<ClapLiveInsert>,
    /// Built-in instrument rack assignment for MIDI tracks (SoundFont preset).
    /// `None` keeps the legacy built-in sine synth. Old projects deserialize as
    /// `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instrument: Option<BuiltinInstrument>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ClapLiveInsert {
    pub bundle: PathBuf,
    #[serde(default)]
    pub class_id: String,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    /// Opaque native plugin state captured from the isolated worker. Optional so
    /// projects written before state recall still load, and a missing or
    /// malformed blob never prevents the device (or project) from opening.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<PluginStateBlob>,
}

fn default_enabled() -> bool {
    true
}

impl Track {
    pub fn all_clips(&self) -> impl Iterator<Item = &Clip> {
        self.clips.iter().chain(self.session_slots.values())
    }
    pub fn all_clips_mut(&mut self) -> impl Iterator<Item = &mut Clip> {
        self.clips.iter_mut().chain(self.session_slots.values_mut())
    }
    pub fn playback_clips(&self, session: bool) -> impl Iterator<Item = &Clip> {
        self.clips
            .iter()
            .filter(move |_| !session)
            .chain(self.session_slots.values().filter(move |_| session))
    }

    pub fn new(id: u64, name: impl Into<String>, kind: TrackKind) -> Self {
        Self {
            id,
            name: name.into(),
            kind,
            clips: Vec::new(),
            session_slots: Default::default(),
            volume: 1.0,
            pan: 0.0,
            group_id: None,
            muted: false,
            solo: false,
            effects: Vec::new(),
            record_input: None,
            record_input_id: None,
            record_input_channels: RecordInputChannels::default(),
            live_clap: None,
            instrument: None,
        }
    }
}

/// Original Slate Studio insert devices. These are real DSP effects, not
/// external plugin instances and not copies of another product's assets.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BuiltinEffectKind {
    Eq3,
    Eq8,
    VocalReverb,
    GuitarReverb,
}

impl BuiltinEffectKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Eq3 => "EQ3",
            Self::Eq8 => "EQ8",
            Self::VocalReverb => "Vocal Reverb",
            Self::GuitarReverb => "Guitar Reverb",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BuiltinEffect {
    pub kind: BuiltinEffectKind,
    #[serde(default = "default_effect_enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub gains_db: [f32; 8],
    #[serde(default = "default_effect_wet")]
    pub wet: f32,
    #[serde(default = "default_effect_decay")]
    pub decay_s: f32,
    #[serde(default = "default_effect_tone")]
    pub tone: f32,
}

fn default_effect_enabled() -> bool {
    true
}
fn default_effect_wet() -> f32 {
    0.22
}
fn default_effect_decay() -> f32 {
    1.8
}
fn default_effect_tone() -> f32 {
    0.5
}

impl BuiltinEffect {
    pub fn new(kind: BuiltinEffectKind) -> Self {
        Self {
            kind,
            enabled: true,
            gains_db: [0.0; 8],
            wet: match kind {
                BuiltinEffectKind::GuitarReverb => 0.30,
                _ => 0.22,
            },
            decay_s: match kind {
                BuiltinEffectKind::GuitarReverb => 2.1,
                _ => 1.8,
            },
            tone: 0.5,
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
    /// Frame offset into decoded source for non-destructive clip splitting.
    /// Defaults to zero for projects saved before the beta editor.
    #[serde(default)]
    pub source_offset_frames: u64,
    /// Linear non-destructive clip gain. 1.0 = source unchanged.
    #[serde(default = "audio_unity_gain")]
    pub gain: f32,
    /// Clip fades are stored in milliseconds so rate negotiation cannot alter
    /// their durations when an interface switches 44.1 <-> 48 kHz.
    #[serde(default)]
    pub fade_in_ms: u32,
    #[serde(default)]
    pub fade_out_ms: u32,
}

fn audio_unity_gain() -> f32 {
    1.0
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
    fn old_audio_projects_keep_unity_gain_without_fades() {
        let old = r#"{"source":"old-recording.wav","warped":false,"source_offset_frames":0}"#;
        let audio: AudioClip = serde_json::from_str(old).unwrap();
        assert_eq!(audio.gain, 1.0);
        assert_eq!(audio.fade_in_ms, 0);
        assert_eq!(audio.fade_out_ms, 0);
        let serialized = serde_json::to_string(&audio).unwrap();
        let restored: AudioClip = serde_json::from_str(&serialized).unwrap();
        assert_eq!(restored, audio);
    }

    #[test]
    fn group_bus_routes_solo_and_mute_without_changing_old_json() {
        let mut p = Project::new("Groups", Tempo::default());
        let mut group = Track::new(1, "Rhythm", TrackKind::Group);
        group.volume = 0.6;
        let mut bass = Track::new(2, "Bass", TrackKind::Midi);
        bass.group_id = Some(1);
        let guitar = Track::new(3, "Guitar", TrackKind::Audio);
        p.tracks = vec![group, bass, guitar];
        assert!(p.track_audible(&p.tracks[1]));
        p.tracks[0].solo = true;
        assert!(p.track_audible(&p.tracks[1]));
        assert!(!p.track_audible(&p.tracks[2]));
        p.tracks[0].muted = true;
        assert!(!p.track_audible(&p.tracks[1]));
        assert!(p.validate_groups().is_ok());
        let encoded = serde_json::to_string(&p).unwrap();
        let round_trip: Project = serde_json::from_str(&encoded).unwrap();
        assert_eq!(round_trip.tracks[1].group_id, Some(1));
        assert_eq!(round_trip.tracks[0].kind, TrackKind::Group);
        // Existing projects did not store group_id and must keep working.
        let old = encoded.replace(",\"group_id\":1", "");
        let restored: Project = serde_json::from_str(&old).unwrap();
        assert_eq!(restored.tracks[1].group_id, None);
        p.tracks[1].group_id = Some(123_456);
        assert!(p.validate_groups().is_err());
        p.tracks[1].group_id = Some(1);
        p.tracks[0].group_id = Some(2);
        assert!(p.validate_groups().is_err());
    }

    #[test]
    fn project_initializes_without_audio_io() {
        let mut p = Project::new("Untitled", Tempo::default());
        p.tracks.push(Track::new(42, "Piano", TrackKind::Midi));
        assert_eq!(p.track_mut(42).unwrap().name, "Piano");
        assert!(p.track_mut(17).is_none());
        assert_eq!(p.time_signature, TimeSignature::default());
    }
}

#[cfg(test)]
mod new_effect_persistence_tests {
    use super::*;

    #[test]
    fn session_round_trip_saves_effect_chain_and_click_state() {
        let mut project = Project::new("Original FX", Tempo::default());
        let mut vocals = Track::new(1, "Lead", TrackKind::Audio);
        let mut eq = BuiltinEffect::new(BuiltinEffectKind::Eq8);
        eq.gains_db[1] = -6.5;
        vocals.effects.push(eq);
        let mut reverb = BuiltinEffect::new(BuiltinEffectKind::VocalReverb);
        reverb.enabled = false;
        reverb.wet = 0.73;
        vocals.effects.push(reverb);
        project.tracks.push(vocals);
        project.metronome_enabled = true;
        project.metronome_volume = 0.35;
        let serialized = serde_json::to_string(&project).unwrap();
        let recovered: Project = serde_json::from_str(&serialized).unwrap();
        assert_eq!(recovered, project);
        assert!(recovered.metronome_enabled);
        assert_eq!(recovered.metronome_volume, 0.35);
        assert_eq!(recovered.tracks[0].effects.len(), 2);
        assert_eq!(recovered.tracks[0].effects[0].gains_db[1], -6.5);
        assert!(!recovered.tracks[0].effects[1].enabled);
        assert_eq!(recovered.tracks[0].effects[1].wet, 0.73);
    }

    #[test]
    fn old_projects_remain_compatible_without_effects_or_click_metadata() {
        let mut project = Project::new("Old session", Tempo::default());
        project
            .tracks
            .push(Track::new(4, "Old track", TrackKind::Midi));
        let value = serde_json::to_value(&project).unwrap();
        let mut old = value.as_object().unwrap().clone();
        old.remove("metronome_enabled");
        old.remove("metronome_volume");
        for track in old["tracks"].as_array_mut().unwrap() {
            track.as_object_mut().unwrap().remove("effects");
            track.as_object_mut().unwrap().remove("record_input");
        }
        let recovered: Project = serde_json::from_value(serde_json::Value::Object(old)).unwrap();
        assert!(!recovered.metronome_enabled);
        assert_eq!(recovered.metronome_volume, 1.0);
        assert!(recovered.tracks[0].effects.is_empty());
        assert!(recovered.tracks[0].record_input.is_none());
        assert!(recovered.tracks[0].record_input_id.is_none());
        assert_eq!(
            recovered.tracks[0].record_input_channels,
            RecordInputChannels::Stereo
        );
        assert!(recovered.tracks[0].instrument.is_none());
    }
}

#[cfg(test)]
mod recording_input_persistence_tests {
    use super::*;

    #[test]
    fn audio_track_capture_source_and_channel_survive_save_and_reopen() {
        let mut project = Project::new("Tracking session", Tempo::default());
        let mut track = Track::new(50, "Vocal", TrackKind::Audio);
        track.record_input = Some("AIR 192 6 Analog Stereo".into());
        track.record_input_id = Some("test-capture-device".into());
        track.record_input_channels = RecordInputChannels::Mono2;
        project.tracks.push(track);

        let text = serde_json::to_string(&project).unwrap();
        let loaded: Project = serde_json::from_str(&text).unwrap();

        assert_eq!(
            loaded.tracks[0].record_input.as_deref(),
            Some("AIR 192 6 Analog Stereo")
        );
        assert_eq!(
            loaded.tracks[0].record_input_id.as_deref(),
            Some("test-capture-device")
        );
        assert_eq!(
            loaded.tracks[0].record_input_channels,
            RecordInputChannels::Mono2
        );
    }

    #[test]
    fn recording_channel_options_follow_source_input_count() {
        use RecordInputChannels::{Mono1, Mono2, Stereo};

        assert!(RecordInputChannels::options(0).is_empty());
        assert_eq!(RecordInputChannels::options(1), &[Mono1]);
        assert_eq!(RecordInputChannels::options(2), &[Mono1, Mono2, Stereo]);
        assert_eq!(Stereo.for_source(1), Mono1);
        assert_eq!(Stereo.for_source(2), Stereo);
        assert!(Mono1.supported_by(1));
        assert!(!Mono2.supported_by(1));
        assert_eq!(Mono2.for_source(1), Mono2);
    }
}

#[cfg(test)]
mod live_clap_persistence_tests {
    use super::*;

    #[test]
    fn optional_live_clap_insert_survives_save_reopen_without_breaking_old_projects() {
        let mut project = Project::new("Live CLAP session", Tempo::default());
        let mut track = Track::new(100, "Guitar", TrackKind::Audio);
        track.live_clap = Some(ClapLiveInsert {
            bundle: PathBuf::from("/usr/lib/clap/lsp-plugins.clap"),
            class_id: "in.lsp-plug.compressor_stereo".into(),
            enabled: true,
            state: Some(
                crate::plugin_state::PluginStateBlob::from_bytes(
                    crate::plugin_state::FORMAT_CLAP,
                    "in.lsp-plug.compressor_stereo",
                    48_000,
                    b"native-state",
                )
                .unwrap(),
            ),
        });
        project.tracks.push(track);
        let text = serde_json::to_string(&project).unwrap();
        let loaded: Project = serde_json::from_str(&text).unwrap();
        assert_eq!(loaded, project);
        assert_eq!(
            loaded.tracks[0].live_clap.as_ref().unwrap().class_id,
            "in.lsp-plug.compressor_stereo"
        );
        assert_eq!(
            loaded.tracks[0]
                .live_clap
                .as_ref()
                .unwrap()
                .state
                .as_ref()
                .unwrap()
                .decode(crate::plugin_state::FORMAT_CLAP)
                .unwrap(),
            b"native-state"
        );
        // A previous-generation project without live_clap metadata must still
        // open with the original dry audio path intact.
        let mut old = serde_json::to_value(&project).unwrap();
        old["tracks"][0]
            .as_object_mut()
            .unwrap()
            .remove("live_clap");
        let old_project: Project = serde_json::from_value(old).unwrap();
        assert!(old_project.tracks[0].live_clap.is_none());
    }
}
