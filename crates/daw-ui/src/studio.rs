use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, atomic::Ordering},
    time::Duration,
};

use daw_core::{
    AudioClip, BuiltinEffect, BuiltinEffectKind, BuiltinInstrument, Clip, ClipKind, DEFAULT_PRESET,
    InstrumentCategory, InstrumentPreset, MidiClip, MidiNote, MusicalTime, PPQ, PRESETS,
    PitchClass, Project, ScaleDefinition, ScaleMode, Tempo, TimeSignature, Track, TrackKind,
    resolve_preset,
};
use daw_engine::{
    AudioDriver, DriverConfig, EngineCommand, GraphBuilder, NodeSpec, SoundFontBank,
    SoundFontHandle, SoundFontInstrument, SoundFontTone, Source, StereoFrame,
    build_synth_with_tone, decode_to_source, driver::Backend, effects::EffectRack,
    live_plugin::LivePluginControl, load_sound_font, monitor::RecordingMonitor,
};
use eframe::egui::{self, Color32, FontId, Pos2, Rect, Sense, Stroke, Vec2};

const MASTER_ID: u32 = 900_000;
const METRONOME_ID: u32 = 990_000;
const RECORD_MONITOR_ID: u32 = 990_001;
const AUDIO_OFFSET: u32 = 10_000;
const MIDI_OFFSET: u32 = 500_000;
const GROUP_OFFSET: u32 = 930_000;
const LIVE_BUS_OFFSET: u32 = 850_000;
const MAX_CLIPS: usize = 48;
type PluginScanResult = Result<Vec<PluginClass>, String>;

