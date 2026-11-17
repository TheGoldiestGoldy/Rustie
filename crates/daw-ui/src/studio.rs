use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, atomic::Ordering},
    time::Duration,
};

use daw_core::{
    AudioClip, Clip, ClipKind, MidiClip, MidiNote, MusicalTime, PPQ, PitchClass, Project,
    ScaleDefinition, ScaleMode, Tempo, Track, TrackKind,
};
use daw_engine::{
    AudioDriver, DriverConfig, EngineCommand, GraphBuilder, NodeSpec, ScheduledCommand, Source,
    StereoFrame, decode_to_source, driver::Backend,
};
use eframe::egui::{self, Color32, FontId, Pos2, Rect, Sense, Stroke, Vec2};

const MASTER_ID: u32 = 900_000;
const AUDIO_OFFSET: u32 = 10_000;
const MIDI_OFFSET: u32 = 500_000;
const MAX_CLIPS: usize = 48;

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
    Devices,
}

pub struct Studio {
    project: Project,
    media: HashMap<PathBuf, PreparedAudio>,
    driver: Option<AudioDriver>,
    backend: Backend,
    sample_rate: u32,
    playhead_frame: u64,
    playing: bool,
    selected_track: Option<u64>,
    selected_clip: Option<u64>,
    next_id: u64,
    view: View,
    detail: Detail,
    fold: bool,
    project_path: Option<PathBuf>,
    search: String,
    path_field: String,
    error: Option<String>,
    status: String,
    zoom: f32,
    snap: bool,
    clip_drag_preview: Option<(u64, f32)>,
    events: Vec<(u64, EngineCommand)>,
    next_event: usize,
    play_engine_start: u64,
    play_project_start: u64,
    dirty: bool,
}