#[derive(Clone)]
struct PreparedAudio {
    samples: Arc<[StereoFrame]>,
    peaks: Vec<f32>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum View {
    Arrangement,
    Session,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Detail {
    Clip,
    Mixer,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum BrowserSection {
    All,
    Instruments,
    Effects,
    Plugins,
    Samples,
    Places,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PianoGrid {
    Quarter,
    Eighth,
    Sixteenth,
    ThirtySecond,
}
impl PianoGrid {
    fn ticks(self) -> i64 {
        match self {
            Self::Quarter => PPQ,
            Self::Eighth => PPQ / 2,
            Self::Sixteenth => PPQ / 4,
            Self::ThirtySecond => PPQ / 8,
        }
    }
    fn label(self) -> &'static str {
        match self {
            Self::Quarter => "1/4",
            Self::Eighth => "1/8",
            Self::Sixteenth => "1/16",
            Self::ThirtySecond => "1/32",
        }
    }
}

/// Bar length in musical ticks for any numerator/denominator. 6/8 is three
/// quarter notes, not six, so rulers and snap helpers share this one rule.
fn bar_ticks_for(signature: TimeSignature) -> i64 {
    let denominator = i64::from(signature.denominator).max(1);
    i64::from(signature.numerator).saturating_mul(PPQ * 4) / denominator
}

pub struct Studio {
    project: Project,
    history: ProjectHistory,
    last_autosaved_project: Project,
    last_autosave: Instant,
    autosave_job: Option<std::sync::mpsc::Receiver<Result<PathBuf, String>>>,
    media: HashMap<PathBuf, PreparedAudio>,
    driver: Option<AudioDriver>,
    backend: Backend,
    show_devices: bool,
    device_tab: DeviceTab,
    capture_devices: Option<(Instant, Result<Vec<CaptureSource>, String>)>,
    input_probe: Option<InputProbe>,
    fx_rebuild_pending: bool,
    live_plugin_errors: HashMap<u64, String>,
    live_plugin_runtimes: HashMap<u64, Vec<LivePluginRuntime>>,
    /// UI-side handles to live worker instances, used only for native-state
    /// capture at project save. The audio callback never touches this map.
    live_clap_controls: HashMap<u64, LivePluginControl>,
    show_tutorial: bool,
    recorder: Option<ActiveRecording>,
    input_monitor_enabled: bool,
    /// Monitor ring allocated before a take starts so enabling direct
    /// monitoring does not require a post-arm transport restart.
    pending_monitor: Option<(u64, Arc<RecordingMonitor>)>,
    midi_connection: Option<MidiInputConnection<()>>,
    midi_receiver: Option<Receiver<MidiInputEvent>>,
    midi_name: Option<String>,
    midi_held: HashMap<(u8, u8), u32>,
    midi_sustain_node: [Option<u32>; 16],
    midi_pending_panics: std::collections::BTreeSet<u32>,
    midi_overflow: Arc<std::sync::atomic::AtomicBool>,
    midi_take: Option<MidiTake>,
    /// Built-in SoundFont instrument rack state.
    soundfont_banks: Vec<PathBuf>,
    soundfonts: HashMap<String, SoundFontBank>,
    soundfont_loads: HashMap<String, std::sync::mpsc::Receiver<Result<SoundFontBank, String>>>,
    soundfont_handles: HashMap<u32, SoundFontHandle>,
    soundfont_error: Option<String>,
    soundfont_rebuild_pending: bool,
    soundfont_ready: bool,
    keyboard_octave: i8,
    instrument_tones: HashMap<u64, SoundFontTone>,
    keyboard_mouse_note: Option<(u32, u8)>,
    keyboard_held: HashMap<egui::Key, (u32, u8)>,
    sample_rate: u32,
    playhead_frame: u64,
    seek_preview_frame: Option<u64>,
    arrangement_seek_revision: u64,
    pending_arrangement_seek: Option<u64>,
    arrangement_plain_press: bool,
    playing: bool,
    watchdog_last_callbacks: u64,
    watchdog_last_change: Instant,
    selected_track: Option<u64>,
    selected_clip: Option<u64>,
    arrangement_selection: Vec<u64>,
    next_id: u64,
    view: View,
    detail: Detail,
    track_rename: Option<(u64, String)>,
    track_delete: Option<u64>,
    clip_delete: Option<(u64, u64)>,
    fold: bool,
    scale_highlight: bool,
    scale_lock: bool,
    piano_grid: PianoGrid,
    piano_note_length: PianoGrid,
    piano_selected: Option<usize>,
    piano_scroll_clip: Option<u64>,
    piano_resize_note: Option<usize>,
    piano_velocity_drag: Option<usize>,
    piano_velocity_drag_value: Option<u8>,
    browser_section: BrowserSection,
    browser_ready: bool,
    browser_width: f32,
    sample_roots: Vec<PathBuf>,
    sample_paths: Vec<PathBuf>,
    plugin_paths: Vec<PathBuf>,
    plugin_selected: Option<PathBuf>,
    plugin_classes: Vec<PluginClass>,
    selected_class_id: Option<String>,
    plugin_scan_job: Option<(PathBuf, mpsc::Receiver<PluginScanResult>)>,
    plugin_scan_error: Option<String>,
    plugin_job: Option<std::sync::mpsc::Receiver<Result<PathBuf, String>>>,
    export_job: Option<std::sync::mpsc::Receiver<Result<(PathBuf, f32), String>>>,
    session_active: HashMap<u64, u64>,
    playback_view: View,
    session_meters: HashMap<u64, Arc<daw_engine::session::SlotMeter>>,
    project_path: Option<PathBuf>,
    search: String,
    error: Option<String>,
    status: String,
    zoom: f32,
    snap: bool,
    clip_drag_preview: Option<(u64, f32)>,
    clip_drag_origin_x: Option<f32>,
    clip_resize_drag: Option<ClipTrimEdge>,
    clip_resize_preview: Option<(u64, f32)>,
    clip_clipboard: Vec<(u64, Clip)>,
    events: Vec<(u64, EngineCommand)>,
    next_event: usize,
    play_project_start: u64,
    dirty: bool,
}

impl Studio {
    /// Read the actual default output sink's sample rate, not an unrelated
    /// microphone's rate or the daemon's generic default specification.
    /// PulseAudio can silently stop callback delivery at a mismatched rate.
    /// Parsing is isolated for deterministic tests without hardware.
    fn default_sink_rate(info: &str, sinks: &str) -> Option<u32> {
        let default_name = info
            .lines()
            .find_map(|line| line.trim().strip_prefix("Default Sink: "))?
            .trim();
        sinks.lines().find_map(|line| {
            let fields = line.split('\t').collect::<Vec<_>>();
            if fields.get(1).map(|name| name.trim()) != Some(default_name) {
                return None;
            }
            let rate = fields
                .get(3)?
                .split_whitespace()
                .last()?
                .trim_end_matches("Hz")
                .parse::<u32>()
                .ok()?;
            [44_100, 48_000, 96_000].contains(&rate).then_some(rate)
        })
    }

    pub(crate) fn negotiate_sample_rate() -> u32 {
        if let Ok(forced) = std::env::var("RUSTIE_SAMPLE_RATE")
            && let Ok(rate) = forced.parse::<u32>()
            && [44_100, 48_000, 96_000].contains(&rate)
        {
            return rate;
        }
        let (Ok(info), Ok(sinks)) = (
            Command::new("pactl").arg("info").output(),
            Command::new("pactl")
                .args(["list", "short", "sinks"])
                .output(),
        ) else {
            return 48_000;
        };
        if !info.status.success() || !sinks.status.success() {
            return 48_000;
        }
        Self::default_sink_rate(
            &String::from_utf8_lossy(&info.stdout),
            &String::from_utf8_lossy(&sinks.stdout),
        )
        .unwrap_or(48_000)
    }

    fn tutorial_marker() -> PathBuf {
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_default()
            .join(".config/rustie/tutorial-seen")
    }

    fn dismiss_tutorial(&mut self) {
        self.show_tutorial = false;
        let marker = Self::tutorial_marker();
        if let Some(parent) = marker.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _ = fs::write(marker, b"1");
    }

    fn bar_ticks(&self) -> i64 {
        bar_ticks_for(self.project.time_signature)
    }

    /// Playhead shown to the user: the engine's generated frame minus the
    /// output stream's predicted callback-to-playback delay. Clamped at zero.
    fn displayed_playhead_frame(generated: u64, delay_us: u64, rate: u32) -> u64 {
        generated.saturating_sub(delay_us.saturating_mul(u64::from(rate)) / 1_000_000)
    }

    fn beats_per_bar(&self) -> f64 {
        self.project.time_signature.numerator as f64 * 4.0
            / self.project.time_signature.denominator as f64
    }

    fn seek_to_frame(&mut self, frame: u64) {
        if self.recorder.is_some() || self.midi_take.is_some() {
            self.error = Some(
                "Stop the current take before seeking: moving the transport while armed would misalign the recording."
                    .into(),
            );
            return;
        }
        self.seek_preview_frame = None;
        if self.playing && self.playback_view == View::Arrangement {
            let revision = self.arrangement_seek_revision.wrapping_add(1);
            if self.driver.as_mut().is_none_or(|driver| {
                driver
                    .controller()
                    .seek_arrangement(frame, revision)
                    .is_err()
            }) {
                self.error =
                    Some("Audio command queue is full; seek was not applied. Try again.".into());
                return;
            }
            self.arrangement_seek_revision = revision;
            self.pending_arrangement_seek = Some(frame);
            self.playhead_frame = frame;
            self.prepare_arrangement_midi(frame);
            self.schedule_midi_ahead();
        } else {
            self.playhead_frame = frame;
            self.restart_for_edit();
        }
        self.status = format!("Seek: {}", self.time_display());
    }

    fn prepare_arrangement_midi(&mut self, frame: u64) {
        self.next_event = self.events.partition_point(|(at, _)| *at < frame);
        self.play_project_start = frame;
        // Chase notes spanning the destination. Their original NoteOff remains
        // scheduled, and the seek barrier has already silenced the old voices.
        let mut active = std::collections::BTreeMap::new();
        for &(_, command) in &self.events[..self.next_event] {
            match command {
                EngineCommand::NoteOn {
                    node,
                    note,
                    velocity,
                } => {
                    active.insert((node, note), velocity);
                }
                EngineCommand::NoteOff { node, note } => {
                    active.remove(&(node, note));
                }
                _ => {}
            }
        }
        if let Some(driver) = self.driver.as_mut() {
            for ((node, note), velocity) in active {
                if driver
                    .controller()
                    .schedule_arrangement(
                        frame,
                        EngineCommand::NoteOn {
                            node,
                            note,
                            velocity,
                        },
                    )
                    .is_err()
                {
                    self.error =
                        Some("Audio command queue is full while restoring MIDI notes.".into());
                    break;
                }
            }
        }
    }

    pub fn new(cc: &eframe::CreationContext<'_>, initial_project_path: Option<PathBuf>) -> Self {
        install_midnight_ember_theme(&cc.egui_ctx);
        let mut studio = Self::with_sample_rate(Self::negotiate_sample_rate());
        studio.discover_soundfonts();
        if let Some(path) = initial_project_path {
            studio.load_project_path(path);
        }
        studio
    }

    fn with_sample_rate(sample_rate: u32) -> Self {
        let mut project = Project::new("Untitled Session", Tempo::default());
        project.global_scale = Some(ScaleDefinition::new(PitchClass::C, ScaleMode::Major));
        let mut track = Track::new(1, "Demo Sine Synth", TrackKind::Midi);
        let pitches = [60, 64, 67, 72, 67, 64, 62, 60];
        let notes = pitches
            .into_iter()
            .enumerate()
            .map(|(i, pitch)| MidiNote {
                pitch,
                start: MusicalTime::from_ticks(i as i64 * PPQ),
                duration: MusicalTime::from_ticks(PPQ * 3 / 4),
                velocity: 100,
                muted: false,
            })
            .collect();
        track.clips.push(Clip {
            id: 2,
            start: MusicalTime::ZERO,
            duration: MusicalTime::from_ticks(8 * PPQ),
            kind: ClipKind::Midi(MidiClip { notes }),
            scale_override: None,
        });
        project.tracks.push(track);

        // A small, genuinely playable three-track starter song. These are
        // live MIDI clips routed to the built-in synth, not decorative UI data.
        let mut bass = Track::new(3, "Sine Bassline", TrackKind::Midi);
        bass.volume = 0.34;
        let bass_notes = [36_u8, 36, 43, 43, 41, 41, 43, 43]
            .into_iter()
            .enumerate()
            .map(|(i, pitch)| MidiNote {
                pitch,
                start: MusicalTime::from_ticks(i as i64 * PPQ),
                duration: MusicalTime::from_ticks(PPQ * 7 / 8),
                velocity: 73,
                muted: false,
            })
            .collect();
        bass.clips.push(Clip {
            id: 4,
            start: MusicalTime::ZERO,
            duration: MusicalTime::from_ticks(8 * PPQ),
            kind: ClipKind::Midi(MidiClip { notes: bass_notes }),
            scale_override: None,
        });
        project.tracks.push(bass);

        let mut chords = Track::new(5, "Sine Chords", TrackKind::Midi);
        chords.volume = 0.22;
        let chord_notes = [
            (0_i64, [60_u8, 64, 67]),
            (2_i64, [65_u8, 69, 72]),
            (4_i64, [67_u8, 71, 74]),
            (6_i64, [60_u8, 64, 67]),
        ]
        .into_iter()
        .flat_map(|(beat, pitches)| {
            pitches.into_iter().map(move |pitch| MidiNote {
                pitch,
                start: MusicalTime::from_ticks(beat * PPQ),
                duration: MusicalTime::from_ticks(2 * PPQ - PPQ / 8),
                velocity: 55,
                muted: false,
            })
        })
        .collect();
        chords.clips.push(Clip {
            id: 6,
            start: MusicalTime::ZERO,
            duration: MusicalTime::from_ticks(8 * PPQ),
            kind: ClipKind::Midi(MidiClip { notes: chord_notes }),
            scale_override: None,
        });
        project.tracks.push(chords);

        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_default();
        project.session = None;
        project.prepare_session().expect("valid starter Session");
        let next_id = project.next_object_id().unwrap();
        let mut studio = Self {
            history: ProjectHistory::new(&project),
            last_autosaved_project: project.clone(),
            last_autosave: Instant::now(),
            autosave_job: None,
            project,
            media: HashMap::new(),
            driver: None,
            // Desktop PulseAudio usually owns the physical audio interface;
            // direct ALSA often opens a discovered but busy device.
            backend: Backend::PulseAudio,
            show_devices: false,
            device_tab: DeviceTab::Effects,
            capture_devices: None,
            input_probe: None,
            fx_rebuild_pending: false,
            live_plugin_errors: HashMap::new(),
            live_plugin_runtimes: HashMap::new(),
            live_clap_controls: HashMap::new(),
            show_tutorial: !Self::tutorial_marker().exists(),
            recorder: None,
            input_monitor_enabled: false,
            pending_monitor: None,
            midi_connection: None,
            midi_receiver: None,
            midi_name: None,
            midi_held: HashMap::new(),
            midi_sustain_node: [None; 16],
            midi_pending_panics: std::collections::BTreeSet::new(),
            midi_overflow: Arc::default(),
            midi_take: None,
            soundfont_banks: Vec::new(),
            soundfonts: HashMap::new(),
            soundfont_loads: HashMap::new(),
            soundfont_handles: HashMap::new(),
            soundfont_error: None,
            soundfont_rebuild_pending: false,
            soundfont_ready: false,
            keyboard_octave: 0,
            instrument_tones: HashMap::new(),
            keyboard_mouse_note: None,
            keyboard_held: HashMap::new(),
            sample_rate,
            playhead_frame: 0,
            seek_preview_frame: None,
            arrangement_seek_revision: 0,
            pending_arrangement_seek: None,
            arrangement_plain_press: false,
            playing: false,
            watchdog_last_callbacks: 0,
            watchdog_last_change: Instant::now(),
            selected_track: Some(1),
            selected_clip: Some(2),
            arrangement_selection: Vec::new(),
            next_id,
            view: if std::env::args().any(|arg| arg == "--session") {
                View::Session
            } else {
                View::Arrangement
            },
            detail: if std::env::args().any(|arg| arg == "--session") {
                Detail::Mixer
            } else {
                Detail::Clip
            },
            track_rename: None,
            track_delete: None,
            clip_delete: None,
            fold: false,
            scale_highlight: true,
            scale_lock: false,
            piano_grid: PianoGrid::Sixteenth,
            piano_note_length: PianoGrid::Quarter,
            piano_selected: None,
            piano_scroll_clip: None,
            piano_resize_note: None,
            piano_velocity_drag: None,
            piano_velocity_drag_value: None,
            browser_section: BrowserSection::Instruments,
            browser_ready: false,
            browser_width: 252.0,
            sample_roots: vec![
                home.join("Music"),
                home.join("Samples"),
                home.join("Downloads"),
            ],
            sample_paths: Vec::new(),
            plugin_paths: Vec::new(),
            plugin_selected: None,
            plugin_classes: Vec::new(),
            selected_class_id: None,
            plugin_scan_job: None,
            plugin_scan_error: None,
            plugin_job: None,
            export_job: None,
            session_active: HashMap::new(),
            playback_view: View::Arrangement,
            session_meters: HashMap::new(),
            project_path: None,
            search: String::new(),
            error: None,
            status: "Ready • Start with ▶ Play to hear the built-in MIDI demo".into(),
            zoom: 56.0,
            snap: true,
            clip_drag_preview: None,
            clip_drag_origin_x: None,
            clip_resize_drag: None,
            clip_resize_preview: None,
            clip_clipboard: Vec::new(),
            events: Vec::new(),
            next_event: 0,
            play_project_start: 0,
            dirty: false,
        };
        if studio.view == View::Session {
            studio.selected_clip = studio.project.tracks[0]
                .session_slots
                .values()
                .next()
                .map(|c| c.id);
        }
        studio
    }

    fn apply_error<T>(&mut self, result: Result<T, String>) -> Option<T> {
        match result {
            Ok(v) => {
                self.error = None;
                Some(v)
            }
            Err(err) => {
                self.status = "Operation failed".into();
                self.error = Some(err);
                None
            }
        }
    }

    fn pick_path(save: bool, audio: bool) -> Option<PathBuf> {
        let mut args = vec!["--file-selection", "--title"];
        let title = if audio {
            "Choose an audio file"
        } else if save {
            "Save Rusty project"
        } else {
            "Open Rusty project"
        };
        args.push(title);
        if save {
            args.extend([
                "--save",
                "--confirm-overwrite",
                "--filename=Untitled.rusty.json",
            ]);
        } else if audio {
            args.push("--file-filter=Audio files | *.wav *.flac *.mp3 *.ogg *.m4a *.aiff");
        } else {
            args.push("--file-filter=Rusty projects | *.rusty.json *.slate.json *.json");
        }
        let output = Command::new("zenity").args(&args).output().ok()?;
        if !output.status.success() {
            return None;
        }
        let name = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if name.is_empty() {
            None
        } else {
            Some(PathBuf::from(name))
        }
    }

    fn decode_cached(&mut self, path: &Path) -> Result<(), String> {
        if self.media.contains_key(path) {
            return Ok(());
        }
        let source = decode_to_source(path, self.sample_rate, 0)
            .map_err(|e| format!("{}: {e}", path.display()))?;
        let Source::Samples { frames, .. } = source else {
            return Err("Audio decoder did not return sample data".into());
        };
        let step = (frames.len() / 1200).max(1);
        let peaks = frames
            .chunks(step)
            .map(|block| {
                block
                    .iter()
                    .fold(0.0_f32, |peak, f| peak.max(f.left.abs()).max(f.right.abs()))
            })
            .collect();
        self.media.insert(
            path.to_path_buf(),
            PreparedAudio {
                samples: frames,
                peaks,
            },
        );
        Ok(())
    }

    /// Imports into the selected AUDIO lane, if any. A user-created empty
    /// "Vocals" track should receive the dragged WAV and show its waveform;
    /// silently creating another track is surprising and looks like a failure.
    fn import_audio(&mut self, path: PathBuf) {
        let selected_audio = self.selected_track.filter(|id| {
            self.project
                .tracks
                .iter()
                .any(|t| t.id == *id && t.kind == TrackKind::Audio)
        });
        if let Some(track_id) = selected_audio {
            self.session_append_audio(track_id, path);
        } else {
            self.add_audio(path);
        }
    }

    fn add_audio(&mut self, path: PathBuf) {
        if self.visible_clip_count() >= MAX_CLIPS || self.project.tracks.len() >= 48 {
            self.error = Some(format!("Prototype limit: at most {MAX_CLIPS} clips"));
            return;
        }
        let decoded = self.decode_cached(&path);
        if self.apply_error(decoded).is_none() {
            return;
        }
        let Some(cached) = self.media.get(&path) else {
            return;
        };
        let duration_seconds = cached.samples.len() as f64 / self.sample_rate as f64;
        let ticks = (duration_seconds
            * self
                .project
                .tempo_map
                .tempo_at(MusicalTime::ZERO)
                .map_or(120.0, |t| t.bpm())
            / 60.0
            * PPQ as f64)
            .round()
            .max(1.0) as i64;
        let track_id = self.next_id;
        self.next_id += 1;
        let clip_id = self.next_id;
        self.next_id += 1;
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Audio")
            .to_string();
        let mut track = Track::new(track_id, name, TrackKind::Audio);
        let clip = Clip {
            id: clip_id,
            start: MusicalTime::ZERO,
            duration: MusicalTime::from_ticks(ticks),
            kind: ClipKind::Audio(AudioClip {
                source: path.clone(),
                warped: false,
                source_offset_frames: 0,
                gain: 1.0,
                fade_in_ms: 0,
                fade_out_ms: 0,
            }),
            scale_override: None,
        };
        if self.view == View::Session {
            track
                .session_slots
                .insert(self.project.session.as_ref().unwrap().scenes[0], clip);
        } else {
            track.clips.push(clip);
        }

        self.project.tracks.push(track);
        self.selected_track = Some(track_id);
        self.selected_clip = Some(clip_id);
        self.dirty = true;
        self.status = format!("Imported {} ({duration_seconds:.1}s)", path.display());
        self.restart_for_edit();
    }

    // Recording targets Arrangement even when the Session grid is visible.
    fn clip_count(&self) -> usize {
        self.project.tracks.iter().map(|t| t.clips.len()).sum()
    }

    fn visible_clip_count(&self) -> usize {
        self.project
            .tracks
            .iter()
            .map(|t| {
                if self.view == View::Session {
                    t.session_slots.len()
                } else {
                    t.clips.len()
                }
            })
            .sum()
    }

    fn add_audio_track(&mut self) {
        if self.project.tracks.len() >= 48 {
            self.error = Some("Maximum 48 tracks and groups".into());
            return;
        }
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        self.project
            .tracks
            .push(Track::new(id, format!("Audio {id}"), TrackKind::Audio));
        self.selected_track = Some(id);
        self.selected_clip = None;
        self.dirty = true;
        self.status = "Audio track created • right-click its name to import a clip".into();
        self.restart_for_edit();
    }

    fn add_midi_track(&mut self) {
        if self.visible_clip_count() >= MAX_CLIPS || self.project.tracks.len() >= 48 {
            self.error = Some("Clip limit reached".into());
            return;
        }
        let track_id = self.next_id;
        self.next_id += 1;
        let clip_id = self.next_id;
        self.next_id += 1;
        let mut track = Track::new(track_id, format!("MIDI {track_id}"), TrackKind::Midi);
        let clip = Clip {
            id: clip_id,
            start: MusicalTime::ZERO,
            duration: MusicalTime::from_ticks(16 * PPQ),
            kind: ClipKind::Midi(MidiClip::default()),
            scale_override: None,
        };
        if self.view == View::Session {
            track
                .session_slots
                .insert(self.project.session.as_ref().unwrap().scenes[0], clip);
        } else {
            track.clips.push(clip);
        }

        self.project.tracks.push(track);
        self.selected_track = Some(track_id);
        self.selected_clip = Some(clip_id);
        self.dirty = true;
        self.restart_for_edit();
    }

    fn track_midi_node(id: u64) -> Result<u32, String> {
        u32::try_from(id)
            .ok()
            .and_then(|n| n.checked_add(MIDI_OFFSET))
            .filter(|id| *id < MASTER_ID)
            .ok_or("MIDI track ID out of range".into())
    }

    fn group_node(id: u64) -> Result<u32, String> {
        u32::try_from(id)
            .ok()
            .and_then(|n| n.checked_add(GROUP_OFFSET))
            .ok_or("Group ID out of range".into())
    }

    fn live_audio_bus_node(id: u64) -> Result<u32, String> {
        u32::try_from(id)
            .ok()
            .and_then(|id| id.checked_add(LIVE_BUS_OFFSET))
            .filter(|id| *id < MASTER_ID)
            .ok_or("Live CLAP audio bus ID out of range".into())
    }

    fn clip_audio_node(id: u64) -> Result<u32, String> {
        u32::try_from(id)
            .ok()
            .and_then(|n| n.checked_add(AUDIO_OFFSET))
            .filter(|id| *id < MIDI_OFFSET)
            .ok_or("Audio clip ID out of range".into())
    }

    fn compiled_graph(&mut self) -> Result<daw_engine::AudioGraph, String> {
        self.compiled_graph_with_live(true)
    }

    fn compiled_graph_with_live(
        &mut self,
        enable_live: bool,
    ) -> Result<daw_engine::AudioGraph, String> {
        if enable_live {
            // Graph rebuild replaces every worker; old control handles would
            // address dead children and must not be reused for state capture.
            self.live_plugin_errors.clear();
            self.live_plugin_runtimes.clear();
            self.live_clap_controls.clear();
            self.soundfont_handles.clear();
        }
        let session = self.view == View::Session;
        let clock = if session {
            Some(Arc::new(daw_engine::session::SessionClock::new(
                &self.project.tempo_map,
                self.sample_rate,
            )?))
        } else {
            None
        };
        if enable_live {
            self.session_meters.clear();
        }
        let paths = self
            .project
            .tracks
            .iter()
            .flat_map(|t| t.playback_clips(session))
            .filter_map(|c| match &c.kind {
                ClipKind::Audio(audio) => Some(audio.source.clone()),
                _ => None,
            })
            .collect::<Vec<_>>();
        for path in paths {
            self.decode_cached(&path)?;
        }
        if self.visible_clip_count() > MAX_CLIPS {
            return Err("Too many clips".into());
        }
        let mut builder = GraphBuilder::new(self.sample_rate);
        let bpm = self
            .project
            .tempo_map
            .tempo_at(MusicalTime::ZERO)
            .map_or(120.0, |tempo| tempo.bpm());
        let mut click = NodeSpec::audio(
            METRONOME_ID,
            Source::Metronome {
                bpm,
                beats_per_bar: self.project.time_signature.numerator,
                beat_denominator: self.project.time_signature.denominator,
            },
        );
        click.gain = if self.project.metronome_enabled {
            self.project.metronome_volume.clamp(0.0, 1.0)
        } else {
            0.0
        };
        builder.add_node(click);
        let mut routes = vec![(METRONOME_ID, MASTER_ID)];
        let mut live_buses = HashMap::<u64, u32>::new();
        for group in self
            .project
            .tracks
            .iter()
            .filter(|t| t.kind == TrackKind::Group)
        {
            let node_id = Self::group_node(group.id)?;
            let mut bus = NodeSpec::returns(node_id);
            bus.gain = if group.muted { 0.0 } else { group.volume };
            bus.pan = group.pan;
            bus.effects = EffectRack::prepare(&group.effects, self.sample_rate);
            if enable_live
                && let Some(insert) = group.live_clap.as_ref()
                && insert.enabled
            {
                match prepare_live_clap(insert, self.sample_rate) {
                    Ok((effect, control, runtime)) => {
                        self.live_plugin_runtimes.insert(group.id, vec![runtime]);
                        let plugin_latency = control.latency_samples() as usize;
                        self.live_clap_controls.insert(group.id, control);
                        bus.effects.add_live(effect);
                        bus.latency_samples = daw_engine::live_plugin::LIVE_DELAY + plugin_latency;
                    }
                    Err(err) => {
                        eprintln!("Rusty skipped unavailable group plug-in: {err}");
                        self.live_plugin_errors.insert(group.id, err);
                    }
                }
            }
            builder.add_node(bus);
            routes.push((node_id, MASTER_ID));
        }
        for (lane, track) in self.project.tracks.iter().enumerate() {
            let destination = self
                .project
                .group_of(track)
                .and_then(|parent| Self::group_node(parent.id).ok())
                .unwrap_or(MASTER_ID);
            match track.kind {
                TrackKind::Midi => {
                    // In Session every clip has an independent synth node.
                    // Preloading them avoids rebuilding/reopening the driver on launch.
                    let clip_ids: Vec<u64> = if self.view == View::Session {
                        // Dedicated monitor synth is separate from queued
                        // Session clip voices and never triggers by itself.
                        std::iter::once(track.id)
                            .chain(track.session_slots.values().map(|clip| clip.id))
                            .collect()
                    } else {
                        vec![track.id]
                    };
                    for clip_id in clip_ids {
                        let id = Self::track_midi_node(clip_id)?;
                        let mut spec = NodeSpec::midi(id);
                        spec.gain = self.project.source_gain(track);
                        if let Some(clock) = &clock
                            && let Some(clip) =
                                track.session_slots.values().find(|c| c.id == clip_id)
                        {
                            let origin = (self.session_active.get(&track.id) == Some(&clip.id))
                                .then_some(self.playhead_frame);
                            let voice = daw_engine::session::SessionVoice::new(
                                clock.clone(),
                                lane,
                                clip,
                                origin,
                            );
                            self.session_meters.insert(clip.id, voice.meter());
                            spec.session = Some(voice);
                        }
                        spec.pan = track.pan;
                        spec.effects = EffectRack::prepare(&track.effects, self.sample_rate);
                        let mut uses_live_instrument = false;
                        if enable_live
                            && let Some(insert) = track.live_clap.as_ref()
                            && insert.enabled
                        {
                            match prepare_live_instrument(insert, self.sample_rate) {
                                Ok((instrument, control, runtime)) => {
                                    self.live_plugin_runtimes
                                        .entry(track.id)
                                        .or_default()
                                        .push(runtime);
                                    // The external plugin is the sound source: no doubled sine.
                                    spec.source = Source::Silence;
                                    uses_live_instrument = true;
                                    spec.latency_samples = daw_engine::live_plugin::LIVE_DELAY
                                        + control.latency_samples() as usize;
                                    if clip_id == track.id {
                                        // State capture addresses the track's monitor
                                        // instance; transient Session voices restore the
                                        // same saved state independently.
                                        self.live_clap_controls.insert(track.id, control);
                                    }
                                    spec.effects.add_live(instrument);
                                }
                                Err(err) => {
                                    eprintln!("Rusty skipped unavailable instrument: {err}");
                                    self.live_plugin_errors.insert(track.id, err);
                                }
                            }
                        }
                        // Built-in SoundFont instrument engine. Loaded banks are
                        // embedded at graph build time; preset changes swap in
                        // through the lock-free handle without a rebuild.
                        if !uses_live_instrument
                            && let Some((bank, preset)) = self.track_instrument(track)
                        {
                            let tone = self.instrument_tone(track.id);
                            match build_synth_with_tone(
                                &bank,
                                self.sample_rate,
                                preset.channel,
                                preset.program,
                                tone,
                            ) {
                                Ok(synth) => {
                                    let (instrument, handle) =
                                        SoundFontInstrument::with_synth(synth, preset.channel);
                                    if enable_live {
                                        self.soundfont_handles.insert(id, handle);
                                    }
                                    spec.source = Source::SoundFont {
                                        instrument: Box::new(instrument),
                                    };
                                }
                                Err(err) => {
                                    self.soundfont_error = Some(err);
                                }
                            }
                        }
                        builder.add_node(spec);
                        routes.push((id, destination));
                    }
                }
                TrackKind::Audio => {
                    // One isolated CLAP worker per audio TRACK, not per clip.
                    // Existing individual clip DSP/gain feeds the shared bus.
                    let mut audio_destination = destination;
                    if enable_live
                        && let Some(insert) = track.live_clap.as_ref()
                        && insert.enabled
                    {
                        match prepare_live_clap(insert, self.sample_rate) {
                            Ok((effect, control, runtime)) => {
                                self.live_plugin_runtimes.insert(track.id, vec![runtime]);
                                let plugin_latency = control.latency_samples() as usize;
                                self.live_clap_controls.insert(track.id, control);
                                let node_id = Self::live_audio_bus_node(track.id)?;
                                let mut bus = NodeSpec::returns(node_id);
                                bus.effects.add_live(effect);
                                bus.latency_samples =
                                    daw_engine::live_plugin::LIVE_DELAY + plugin_latency;
                                builder.add_node(bus);
                                routes.push((node_id, destination));
                                live_buses.insert(track.id, node_id);
                                audio_destination = node_id;
                            }
                            Err(err) => {
                                eprintln!("Rusty skipped unavailable audio plug-in: {err}");
                                self.live_plugin_errors.insert(track.id, err);
                            }
                        }
                    }
                    for clip in track.playback_clips(session) {
                        let ClipKind::Audio(audio) = &clip.kind else {
                            continue;
                        };
                        let id = Self::clip_audio_node(clip.id)?;
                        let Some(prepared) = self.media.get(&audio.source) else {
                            return Err(format!("Audio not cached: {}", audio.source.display()));
                        };
                        let start_frame = self
                            .project
                            .tempo_map
                            .sample_frame_at(clip.start, self.sample_rate)
                            .map_err(|e| e.to_string())?;
                        let source = if self.view == View::Session {
                            let loop_frames = self.tick_to_frame(clip.duration.ticks()).max(1);
                            Source::LoopedSamples {
                                source_offset_frames: audio.source_offset_frames,
                                gain: audio.gain,
                                fade_in_frames: (audio.fade_in_ms as u64 * self.sample_rate as u64
                                    / 1000)
                                    .max(32.min(loop_frames / 2)),
                                fade_out_frames: (audio.fade_out_ms as u64
                                    * self.sample_rate as u64
                                    / 1000)
                                    .max(32.min(loop_frames / 2)),
                                frames: Arc::clone(&prepared.samples),
                                start_frame: self.playhead_frame,
                                loop_frames,
                            }
                        } else {
                            Source::TrimmedSamples {
                                source_offset_frames: audio.source_offset_frames,
                                gain: audio.gain,
                                fade_in_frames: audio.fade_in_ms as u64 * self.sample_rate as u64
                                    / 1000,
                                fade_out_frames: audio.fade_out_ms as u64 * self.sample_rate as u64
                                    / 1000,
                                frames: Arc::clone(&prepared.samples),
                                start_frame,
                                // Convert the absolute clip end, not a duration
                                // measured from song zero. Rounding at a nonzero
                                // start otherwise introduces one-frame seams.
                                end_frame: self.tick_to_frame(
                                    clip.start.ticks().saturating_add(clip.duration.ticks()),
                                ),
                            }
                        };
                        let mut spec = NodeSpec::audio(id, source);
                        spec.gain = self.project.source_gain(track);
                        if let Some(clock) = &clock {
                            let origin = (self.session_active.get(&track.id) == Some(&clip.id))
                                .then_some(self.playhead_frame);
                            let voice = daw_engine::session::SessionVoice::new(
                                clock.clone(),
                                lane,
                                clip,
                                origin,
                            );
                            self.session_meters.insert(clip.id, voice.meter());
                            spec.session = Some(voice);
                        }
                        spec.pan = track.pan;
                        spec.effects = EffectRack::prepare(&track.effects, self.sample_rate);
                        builder.add_node(spec);
                        routes.push((id, audio_destination));
                    }
                }
                TrackKind::Group | TrackKind::Return | TrackKind::Master => {}
            }
        }
        // Direct monitoring is opt-in and only exists while a take is armed
        // (or during the short window between arming and the first buffer).
        // Reusing the pre-allocated ring avoids restarting playback after the
        // capture stream starts, which would shift the take's timeline origin.
        let live_monitor = self
            .recorder
            .as_ref()
            .map(|recording| (recording.track_id, Arc::clone(&recording.monitor)))
            .or_else(|| self.pending_monitor.clone());
        if self.input_monitor_enabled
            && let Some((track_id, monitor)) = live_monitor
            && let Some(track) = self
                .project
                .tracks
                .iter()
                .find(|track| track.id == track_id)
        {
            let dest = live_buses.get(&track.id).copied().unwrap_or_else(|| {
                self.project
                    .group_of(track)
                    .and_then(|group| Self::group_node(group.id).ok())
                    .unwrap_or(MASTER_ID)
            });
            let mut node = NodeSpec::audio(
                RECORD_MONITOR_ID,
                Source::LiveInput {
                    monitor: Arc::clone(&monitor),
                    cursor: monitor.next_index(),
                },
            );
            node.gain = self.project.source_gain(track);
            node.pan = track.pan;
            node.effects = EffectRack::prepare(&track.effects, self.sample_rate);
            builder.add_node(node);
            routes.push((RECORD_MONITOR_ID, dest));
        }
        builder.add_node(NodeSpec::master(MASTER_ID));
        for (from, to) in routes {
            builder.route(from, to, 1.0);
        }
        builder
            .build()
            .map_err(|err| format!("Cannot compile audio graph: {err:?}"))
    }

    fn compiled_midi_events(&self) -> Vec<(u64, EngineCommand)> {
        if self.view == View::Session {
            return Vec::new(); // Session voices own their render-thread MIDI schedules.
        }
        let mut events = Vec::new();
        for track in &self.project.tracks {
            if track.kind != TrackKind::Midi {
                continue;
            }
            let Ok(node) = Self::track_midi_node(track.id) else {
                continue;
            };
            for clip in &track.clips {
                let ClipKind::Midi(midi) = &clip.kind else {
                    continue;
                };
                for note in &midi.notes {
                    if note.muted
                        || note.pitch > 127
                        || note.duration.ticks() <= 0
                        || note.start.ticks() >= clip.duration.ticks()
                    {
                        continue;
                    }
                    let start = clip.start.ticks().saturating_add(note.start.ticks());
                    let end = clip.start.ticks().saturating_add(
                        note.start
                            .ticks()
                            .saturating_add(note.duration.ticks())
                            .min(clip.duration.ticks()),
                    );
                    let frame = |ticks| {
                        self.project
                            .tempo_map
                            .sample_frame_at(MusicalTime::from_ticks(ticks), self.sample_rate)
                            .ok()
                    };
                    if let (Some(on), Some(off)) = (frame(start), frame(end)) {
                        events.push((
                            on,
                            EngineCommand::NoteOn {
                                node,
                                note: note.pitch,
                                velocity: note.velocity,
                            },
                        ));
                        events.push((
                            off,
                            EngineCommand::NoteOff {
                                node,
                                note: note.pitch,
                            },
                        ));
                    }
                }
            }
        }
        // NoteOff before NoteOn when sharing the same tick avoids stuck/retriggered notes.
        events.sort_by_key(|(frame, command)| {
            (
                *frame,
                if matches!(command, EngineCommand::NoteOff { .. }) {
                    0
                } else {
                    1
                },
            )
        });
        events
    }

    fn start_playback(&mut self) {
        self.start_playback_with_sources(true);
    }

    fn start_playback_with_sources(&mut self, select_first_scene: bool) {
        self.playback_view = self.view;
        self.midi_reset_routing();
        self.driver = None;
        // USB interfaces can share a hardware clock with an already-open
        // microphone. If the input was switched between 44.1/48 kHz while
        // Rusty was open, recompile and resample all cached media at the new
        // rate before starting the next playback stream.
        if self.backend == Backend::PulseAudio && self.recorder.is_none() {
            let rate = Self::negotiate_sample_rate();
            if rate != self.sample_rate {
                self.sample_rate = rate;
                self.media.clear();
            }
        }
        self.watchdog_last_callbacks = 0;
        self.watchdog_last_change = Instant::now();
        if select_first_scene && self.view == View::Session && self.session_active.is_empty() {
            for track in &self.project.tracks {
                if let Some(clip) = self
                    .project
                    .session
                    .as_ref()
                    .and_then(|s| s.scenes.first())
                    .and_then(|scene| track.session_slots.get(scene))
                {
                    self.session_active.insert(track.id, clip.id);
                }
            }
        }
        let compiled = self.compiled_graph();
        let Some(graph) = self.apply_error(compiled) else {
            self.playing = false;
            return;
        };
        let config = DriverConfig {
            sample_rate: self.sample_rate,
            backend_preference: self.backend,
            ..DriverConfig::default()
        };
        let Some(mut driver) =
            self.apply_error(AudioDriver::start(graph, &config).map_err(|e| e.to_string()))
        else {
            self.playing = false;
            return;
        };
        self.events = self.compiled_midi_events();
        self.next_event = self
            .events
            .partition_point(|(frame, _)| *frame < self.playhead_frame);
        self.play_project_start = self.playhead_frame;
        self.pending_arrangement_seek = None;
        if self.playback_view == View::Arrangement {
            self.arrangement_seek_revision = self.arrangement_seek_revision.wrapping_add(1);
            let _ = driver
                .controller()
                .seek_arrangement(self.playhead_frame, self.arrangement_seek_revision);
            self.pending_arrangement_seek = Some(self.playhead_frame);
        } else {
            let _ = driver.controller().immediate(EngineCommand::Seek {
                project_frame: self.playhead_frame,
            });
        }
        let _ = driver.controller().immediate(EngineCommand::Play);
        self.watchdog_last_callbacks = driver.telemetry().callbacks.load(Ordering::Relaxed);
        self.watchdog_last_change = Instant::now();
        self.driver = Some(driver);
        self.playing = true;
        if self.playback_view == View::Arrangement {
            self.prepare_arrangement_midi(self.playhead_frame);
        }
        self.schedule_midi_ahead();
        self.status = "Playing through the native audio engine".into();
    }

    fn pause_playback(&mut self) {
        if self.playing && self.playback_view == View::Session {
            self.sync_session_active();
        }
        let finishing_audio_take = self.recorder.is_some();
        self.finish_midi_take();
        self.playing = false;
        if self.recorder.is_some() {
            self.finish_recording();
        }
        self.driver = None;
        self.midi_reset_routing();
        self.playing = false;
        self.events.clear();
        self.next_event = 0;
        self.status = if finishing_audio_take {
            format!("Paused • {}", self.status)
        } else {
            "Paused".into()
        };
        self.start_midi_monitor();
    }

    fn stop_playback(&mut self) {
        let finishing_audio_take = self.recorder.is_some();
        self.finish_midi_take();
        self.driver = None;
        self.midi_reset_routing();
        self.playing = false;
        if self.recorder.is_some() {
            self.finish_recording();
        }
        self.session_active.clear();
        self.session_meters.clear();
        self.playhead_frame = 0;
        self.events.clear();
        self.next_event = 0;
        self.status = if finishing_audio_take {
            format!("Stopped • {}", self.status)
        } else {
            "Stopped".into()
        };
        self.start_midi_monitor();
    }

    fn restart_for_edit(&mut self) {
        if self.recorder.is_some() {
            self.error = Some("Finish the microphone take before rebuilding playback; restarting the output clock would misalign the recording.".into());
            return;
        }
        if self.playing {
            let visible = self.view;
            self.view = self.playback_view;
            self.driver = None;
            self.playing = false;
            self.start_playback_with_sources(false);
            self.view = visible;
        } else if self.midi_connection.is_some() {
            self.start_midi_monitor();
        }
    }

    fn schedule_midi_ahead(&mut self) {
        if !self.playing {
            return;
        }
        if self.playback_view == View::Session {
            self.sync_session_active();
            return;
        }
        let Some(driver) = self.driver.as_mut() else {
            return;
        };
        let now = self.pending_arrangement_seek.unwrap_or_else(|| {
            driver
                .controller()
                .telemetry()
                .project_frame
                .load(Ordering::Relaxed)
        });
        let horizon = now.saturating_add(self.sample_rate as u64 * 2);
        while let Some(&(frame, command)) = self.events.get(self.next_event) {
            if frame > horizon {
                break;
            }
            if frame < self.play_project_start {
                self.next_event += 1;
                continue;
            }
            if driver
                .controller()
                .schedule_arrangement(frame, command)
                .is_err()
            {
                break;
            }
            self.next_event += 1;
        }
    }

    fn volume_changed(&mut self, _track_id: u64) {
        if let Some(driver) = self.driver.as_mut() {
            for group in self
                .project
                .tracks
                .iter()
                .filter(|t| t.kind == TrackKind::Group)
            {
                if let Ok(node) = Self::group_node(group.id) {
                    let _ = driver.controller().immediate(EngineCommand::RampGain {
                        node,
                        target_gain: if group.muted { 0.0 } else { group.volume },
                        duration_frames: 128,
                    });
                    let _ = driver.controller().immediate(EngineCommand::SetPan {
                        node,
                        pan: group.pan,
                    });
                }
            }
            for track in &self.project.tracks {
                if track.kind == TrackKind::Group {
                    continue;
                }
                let gain = self.project.source_gain(track);
                let ids = track
                    .playback_clips(self.playback_view == View::Session)
                    .filter_map(|clip| match track.kind {
                        TrackKind::Audio => Self::clip_audio_node(clip.id)
                            .ok()
                            .map(|node| (node, clip.id)),
                        TrackKind::Midi if self.playback_view == View::Session => {
                            Self::track_midi_node(clip.id)
                                .ok()
                                .map(|node| (node, clip.id))
                        }
                        _ => None,
                    })
                    .chain(if track.kind == TrackKind::Midi {
                        Self::track_midi_node(track.id)
                            .ok()
                            .map(|id| (id, track.id))
                    } else {
                        None
                    });
                for (node, _) in ids {
                    let _ = driver.controller().immediate(EngineCommand::RampGain {
                        node,
                        target_gain: gain,
                        duration_frames: 128,
                    });
                    let _ = driver.controller().immediate(EngineCommand::SetPan {
                        node,
                        pan: track.pan,
                    });
                }
            }
        }
        self.dirty = true;
    }

    /// Ask each running isolated worker for its opaque native state and store
    /// a validated, size-limited blob in the project. Failure keeps the last
    /// known-good state (or none) and never removes clips or devices.
    fn capture_live_plugin_states(&mut self, track_filter: Option<u64>) {
        if self.driver.is_none() {
            return;
        }
        let mut warnings = Vec::new();
        let rate = self.sample_rate;
        for track in &mut self.project.tracks {
            if track_filter.is_some_and(|id| id != track.id) {
                continue;
            }
            let Some(insert) = track.live_clap.as_mut() else {
                continue;
            };
            if !insert.enabled {
                continue;
            }
            let Some(control) = self.live_clap_controls.get(&track.id) else {
                continue;
            };
            let Some(runtime) = self
                .live_plugin_runtimes
                .get(&track.id)
                .and_then(|runtimes| runtimes.first())
            else {
                continue;
            };
            if runtime.bridge.is_finished() {
                warnings.push(format!(
                    "{}: kept previous plugin state (worker stopped)",
                    track.name
                ));
                continue;
            }
            let format = if insert.bundle.extension().is_some_and(|ext| ext == "vst3") {
                daw_core::plugin_state::FORMAT_VST3
            } else {
                daw_core::plugin_state::FORMAT_CLAP
            };
            match control.capture_state(Duration::from_millis(1_200)) {
                Ok(bytes) if bytes.is_empty() => {
                    // A plugin without opaque state is valid; keep the
                    // persisted identity but do not claim a state blob.
                    insert.class_id.clone_from(&runtime.class_id);
                    insert.state = None;
                }
                Ok(bytes) => {
                    match daw_core::plugin_state::PluginStateBlob::from_bytes(
                        format,
                        runtime.class_id.clone(),
                        rate,
                        &bytes,
                    ) {
                        Ok(blob) => {
                            insert.class_id.clone_from(&runtime.class_id);
                            insert.state = Some(blob);
                        }
                        Err(err) => warnings.push(format!(
                            "{}: saved without new plugin state ({err})",
                            track.name
                        )),
                    }
                }
                Err(err) => warnings.push(format!(
                    "{}: kept previous plugin state ({err})",
                    track.name
                )),
            }
        }
        if !warnings.is_empty() {
            self.error = Some(warnings.join("; "));
        }
    }

    fn save_project(&mut self, save_as: bool) {
        if self.recorder.is_some() {
            self.error = Some("Finish the WAV take before saving so its finalized file and clip are included in the project.".into());
            return;
        }
        let path = if save_as || self.project_path.is_none() {
            Self::pick_path(true, false)
        } else {
            self.project_path.clone()
        };
        let Some(path) = path else { return };
        self.capture_live_plugin_states(None);
        let content = serde_json::to_vec_pretty(&self.project)
            .map_err(|e| format!("Serialize project: {e}"))
            .and_then(|content| atomic_save_project(&path, &content));
        if self.apply_error(content).is_some() {
            self.project_path = Some(path.clone());
            self.status = format!("Saved {}", path.display());
            self.dirty = false;
        }
    }

    fn open_project(&mut self) {
        if self.recorder.is_some() {
            self.error = Some("Finish the WAV take before opening a different project.".into());
            return;
        }
        if let Some(path) = Self::pick_path(false, false) {
            self.load_project_path(path);
        }
    }

    /// Loading through the File menu and through --project shares one path.
    /// Invalid or missing files leave the current project intact.
    fn load_project_path(&mut self, path: PathBuf) {
        if self.recorder.is_some() {
            self.error =
                Some("Finish and save the current WAV take before opening another project.".into());
            return;
        }
        let candidate: Result<Project, String> = fs::read(&path)
            .map_err(|e| e.to_string())
            .and_then(|bytes| serde_json::from_slice(&bytes).map_err(|e| e.to_string()));
        let Some(mut project) = self.apply_error(candidate) else {
            return;
        };
        if let Err(error) = project.prepare_session() {
            self.error = Some(format!("Invalid Session project: {error}"));
            return;
        }
        if let Err(error) = project.validate_groups() {
            self.error = Some(format!("Invalid project group routing: {error}"));
            return;
        }
        self.stop_playback();
        self.history = ProjectHistory::new(&project);
        self.last_autosaved_project = project.clone();
        self.last_autosave = Instant::now();
        self.project = project;
        self.instrument_tones.clear();
        self.load_project_soundfonts();
        self.live_plugin_errors.clear();
        self.live_plugin_runtimes.clear();
        self.live_clap_controls.clear();
        self.media.clear();
        self.session_active.clear();
        self.session_meters.clear();
        self.piano_selected = None;
        self.piano_scroll_clip = None;
        self.arrangement_selection.clear();
        // Multi-clip destinations refer to tracks in the previous project.
        if self.clip_clipboard.len() > 1 {
            self.clip_clipboard.clear();
        }
        self.clip_delete = None;
        self.project_path = Some(path);
        self.next_id = self.project.next_object_id().unwrap();
        self.selected_track = self.project.tracks.first().map(|t| t.id);
        self.selected_clip = self
            .project
            .tracks
            .first()
            .and_then(|t| t.playback_clips(self.view == View::Session).next())
            .map(|c| c.id);
        // Prepared peaks are necessary for visible waveforms before Play.
        // Missing media is reported, not silently drawn as empty audio.
        let sources = self
            .project
            .tracks
            .iter()
            .flat_map(Track::all_clips)
            .filter_map(|clip| match &clip.kind {
                ClipKind::Audio(a) => Some(a.source.clone()),
                _ => None,
            })
            .collect::<std::collections::HashSet<_>>();
        let mut missing = Vec::new();
        for source in sources {
            if let Err(err) = self.decode_cached(&source) {
                missing.push(err);
            }
        }
        if missing.is_empty() {
            self.status = "Project opened • audio waveforms ready".into();
        } else {
            self.error = Some(format!(
                "{} audio file(s) unavailable: {}",
                missing.len(),
                missing[0]
            ));
            self.status = "Project opened with missing audio".into();
        }
        self.dirty = false;
        if self.midi_connection.is_some() {
            self.start_midi_monitor();
        }
    }

    fn selected_clip_pair(&self) -> Option<(&Track, &Clip)> {
        self.project.tracks.iter().find_map(|track| {
            track
                .playback_clips(self.view == View::Session)
                .find(|clip| Some(clip.id) == self.selected_clip)
                .map(|clip| (track, clip))
        })
    }

    fn tick_to_frame(&self, ticks: i64) -> u64 {
        self.project
            .tempo_map
            .sample_frame_at(MusicalTime::from_ticks(ticks.max(0)), self.sample_rate)
            .unwrap_or(0)
    }

    fn max_project_seconds(&self) -> f64 {
        self.project
            .tracks
            .iter()
            .flat_map(|t| &t.clips)
            .map(|clip| {
                self.tick_to_frame(clip.start.ticks().saturating_add(clip.duration.ticks())) as f64
                    / self.sample_rate as f64
            })
            .fold(0.0, f64::max)
    }

    fn time_display(&self) -> String {
        let seconds = self.playhead_frame as f64 / self.sample_rate as f64;
        let mins = (seconds / 60.0) as u64;
        format!("{mins:02}:{:05.2}", seconds % 60.0)
    }
}

impl Studio {}

impl Studio {
    fn detail_panel(&mut self, ctx: &egui::Context) {
        let height = ctx.available_rect().height();
        let max_height = (height - 240.0).max(height * 0.45);
        egui::TopBottomPanel::bottom("detail_editor")
            .resizable(true)
            .default_height(390.0)
            .height_range(150.0..=max_height.max(150.0))
            .show(ctx, |ui| {
                // egui stores the panel's actual content height, not just the
                // dragged rectangle. Without filling that rectangle, the panel
                // shrinks back to its content height on the next frame.
                ui.set_min_height(ui.available_height());
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("EDITOR").strong().size(11.0).color(ACCENT));
                    ui.selectable_value(&mut self.detail, Detail::Clip, "▥  Piano roll / Clip");
                    ui.selectable_value(&mut self.detail, Detail::Mixer, "☷  Mixer");

                    ui.separator();
                    if let Some((track, _)) = self.selected_clip_pair() {
                        ui.label(&track.name);
                    } else {
                        ui.weak("Select a clip");
                    }
                });
                ui.separator();
                if self.detail == Detail::Mixer {
                    self.mixer_editor(ui);
                    return;
                }
                let selection = self.selected_clip_pair().map(|(track, clip)| (
                    track.id, track.name.clone(), clip.clone()
                ));
                match selection {
                    Some((_, _, clip)) if matches!(clip.kind, ClipKind::Midi(_)) => {
                        self.piano_editor(ui, &clip);
                    }
                    Some((track_id, name, clip @ Clip {
                        kind: ClipKind::Audio(_), ..
                    })) => {
                        self.audio_detail_editor(ui, track_id, &name, &clip);
                    }
                    _ => {
                        let empty_audio = self.selected_track.and_then(|id| {
                            self.project.tracks.iter()
                                .find(|track| track.id == id && track.kind == TrackKind::Audio)
                        });
                        if let Some(track) = empty_audio {
                            let id = track.id;
                            let input_name = track.record_input.as_deref()
                                .unwrap_or("Auto — first physical input");
                            let input_channels = track.record_input_channels.label();
                            let input_button = format!("{input_name} · {input_channels} · Choose input");
                            ui.vertical_centered(|ui| {
                                ui.heading("Empty audio track");
                                ui.label("Choose this track's input, then record or import audio.");
                                if ui
                                    .button(input_button)
                                    .on_hover_text("Open Audio / MIDI devices for this Audio track")
                                    .clicked()
                                {
                                    self.device_tab = DeviceTab::Hardware;
                                    self.show_devices = true;
                                }
                                if ui.button("＋ Import WAV / FLAC / MP3…").clicked()
                                    && let Some(path) = Self::pick_path(false, true) {
                                    self.session_append_audio(id, path);
                                }
                                if ui.button(if self.recorder.is_some() {
                                    "■ Stop recording and keep take"
                                } else {
                                    "● Record into this Audio track"
                                }).clicked() {
                                    self.toggle_recording();
                                }
                            });
                        } else {
                            ui.centered_and_justified(|ui| {
                                ui.label("Select a clip, or import WAV / FLAC / MP3 / OGG to get started.");
                            });
                        }
                    }
                }
            });
    }

    fn device_editor(&mut self, ui: &mut egui::Ui) {
        self.device_editor_compact(ui);
    }

    fn footer(&self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if let Some(error) = &self.error {
                    ui.colored_label(Color32::from_rgb(255, 149, 141), format!("⚠ {error}"));
                } else {
                    ui.label(egui::RichText::new(&self.status).weak());
                }
                ui.separator();
                if let Some(driver) = &self.driver {
                    ui.small(format!(
                        "{} • {} Hz • {}",
                        driver.info().backend,
                        driver.info().sample_rate,
                        driver.info().device
                    ));
                    let telem = driver.telemetry();
                    ui.small(format!(
                        "audio callbacks: {}",
                        telem.callbacks.load(Ordering::Relaxed)
                    ));
                } else {
                    ui.weak("Audio engine idle");
                }
                ui.separator();
                ui.small("F3 Devices · Tab switches view · Help opens the guide")
                    .on_hover_text("Space plays or stops. Shift+Tab switches the editor. Ctrl+C/V copies and pastes clips. S splits and Delete removes the selected clip. Q toggles snap. Arrow keys seek by bar. Ctrl+Z/Y undoes or redoes an edit.");
            });
        });
    }
}

impl eframe::App for Studio {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let shortcuts = ctx.input(|input| {
            (
                input.key_pressed(egui::Key::Space),
                input.key_pressed(egui::Key::Tab),
                input.modifiers.shift,
                input.modifiers.command,
                input.key_pressed(egui::Key::S),
                input.key_pressed(egui::Key::O),
                input.key_pressed(egui::Key::F),
            )
        });
        let command_z = ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::Z));
        let command_y = ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::Y));
        if !ctx.wants_keyboard_input() && (command_z || command_y) {
            let redo = (command_z && shortcuts.2) || command_y;
            self.history_step(redo);
        }
        // Arrangement/editor keyboard map. Plain letters are ignored while a
        // text field or search box owns the keyboard.
        let keys = ctx.input(|input| {
            let command = input.modifiers.command;
            (
                command,
                input.modifiers.shift,
                input.key_pressed(egui::Key::C),
                input.key_pressed(egui::Key::V),
                input.key_pressed(egui::Key::X),
                input.key_pressed(egui::Key::D),
                input.key_pressed(egui::Key::S),
                input.key_pressed(egui::Key::Q),
                input.key_pressed(egui::Key::Delete) || input.key_pressed(egui::Key::Backspace),
                input.key_pressed(egui::Key::Plus)
                    || (command && input.key_pressed(egui::Key::Equals)),
                input.key_pressed(egui::Key::Minus),
                input.key_pressed(egui::Key::Home),
                input.key_pressed(egui::Key::ArrowLeft),
                input.key_pressed(egui::Key::ArrowRight),
            )
        });
        if !ctx.wants_keyboard_input() {
            let (
                command,
                shift,
                key_c,
                key_v,
                key_x,
                key_d,
                key_s,
                key_q,
                key_delete,
                key_zoom_in,
                key_zoom_out,
                key_home,
                key_left,
                key_right,
            ) = keys;
            if command {
                if key_c {
                    self.copy_selected_clip();
                }
                if key_v {
                    self.paste_clip();
                }
                if key_x && self.copy_selected_clip() {
                    self.delete_selected_clip();
                }
                if key_d {
                    self.duplicate_selected_clip();
                }
                if key_zoom_in {
                    self.zoom = (self.zoom + 4.0).clamp(28.0, 140.0);
                }
                if key_zoom_out {
                    self.zoom = (self.zoom - 4.0).clamp(28.0, 140.0);
                }
            } else {
                // The piano roll handles Delete for a selected MIDI note.
                // Never also delete its parent clip in the arrangement.
                let piano_note_target = self.detail == Detail::Clip
                    && self.piano_selected.is_some()
                    && self
                        .selected_clip_pair()
                        .is_some_and(|(_, clip)| matches!(clip.kind, ClipKind::Midi(_)));
                if key_delete && !piano_note_target {
                    self.delete_selected_clip();
                }
                if key_s && self.view == View::Arrangement {
                    self.split_selected_clip();
                }
                if key_q {
                    self.snap = !self.snap;
                }
                if key_home {
                    self.seek_to_frame(0);
                }
                if key_left || key_right {
                    let beat_ticks = (self.bar_ticks() / 4).max(1);
                    let step = if shift { beat_ticks } else { self.bar_ticks() };
                    let current = self.tick_at_frame(self.playhead_frame);
                    let target = if key_left {
                        current - step
                    } else {
                        current + step
                    };
                    let frame = self.tick_to_frame(target.max(0));
                    self.seek_to_frame(frame);
                }
            }
        }
        if shortcuts.3 && shortcuts.4 {
            self.save_project(false);
        }
        if shortcuts.3 && shortcuts.5 {
            self.open_project();
        }
        if shortcuts.3 && shortcuts.6 {
            ctx.memory_mut(|m| m.request_focus(egui::Id::new("source_search")));
        }
        if shortcuts.1 && !ctx.wants_keyboard_input() {
            if shortcuts.2 {
                self.detail = if self.detail == Detail::Clip {
                    Detail::Mixer
                } else {
                    Detail::Clip
                };
            } else {
                let next = if self.view == View::Arrangement {
                    View::Session
                } else {
                    View::Arrangement
                };
                self.change_view(next);
            }
        }
        if shortcuts.0 && !ctx.wants_keyboard_input() {
            if self.playing {
                self.stop_playback();
            } else {
                self.start_playback();
            }
        }
        self.computer_keyboard(ctx);

        let dropped = ctx.input(|input| {
            input
                .raw
                .dropped_files
                .iter()
                .filter_map(|file| file.path.clone())
                .collect::<Vec<_>>()
        });
        for path in dropped {
            self.import_audio(path);
        }
        self.poll_plugin_job();
        self.poll_plugin_scan();
        self.poll_mix_export();
        self.poll_autosave(ctx);
        self.poll_midi_events();
        self.poll_soundfont_loads();
        for handle in self.soundfont_handles.values_mut() {
            handle.reclaim_retired();
        }
        if !self.soundfont_loads.is_empty() {
            ctx.request_repaint_after(Duration::from_millis(50));
        }
        self.poll_track_input(ctx);
        if self.recorder.is_some() {
            self.poll_recording_input();
        }
        if self.fx_rebuild_pending && !ctx.input(|i| i.pointer.any_down()) {
            self.fx_rebuild_pending = false;
            self.restart_for_edit();
        }
        if self.soundfont_rebuild_pending
            && self.recorder.is_none()
            && self.midi_take.is_none()
            && !ctx.input(|i| i.pointer.any_down())
        {
            self.soundfont_rebuild_pending = false;
            if !self.playing && self.driver.take().is_some() {
                self.start_monitor_driver();
            } else {
                self.restart_for_edit();
            }
        }
        if self.midi_connection.is_some() {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
        if self.plugin_scan_job.is_some()
            || self.plugin_job.is_some()
            || self.export_job.is_some()
            || self.recorder.is_some()
        {
            ctx.request_repaint_after(Duration::from_millis(50));
        }

        if self.playing {
            let mut stalled = false;
            if let Some(driver) = &self.driver {
                let telemetry = driver.telemetry();
                let callbacks = telemetry.callbacks.load(Ordering::Relaxed);
                if callbacks != self.watchdog_last_callbacks {
                    self.watchdog_last_callbacks = callbacks;
                    self.watchdog_last_change = Instant::now();
                } else if self.watchdog_last_change.elapsed() >= Duration::from_secs(2) {
                    stalled = true;
                }
                let delay_us = telemetry
                    .predicted_playback_delay_us
                    .load(Ordering::Relaxed);
                let seek_rendered = telemetry.arrangement_seek_revision.load(Ordering::Acquire)
                    == self.arrangement_seek_revision;
                if self.playback_view != View::Arrangement || seek_rendered {
                    self.pending_arrangement_seek = None;
                    self.playhead_frame = Self::displayed_playhead_frame(
                        telemetry.project_frame.load(Ordering::Relaxed),
                        delay_us,
                        self.sample_rate,
                    )
                    .max(if self.playback_view == View::Arrangement {
                        self.play_project_start
                    } else {
                        0
                    });
                }
            }
            if stalled && self.recorder.is_some() {
                // The playback clock is part of the take alignment contract.
                // A dead output stream cannot support a valid continuing take:
                // stop capture promptly and preserve the already written WAV.
                self.driver = None;
                self.playing = false;
                self.finish_recording();
                let writer_error = self.error.take();
                self.error = Some(format!(
                    "Output clock stalled during recording; capture stopped to prevent a misaligned take. Save the recovered clip if present.{}",
                    writer_error.map_or(String::new(), |error| format!(" Writer status: {error}"))
                ));
                self.status = "Recording stopped: output clock lost".into();
            } else if stalled {
                let rate = if self.backend == Backend::PulseAudio && self.recorder.is_none() {
                    Some(Self::negotiate_sample_rate())
                } else {
                    None
                };
                if rate.is_some_and(|new_rate| new_rate != self.sample_rate) {
                    self.status = "Audio clock changed: rebuilding graph at new rate".into();
                    self.restart_for_edit();
                } else {
                    self.driver = None;
                    self.playing = false;
                    self.error = Some(format!(
                        "Playback stream stalled at {} Hz. Check the interface clock and PulseAudio output. The project was not modified.",
                        self.sample_rate
                    ));
                    self.status = "Audio device stopped delivering buffers".into();
                }
            } else {
                self.schedule_midi_ahead();
            }
            ctx.request_repaint_after(Duration::from_millis(32));
        }

        self.studio_transport(ctx);
        self.footer(ctx);
        // Side panel first, so the browser spans both the arrangement and
        // editor instead of being clipped above the detail panel.
        self.studio_browser(ctx);
        self.detail_panel(ctx);
        self.draw_rename_window(ctx);
        self.draw_delete_window(ctx);
        self.draw_clip_delete_window(ctx);
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(if self.view == View::Arrangement {
                        "TIMELINE  /  ARRANGEMENT"
                    } else {
                        "PERFORMANCE  /  SESSION"
                    })
                    .size(11.0)
                    .strong()
                    .color(ACCENT),
                );
                ui.separator();
                ui.label(
                    egui::RichText::new("RUSTIE  /  SLATE STUDIO")
                        .size(10.0)
                        .weak(),
                );
            });
            ui.separator();
            match self.view {
                View::Arrangement => self.studio_arrangement(ui),
                View::Session => self.studio_session(ui),
            }
        });
        if self.show_devices {
            let mut open = true;
            egui::Window::new("Devices & Plug-ins")
                .id(egui::Id::new("studio_devices_window"))
                .default_width(720.0)
                .default_height(450.0)
                .open(&mut open)
                .show(ctx, |ui| self.device_editor(ui));
            if !open {
                self.show_devices = false;
            }
        }
        if !self.show_devices
            && let Some(note) = self.keyboard_mouse_note.take()
        {
            self.stop_audition_note(note);
        }
        if self.show_tutorial {
            self.tutorial_window(ctx);
        }
        if self.view != View::Arrangement
            || self.arrangement_selection.last().copied() != self.selected_clip
        {
            self.arrangement_selection.clear();
        }
        let gesture_active = ctx.input(|i| i.pointer.any_down()) || ctx.wants_keyboard_input();
        let selection = self.editor_selection();
        self.history
            .observe(&self.project, selection, gesture_active);
        // A pointer release outside every panel can leave a preview behind.
        // Dropping the stale preview never applies an uncommitted edit.
        if !ctx.input(|i| i.pointer.any_down()) {
            self.clip_drag_preview = None;
            self.clip_drag_origin_x = None;
            self.clip_resize_preview = None;
            self.clip_resize_drag = None;
            self.piano_velocity_drag = None;
            self.piano_velocity_drag_value = None;
        }
    }
}