impl Studio {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let mut visuals = egui::Visuals::dark();
        visuals.panel_fill = Color32::from_rgb(20, 24, 30);
        visuals.window_fill = Color32::from_rgb(22, 27, 34);
        visuals.extreme_bg_color = Color32::from_rgb(12, 16, 22);
        visuals.widgets.active.bg_fill = Color32::from_rgb(56, 118, 155);
        visuals.selection.bg_fill = Color32::from_rgb(44, 104, 141);
        cc.egui_ctx.set_visuals(visuals);

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
        Self {
            project,
            media: HashMap::new(),
            driver: None,
            backend: Backend::Alsa,
            sample_rate: 48_000,
            playhead_frame: 0,
            playing: false,
            selected_track: Some(1),
            selected_clip: Some(2),
            next_id: 3,
            view: View::Arrangement,
            detail: Detail::Clip,
            fold: false,
            project_path: None,
            search: String::new(),
            path_field: String::new(),
            error: None,
            status: "Ready • Start with ▶ Play to hear the built-in MIDI demo".into(),
            zoom: 56.0,
            snap: true,
            clip_drag_preview: None,
            events: Vec::new(),
            next_event: 0,
            play_engine_start: 0,
            play_project_start: 0,
            dirty: false,
        }
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
            "Save Slate Studio project"
        } else {
            "Open Slate Studio project"
        };
        args.push(title);
        if save {
            args.extend([
                "--save",
                "--confirm-overwrite",
                "--filename=Untitled.slate.json",
            ]);
        } else if audio {
            args.push("--file-filter=Audio files | *.wav *.flac *.mp3 *.ogg *.m4a *.aiff");
        } else {
            args.push("--file-filter=Slate projects | *.json");
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

    fn add_audio(&mut self, path: PathBuf) {
        if self.clip_count() >= MAX_CLIPS {
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
        track.clips.push(Clip {
            id: clip_id,
            start: MusicalTime::ZERO,
            duration: MusicalTime::from_ticks(ticks),
            kind: ClipKind::Audio(AudioClip {
                source: path.clone(),
                warped: false,
            }),
            scale_override: None,
        });
        self.project.tracks.push(track);
        self.selected_track = Some(track_id);
        self.selected_clip = Some(clip_id);
        self.dirty = true;
        self.status = format!("Imported {} ({duration_seconds:.1}s)", path.display());
        self.restart_for_edit();
    }

    fn clip_count(&self) -> usize {
        self.project.tracks.iter().map(|t| t.clips.len()).sum()
    }

    fn add_midi_track(&mut self) {
        if self.clip_count() >= MAX_CLIPS {
            self.error = Some("Clip limit reached".into());
            return;
        }
        let track_id = self.next_id;
        self.next_id += 1;
        let clip_id = self.next_id;
        self.next_id += 1;
        let mut track = Track::new(track_id, format!("MIDI {track_id}"), TrackKind::Midi);
        track.clips.push(Clip {
            id: clip_id,
            start: MusicalTime::ZERO,
            duration: MusicalTime::from_ticks(16 * PPQ),
            kind: ClipKind::Midi(MidiClip::default()),
            scale_override: None,
        });
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

    fn clip_audio_node(id: u64) -> Result<u32, String> {
        u32::try_from(id)
            .ok()
            .and_then(|n| n.checked_add(AUDIO_OFFSET))
            .filter(|id| *id < MIDI_OFFSET)
            .ok_or("Audio clip ID out of range".into())
    }

    fn compiled_graph(&mut self) -> Result<daw_engine::AudioGraph, String> {
        let paths = self
            .project
            .tracks
            .iter()
            .flat_map(|t| t.clips.iter())
            .filter_map(|c| match &c.kind {
                ClipKind::Audio(audio) => Some(audio.source.clone()),
                _ => None,
            })
            .collect::<Vec<_>>();
        for path in paths {
            self.decode_cached(&path)?;
        }
        if self.clip_count() > MAX_CLIPS {
            return Err("Too many clips".into());
        }
        let solo_any = self.project.tracks.iter().any(|track| track.solo);
        let mut builder = GraphBuilder::new(self.sample_rate);
        let mut route_ids = Vec::new();
        for track in &self.project.tracks {
            match track.kind {
                TrackKind::Midi => {
                    let id = Self::track_midi_node(track.id)?;
                    let mut spec = NodeSpec::midi(id);
                    spec.gain = if track.muted || (solo_any && !track.solo) {
                        0.0
                    } else {
                        track.volume
                    };
                    spec.pan = track.pan;
                    builder.add_node(spec);
                    route_ids.push(id);
                }
                TrackKind::Audio => {
                    for clip in &track.clips {
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
                        let mut spec = NodeSpec::audio(
                            id,
                            Source::Samples {
                                frames: Arc::clone(&prepared.samples),
                                start_frame,
                            },
                        );
                        spec.gain = if track.muted || (solo_any && !track.solo) {
                            0.0
                        } else {
                            track.volume
                        };
                        spec.pan = track.pan;
                        builder.add_node(spec);
                        route_ids.push(id);
                    }
                }
                TrackKind::Return | TrackKind::Master => {}
            }
        }
        builder.add_node(NodeSpec::master(MASTER_ID));
        for id in route_ids {
            builder.route(id, MASTER_ID, 1.0);
        }
        builder
            .build()
            .map_err(|err| format!("Cannot compile audio graph: {err:?}"))
    }

    fn compiled_midi_events(&self) -> Vec<(u64, EngineCommand)> {
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
                    if note.muted || note.pitch > 127 || note.duration.ticks() <= 0 {
                        continue;
                    }
                    let start = clip.start.ticks().saturating_add(note.start.ticks());
                    let end = start.saturating_add(note.duration.ticks());
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
        self.driver = None;
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
        self.play_engine_start = driver.controller().engine_frame_snapshot();
        let _ = driver.controller().immediate(EngineCommand::Seek {
            project_frame: self.playhead_frame,
        });
        let _ = driver.controller().immediate(EngineCommand::Play);
        self.driver = Some(driver);
        self.playing = true;
        self.schedule_midi_ahead();
        self.status = "Playing through the native audio engine".into();
    }

    fn pause_playback(&mut self) {
        self.driver = None;
        self.playing = false;
        self.events.clear();
        self.next_event = 0;
        self.status = "Paused".into();
    }

    fn stop_playback(&mut self) {
        self.driver = None;
        self.playing = false;
        self.playhead_frame = 0;
        self.events.clear();
        self.next_event = 0;
        self.status = "Stopped".into();
    }

    fn restart_for_edit(&mut self) {
        if self.playing {
            self.driver = None;
            self.playing = false;
            self.start_playback();
        }
    }

    fn schedule_midi_ahead(&mut self) {
        if !self.playing {
            return;
        }
        let Some(driver) = self.driver.as_mut() else {
            return;
        };
        let now = driver
            .controller()
            .telemetry()
            .project_frame
            .load(Ordering::Relaxed);
        let horizon = now.saturating_add(self.sample_rate as u64 * 2);
        while let Some(&(frame, command)) = self.events.get(self.next_event) {
            if frame > horizon {
                break;
            }
            if frame < self.play_project_start {
                self.next_event += 1;
                continue;
            }
            let at_engine_frame = self
                .play_engine_start
                .saturating_add(frame.saturating_sub(self.play_project_start));
            if driver
                .controller()
                .schedule(ScheduledCommand {
                    at_engine_frame,
                    command,
                })
                .is_err()
            {
                break;
            }
            self.next_event += 1;
        }
    }

    fn volume_changed(&mut self, _track_id: u64) {
        let solo_any = self.project.tracks.iter().any(|track| track.solo);
        if let Some(driver) = self.driver.as_mut() {
            for track in &self.project.tracks {
                let gain = if track.muted || (solo_any && !track.solo) {
                    0.0
                } else {
                    track.volume
                };
                let ids = track
                    .clips
                    .iter()
                    .filter_map(|clip| {
                        if track.kind == TrackKind::Audio {
                            Self::clip_audio_node(clip.id).ok()
                        } else {
                            None
                        }
                    })
                    .chain(if track.kind == TrackKind::Midi {
                        Self::track_midi_node(track.id).ok()
                    } else {
                        None
                    });
                for node in ids {
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

    fn save_project(&mut self, save_as: bool) {
        let path = if save_as || self.project_path.is_none() {
            Self::pick_path(true, false)
        } else {
            self.project_path.clone()
        };
        let Some(path) = path else { return };
        let content = serde_json::to_vec_pretty(&self.project)
            .map_err(|e| format!("Serialize project: {e}"))
            .and_then(|content| {
                fs::write(&path, content).map_err(|e| format!("Save {}: {e}", path.display()))
            });
        if self.apply_error(content).is_some() {
            self.project_path = Some(path.clone());
            self.status = format!("Saved {}", path.display());
            self.dirty = false;
        }
    }

    fn open_project(&mut self) {
        let Some(path) = Self::pick_path(false, false) else {
            return;
        };
        let candidate: Result<Project, String> = fs::read(&path)
            .map_err(|e| e.to_string())
            .and_then(|bytes| serde_json::from_slice(&bytes).map_err(|e| e.to_string()));
        let Some(project) = self.apply_error(candidate) else {
            return;
        };
        if project.tracks.len() > 48
            || project.tracks.iter().map(|t| t.clips.len()).sum::<usize>() > MAX_CLIPS
        {
            self.error = Some("Project exceeds the prototype's 48-track/48-clip limit".into());
            return;
        }
        self.stop_playback();
        self.project = project;
        self.media.clear();
        self.project_path = Some(path);
        self.next_id = self
            .project
            .tracks
            .iter()
            .flat_map(|track| {
                std::iter::once(track.id).chain(track.clips.iter().map(|clip| clip.id))
            })
            .max()
            .unwrap_or(0)
            .saturating_add(1);
        self.selected_track = self.project.tracks.first().map(|t| t.id);
        self.selected_clip = self
            .project
            .tracks
            .first()
            .and_then(|t| t.clips.first())
            .map(|c| c.id);
        self.status = "Project opened • press Play to load assets".into();
        self.dirty = false;
    }

    fn selected_clip_pair(&self) -> Option<(&Track, &Clip)> {
        self.project.tracks.iter().find_map(|track| {
            track
                .clips
                .iter()
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
        egui::TopBottomPanel::bottom("detail_editor")
            .resizable(true).default_height(280.0).min_height(125.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("DETAIL").strong().size(11.0));
                    ui.selectable_value(&mut self.detail, Detail::Clip, "Clip editor");
                    ui.selectable_value(&mut self.detail, Detail::Devices, "Device chain");
                    ui.separator();
                    if let Some((track, _)) = self.selected_clip_pair() {
                        ui.label(&track.name);
                    } else {
                        ui.weak("Select a clip");
                    }
                });
                ui.separator();
                if self.detail == Detail::Devices {
                    self.device_editor(ui);
                    return;
                }
                let selection = self.selected_clip_pair().map(|(track, clip)| (
                    track.id, track.name.clone(), clip.clone()
                ));
                match selection {
                    Some((_, _, clip)) if matches!(clip.kind, ClipKind::Midi(_)) => {
                        self.piano_editor(ui, &clip);
                    }
                    Some((_, name, Clip {
                        kind: ClipKind::Audio(audio), ..
                    })) => {
                        ui.heading(name);
                        ui.label(format!("Source: {}", audio.source.display()));
                        if let Some(cached) = self.media.get(&audio.source) {
                            ui.label(format!("Samples: {} • {:.2} seconds • {} Hz",
                                cached.samples.len(),
                                cached.samples.len() as f64 / self.sample_rate as f64,
                                self.sample_rate));
                        } else { ui.weak("Press Play to load the clip data"); }
                        ui.label(egui::RichText::new(
                            "Non-destructive playback prototype • warping and sample editing are later milestones"
                        ).weak());
                    }
                    _ => { ui.centered_and_justified(|ui| {
                        ui.label("Select a clip, or import WAV / FLAC / MP3 / OGG to get started.");
                    }); }
                }
            });
    }

    fn device_editor(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if let Some(id) = self.selected_track
                && let Some(track) = self.project.tracks.iter().find(|t| t.id == id)
            {
                if track.kind == TrackKind::Midi {
                    ui.group(|ui| {
                        ui.heading("♫ Built-in Sine");
                        ui.label("16-voice diagnostic MIDI instrument");
                        ui.weak("Audio engine → Mixer");
                    });
                } else {
                    ui.group(|ui| {
                        ui.heading("≈ Sample playback");
                        ui.label("In-memory audio source");
                    });
                }
                ui.label("→");
                ui.group(|ui| {
                    ui.heading("Mixer");
                    ui.label(format!("Gain {:.0}%", track.volume * 100.0));
                    ui.label(format!("Pan {:.2}", track.pan));
                });
                ui.label("→");
                ui.group(|ui| {
                    ui.heading("Master");
                    ui.label("Stereo output");
                });
            } else {
                ui.label("Select a track to inspect its signal chain.");
            }
        });
        ui.add_space(12.0);
        ui.weak("CLAP / VST3 plugin racks are not implemented yet. The device chain shown here is live engine routing.");
    }

    fn piano_editor(&mut self, ui: &mut egui::Ui, clip: &Clip) {
        let ClipKind::Midi(midi) = &clip.kind else {
            return;
        };
        const PITCH_NAMES: [&str; 12] = [
            "C", "C♯", "D", "D♯", "E", "F", "F♯", "G", "G♯", "A", "A♯", "B",
        ];
        let mut scale = clip
            .scale_override
            .or(self.project.global_scale)
            .unwrap_or(ScaleDefinition::new(PitchClass::C, ScaleMode::Major));
        let mut root = scale.root.semitone();
        let mut mode = scale.mode;
        let mut changed_scale = false;
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("PIANO ROLL")
                    .strong()
                    .color(Color32::from_rgb(112, 201, 211)),
            );
            ui.separator();
            ui.label("Root");
            egui::ComboBox::from_id_salt("piano_root")
                .selected_text(PITCH_NAMES[root as usize])
                .show_ui(ui, |ui| {
                    for (i, name) in PITCH_NAMES.iter().enumerate() {
                        if ui.selectable_value(&mut root, i as u8, *name).changed() {
                            changed_scale = true;
                        }
                    }
                });
            egui::ComboBox::from_id_salt("piano_mode")
                .selected_text(format!("{mode:?}"))
                .show_ui(ui, |ui| {
                    for value in ScaleMode::ALL {
                        if ui
                            .selectable_value(&mut mode, value, format!("{value:?}"))
                            .changed()
                        {
                            changed_scale = true;
                        }
                    }
                });
            ui.toggle_value(&mut self.fold, "Scale Fold");
            ui.label(
                egui::RichText::new("Click to add/remove sixteenth notes")
                    .size(11.0)
                    .weak(),
            );
        });
        if changed_scale {
            scale = ScaleDefinition::new(PitchClass::new(root).unwrap_or(PitchClass::C), mode);
            if let Some(clip_id) = self.selected_clip {
                for track in &mut self.project.tracks {
                    if let Some(target) = track.clips.iter_mut().find(|c| c.id == clip_id) {
                        target.scale_override = Some(scale);
                        self.dirty = true;
                        break;
                    }
                }
            }
        }
        let notes = midi.notes.clone();
        let rows = (36_u8..=96_u8)
            .rev()
            .filter(|&note| !self.fold || scale.contains_midi_note(note))
            .collect::<Vec<_>>();
        const KEY_LABEL_W: f32 = 44.0;
        const ROW_H: f32 = 15.0;
        const PIXELS_PER_16TH: f32 = 17.0;
        let width = KEY_LABEL_W + 16.0 * 4.0 * PIXELS_PER_16TH;
        let height = rows.len() as f32 * ROW_H;
        let mut toggle = None;
        egui::ScrollArea::both()
            .id_salt("piano_scroll")
            .max_height(230.0)
            .show(ui, |ui| {
                let (rect, resp) = ui.allocate_exact_size(Vec2::new(width, height), Sense::click());
                let painter = ui.painter_at(rect);
                for (i, &pitch) in rows.iter().enumerate() {
                    let y = rect.top() + i as f32 * ROW_H;
                    let in_key = scale.contains_midi_note(pitch);
                    let bg = if in_key {
                        Color32::from_rgb(32, 43, 51)
                    } else {
                        Color32::from_rgb(20, 24, 29)
                    };
                    painter.rect_filled(
                        Rect::from_min_size(
                            Pos2::new(rect.left(), y),
                            Vec2::new(width, ROW_H - 1.0),
                        ),
                        0.0,
                        bg,
                    );
                    let label = format!("{}{}", PITCH_NAMES[(pitch % 12) as usize], pitch / 12);
                    painter.text(
                        Pos2::new(rect.left() + 4.0, y + 1.0),
                        egui::Align2::LEFT_TOP,
                        label,
                        FontId::monospace(10.0),
                        Color32::from_gray(180),
                    );
                }
                for tick_16 in 0..=64 {
                    let x = rect.left() + KEY_LABEL_W + tick_16 as f32 * PIXELS_PER_16TH;
                    let is_bar = tick_16 % 16 == 0;
                    painter.line_segment(
                        [Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())],
                        Stroke::new(
                            if is_bar { 1.2_f32 } else { 0.4_f32 },
                            if is_bar {
                                Color32::from_gray(104)
                            } else {
                                Color32::from_gray(59)
                            },
                        ),
                    );
                }
                for note in &notes {
                    let Some(row) = rows.iter().position(|&pitch| pitch == note.pitch) else {
                        continue;
                    };
                    let x = rect.left()
                        + KEY_LABEL_W
                        + note.start.ticks() as f32 / (PPQ / 4) as f32 * PIXELS_PER_16TH;
                    let width = (note.duration.ticks() as f32 / (PPQ / 4) as f32 * PIXELS_PER_16TH)
                        .max(3.0);
                    let y = rect.top() + row as f32 * ROW_H + 1.0;
                    painter.rect_filled(
                        Rect::from_min_size(Pos2::new(x, y), Vec2::new(width, ROW_H - 3.0)),
                        2.0,
                        if note.muted {
                            Color32::DARK_GRAY
                        } else {
                            Color32::from_rgb(117, 203, 157)
                        },
                    );
                }
                if resp.clicked()
                    && let Some(pos) = resp.interact_pointer_pos()
                {
                    let x = pos.x - rect.left() - KEY_LABEL_W;
                    let row = ((pos.y - rect.top()) / ROW_H).floor() as usize;
                    if x >= 0.0
                        && let Some(&pitch) = rows.get(row)
                    {
                        let index = (x / PIXELS_PER_16TH).floor() as i64;
                        let tick = index.max(0) * (PPQ / 4);
                        toggle = Some((pitch, tick));
                    }
                }
            });
        if let Some((pitch, tick)) = toggle
            && let Some(id) = self.selected_clip
        {
            for track in &mut self.project.tracks {
                if let Some(target) = track.clips.iter_mut().find(|c| c.id == id)
                    && let ClipKind::Midi(midi) = &mut target.kind
                {
                    if let Some(index) = midi
                        .notes
                        .iter()
                        .position(|n| n.pitch == pitch && n.start.ticks() == tick)
                    {
                        midi.notes.remove(index);
                    } else {
                        midi.notes.push(MidiNote {
                            pitch,
                            start: MusicalTime::from_ticks(tick),
                            duration: MusicalTime::from_ticks(PPQ / 4),
                            velocity: 100,
                            muted: false,
                        });
                    }
                    let end = tick + PPQ / 4;
                    if end > target.duration.ticks() {
                        target.duration = MusicalTime::from_ticks(end);
                    }
                    self.dirty = true;
                    break;
                }
            }
            self.restart_for_edit();
        }
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
                ui.small("Space: Play/Stop • Tab: Session/Arrangement • Shift+Tab: Detail");
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
                    Detail::Devices
                } else {
                    Detail::Clip
                };
            } else {
                self.view = if self.view == View::Arrangement {
                    View::Session
                } else {
                    View::Arrangement
                };
            }
        }
        if shortcuts.0 && !ctx.wants_keyboard_input() {
            if self.playing {
                self.stop_playback();
            } else {
                self.start_playback();
            }
        }

        let dropped = ctx.input(|input| {
            input
                .raw
                .dropped_files
                .iter()
                .filter_map(|file| file.path.clone())
                .collect::<Vec<_>>()
        });
        for path in dropped {
            self.add_audio(path);
        }

        if self.playing {
            if let Some(driver) = &self.driver {
                let telemetry = driver.telemetry();
                let generated = telemetry.project_frame.load(Ordering::Relaxed);
                let delay_us = telemetry
                    .predicted_playback_delay_us
                    .load(Ordering::Relaxed);
                let delayed_frames = delay_us.saturating_mul(self.sample_rate as u64) / 1_000_000;
                self.playhead_frame = generated.saturating_sub(delayed_frames);
            }
            self.schedule_midi_ahead();
            ctx.request_repaint_after(Duration::from_millis(32));
        }

        self.studio_transport(ctx);
        self.footer(ctx);
        self.detail_panel(ctx);
        self.studio_browser(ctx);
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui
                    .selectable_label(self.view == View::Arrangement, "▤ Arrangement")
                    .clicked()
                {
                    self.view = View::Arrangement;
                }
                if ui
                    .selectable_label(self.view == View::Session, "▦ Session")
                    .clicked()
                {
                    self.view = View::Session;
                }
                ui.separator();
                ui.label(
                    egui::RichText::new("SLATE STUDIO • AUDIO/MIDI PREVIEW")
                        .size(11.0)
                        .weak(),
                );
            });
            ui.separator();
            match self.view {
                View::Arrangement => self.studio_arrangement(ui),
                View::Session => self.studio_session(ui),
            }
        });
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

include!("studio_views.rs");