/// Headless integration smoke test for the same MIDI synth and native PulseAudio
/// backend used by the GUI's Play button. Deliberately short and low volume.
pub fn smoke_audio() {
    let mut builder = GraphBuilder::new(48_000);
    let mut instrument = NodeSpec::midi(MIDI_OFFSET + 1);
    instrument.gain = 0.30;
    builder
        .add_node(instrument)
        .add_node(NodeSpec::master(MASTER_ID));
    builder.route(MIDI_OFFSET + 1, MASTER_ID, 1.0);
    let graph = builder.build().expect("valid MIDI demo graph");
    let config = DriverConfig {
        sample_rate: 48_000,
        backend_preference: Backend::Alsa,
        ..DriverConfig::default()
    };
    let mut driver =
        AudioDriver::start(graph, &config).unwrap_or_else(|e| panic!("audio backend: {e}"));
    println!(
        "Audio smoke: {} / {}",
        driver.info().backend,
        driver.info().device
    );
    let telemetry = Arc::clone(driver.telemetry());
    driver
        .controller()
        .immediate(EngineCommand::Play)
        .expect("Play queued");
    driver
        .controller()
        .immediate(EngineCommand::NoteOn {
            node: MIDI_OFFSET + 1,
            note: 69,
            velocity: 90,
        })
        .expect("NoteOn queued");
    std::thread::sleep(Duration::from_millis(650));
    driver
        .controller()
        .immediate(EngineCommand::NoteOff {
            node: MIDI_OFFSET + 1,
            note: 69,
        })
        .expect("NoteOff queued");
    std::thread::sleep(Duration::from_millis(220));
    drop(driver);
    let peak = f32::from_bits(telemetry.output_peak_bits.load(Ordering::Relaxed));
    let callbacks = telemetry.callbacks.load(Ordering::Relaxed);
    println!("Audio smoke callbacks={callbacks}; peak={peak:.6}");
    assert!(
        callbacks > 0 && peak > 0.001,
        "MIDI demo generated no audible samples"
    );
}

include!("theme.rs");
include!("studio_track_management.rs");
include!("studio_clip_management.rs");
include!("studio_history.rs");
include!("studio_persistence.rs");
include!("studio_views.rs");
include!("studio_browser.rs");
include!("studio_piano.rs");
include!("studio_session.rs");
include!("studio_help.rs");
include!("studio_recording.rs");
include!("studio_midi_input.rs");
include!("studio_midi_recording.rs");
include!("studio_devices.rs");
include!("studio_audio_editor.rs");
include!("studio_plugin_discovery.rs");
include!("studio_plugin_render.rs");
include!("studio_live_plugins.rs");
include!("studio_export.rs");
include!("studio_instruments.rs");

#[cfg(test)]
mod sample_rate_selection_tests {
    use super::Studio;

    #[test]
    fn uses_default_output_sink_even_if_unrelated_source_has_another_clock() {
        let info = "Default Sample Specification: s16le 2ch 48000Hz\nDefault Sink: alsa_output.usb-interface.analog-stereo\n";
        let sinks = "0\talsa_output.usb-interface.analog-stereo\tmodule-alsa-card.c\ts32le 2ch 44100Hz\tIDLE\n1\tother\tmodule-alsa-card.c\ts16le 2ch 48000Hz\tRUNNING\n";
        assert_eq!(Studio::default_sink_rate(info, sinks), Some(44_100));
    }

    #[test]
    fn bar_ruler_uses_the_real_time_signature() {
        let four_four = super::TimeSignature::new(4, 4).unwrap();
        let six_eight = super::TimeSignature::new(6, 8).unwrap();
        let seven_eight = super::TimeSignature::new(7, 8).unwrap();
        assert_eq!(super::bar_ticks_for(four_four), 4 * super::PPQ);
        assert_eq!(super::bar_ticks_for(six_eight), 3 * super::PPQ);
        assert_eq!(super::bar_ticks_for(seven_eight), 7 * super::PPQ / 2);
        // The old visual code highlighted every fourth quarter-note beat.
        // In 6/8 the real bar boundary is beat 3, not beat 4.
        let old_highlight = (0_i64..7).filter(|beat| beat % 4 == 0).collect::<Vec<_>>();
        let real_highlight = (0_i64..7)
            .filter(|beat| (beat * super::PPQ).rem_euclid(super::bar_ticks_for(six_eight)) == 0)
            .collect::<Vec<_>>();
        assert_eq!(old_highlight, vec![0, 4]);
        assert_eq!(real_highlight, vec![0, 3, 6]);
    }

    #[test]
    fn rejects_unsupported_or_unmatched_sink_rates() {
        let info = "Default Sink: alsa_output.usb-interface.analog-stereo\n";
        assert_eq!(
            Studio::default_sink_rate(info, "0\tother\talsa\ts32le 2ch 48000Hz\tIDLE\n"),
            None
        );
        assert_eq!(
            Studio::default_sink_rate(
                info,
                "0\talsa_output.usb-interface.analog-stereo\talsa\ts32le 2ch 192000Hz\tIDLE\n"
            ),
            None,
        );
    }

    #[test]
    fn displayed_playhead_compensates_reported_output_delay_without_underflow() {
        // 15 ms at 44.1 kHz is 661.5 → 661 frames of compensation.
        assert_eq!(
            Studio::displayed_playhead_frame(10_000, 15_000, 44_100),
            9_339
        );
        assert_eq!(Studio::displayed_playhead_frame(100, 15_000, 44_100), 0);
        assert_eq!(
            Studio::displayed_playhead_frame(220_500, 0, 44_100),
            220_500
        );
    }
}
