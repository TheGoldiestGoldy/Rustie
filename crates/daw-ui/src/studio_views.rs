// Slate Studio's custom editor surface. Functional architecture follows DAW
// conventions but uses original visual styling and no proprietary assets.

const TRACK_HEADER_W: f32 = 226.0;
const TIMELINE_TOP: f32 = 38.0;
const TRACK_ROW_H: f32 = 78.0;
const AUDIO_TRACK_ROW_H: f32 = 98.0;
const BG_CANVAS: Color32 = INK_0;
const BG_HEADER: Color32 = INK_2;
const GRID_FAINT: Color32 = Color32::from_rgb(29, 30, 36);
const GRID_BAR: Color32 = Color32::from_rgb(62, 57, 55);
const TEAL: Color32 = ACCENT;
const AUDIO_BLUE: Color32 = Color32::from_rgb(95, 44, 29);
const MIDI_GREEN: Color32 = Color32::from_rgb(91, 45, 30);
const OFFWHITE: Color32 = STUDIO_TEXT;
const SUNSET: Color32 = ACCENT_BRIGHT;
const RECORDING_RED: Color32 = Color32::from_rgb(255, 132, 121);
const RECORDING_FILL: Color32 = Color32::from_rgb(74, 28, 31);

fn arrangement_track_height(track: &Track) -> f32 {
    if track.kind == TrackKind::Audio { AUDIO_TRACK_ROW_H } else { TRACK_ROW_H }
}

struct InputMeterDisplay {
    track_id: u64,
    label: &'static str,
    color: Color32,
    peak: f32,
    error: Option<String>,
}

fn input_meter_fraction(peak: f32) -> f32 {
    ((20.0 * peak.max(0.001).log10() + 60.0) / 60.0).clamp(0.0, 1.0)
}

fn paint_input_meter(
    ui: &mut egui::Ui,
    rect: Rect,
    track_id: u64,
    meter: Option<&InputMeterDisplay>,
    elapsed: Option<&str>,
) {
    let meter = meter.filter(|meter| meter.track_id == track_id);
    let state = meter.map_or("Select track to meter", |meter| meter.label);
    let color = meter.map_or(STUDIO_DIM, |meter| meter.color);
    let meter_width = rect.width() * 0.44;
    let stacked = rect.width() < 148.0;
    let bar = if stacked {
        Rect::from_min_size(Pos2::new(rect.left(), rect.bottom() - 6.0), Vec2::new(rect.width(), 6.0))
    } else {
        Rect::from_center_size(Pos2::new(rect.right() - meter_width * 0.5, rect.center().y),
            Vec2::new(meter_width, 7.0))
    };
    let painter = ui.painter_at(rect);
    painter.text(
        Pos2::new(rect.left(), if stacked { rect.top() + 5.0 } else { rect.center().y }),
        egui::Align2::LEFT_CENTER,
        elapsed.unwrap_or(state),
        if elapsed.is_some() { FontId::monospace(10.0) } else { FontId::proportional(10.0) },
        if elapsed.is_some() { RECORDING_RED } else { color },
    );
    let peak = meter.map_or(0.0, |meter| meter.peak);
    let fill = input_meter_fraction(peak);
    painter.rect_filled(bar, 2.0, INK_0);
    if fill > 0.0 {
        painter.rect_filled(Rect::from_min_size(bar.min, Vec2::new(bar.width() * fill, bar.height())),
            2.0, if peak >= 1.0 { RECORDING_RED } else { ACCENT_BRIGHT });
    }
    ui.interact(rect, egui::Id::new(("track_input_meter", ui.id(), track_id)), Sense::hover())
        .on_hover_text_at_pointer(meter.and_then(|meter| meter.error.as_deref())
            .unwrap_or("Physical input peak (−60 to 0 dBFS), independent of software monitoring"));
}

fn recording_elapsed(frames: u64, sample_rate: u32) -> String {
    let tenths = frames.saturating_mul(10) / u64::from(sample_rate);
    format!("REC {:02}:{:02}.{}", tenths / 600, tenths / 10 % 60, tenths % 10)
}

struct RecordingTrackDisplay {
    track_id: u64,
    start_frame: u64,
    frames: u64,
    elapsed: String,
    frames_per_bin: u64,
    peaks: Vec<f32>,
}

fn paint_recording_waveform(painter: &egui::Painter, rect: Rect, take: &RecordingTrackDisplay, pixels_per_frame: f32) {
    let painter = painter.with_clip_rect(rect);
    let y = rect.center().y;
    let step = (take.frames_per_bin as f32 * pixels_per_frame).max(f32::EPSILON);
    let bins_per_column = (1.0 / step).ceil().max(1.0) as usize;
    let first = ((painter.clip_rect().left() - rect.left()) / step).floor().max(0.0) as usize;
    for (index, bins) in take.peaks.chunks(bins_per_column).enumerate().skip(first / bins_per_column) {
        let x = rect.left() + ((index * bins_per_column) as f32 + bins.len() as f32 * 0.5) * step;
        if x > painter.clip_rect().right() { break; }
        let peak = bins.iter().copied().fold(0.0, f32::max);
        // A display curve keeps quiet takes legible without changing the samples.
        let height = peak.clamp(0.0, 1.0).sqrt() * rect.height() * 0.43;
        painter.line_segment([Pos2::new(x, y - height), Pos2::new(x, y + height)],
            Stroke::new(1.0_f32, RECORDING_RED));
    }
}

/// Maps a click in an arrangement lane or ruler to an absolute project frame.
/// The track-control column is excluded by the caller.
fn timeline_seek_frame(
    pointer_x: f32,
    canvas_left: f32,
    zoom: f32,
    tempo_map: &daw_core::TempoMap,
    sample_rate: u32,
) -> u64 {
    let beats = ((pointer_x - canvas_left - TRACK_HEADER_W) / zoom.max(1.0)).max(0.0);
    let tick = MusicalTime::from_ticks((f64::from(beats) * PPQ as f64).round() as i64);
    tempo_map.sample_frame_at(tick, sample_rate).unwrap_or(0)
}

impl Studio {
    fn visible_input_meter(&self) -> Option<InputMeterDisplay> {
        let track_id = self.recorder.as_ref().map(|take| take.track_id).or(self.selected_track)?;
        if !self.project.tracks.iter().any(|track| track.id == track_id && track.kind == TrackKind::Audio) {
            return None;
        }
        let (label, color, peak, error) = match self.track_input_status(track_id) {
            TrackInputStatus::Selected => ("Input selected", STUDIO_DIM, 0.0, None),
            TrackInputStatus::Ready { peak } if peak > 0.001 => ("Input signal", ACCENT_BRIGHT, peak, None),
            TrackInputStatus::Ready { peak } => ("No input signal", STUDIO_DIM, peak, None),
            TrackInputStatus::Unavailable(error) => ("Input unavailable", RECORDING_RED, 0.0, Some(error.to_owned())),
        };
        Some(InputMeterDisplay { track_id, label, color, peak, error })
    }

    fn recording_track_display(&self) -> Option<RecordingTrackDisplay> {
        let take = self.recorder.as_ref()?;
        let (frames_per_bin, peaks) = take.preview.snapshot();
        Some(RecordingTrackDisplay {
            track_id: take.track_id,
            start_frame: take.start_frame,
            frames: take.progress_frames,
            elapsed: recording_elapsed(take.progress_frames, self.sample_rate),
            frames_per_bin,
            peaks,
        })
    }

    fn studio_transport(&mut self, ctx: &egui::Context) {
        let mut action: Option<&'static str> = None;
        egui::TopBottomPanel::top("slate_main_transport")
            .frame(egui::Frame::default()
                .fill(INK_1)
                .stroke(Stroke::new(0.8_f32, STUDIO_BORDER))
                .inner_margin(egui::Margin::symmetric(10, 7)))
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing = Vec2::new(4.0, 4.0);
                ui.spacing_mut().button_padding = Vec2::new(7.0, 4.0);
                ui.horizontal_wrapped(|ui| {
                    ui.label(egui::RichText::new("◈").size(18.0).color(ACCENT));
                    ui.label(egui::RichText::new("RUSTY").size(16.0)
                        .strong().color(STUDIO_TEXT));
                    ui.label(egui::RichText::new(format!("{} · {}",
                        crate::RUSTY_DISPLAY_VERSION, crate::RUSTY_BUILD_SHA))
                        .size(10.0).color(STUDIO_DIM))
                        .on_hover_text("Exact compiled version and source commit of this Rusty build");
                    ui.separator();
                    ui.menu_button("File", |ui| {
                        if ui.button("Open project…    Ctrl+O").clicked() {
                            action = Some("open"); ui.close();
                        }
                        if ui.button("Save project      Ctrl+S").clicked() {
                            action = Some("save"); ui.close();
                        }
                        if ui.button("Save as…").clicked() {
                            action = Some("save_as"); ui.close();
                        }
                        if ui.button("Recover this project's autosave…")
                            .on_hover_text("Recover the most recent automatic backup for this project. Use Save As afterwards.")
                            .clicked() { action = Some("recover"); ui.close(); }
                        if ui.button("Export Arrangement to WAV…")
                            .on_hover_text("Exports Arrangement clips and built-in effects. Live plug-ins must be bypassed or printed to audio first.").clicked() {
                            action = Some("export"); ui.close();
                        }
                        ui.separator();
                        if ui.button("Import audio…").clicked() {
                            action = Some("audio"); ui.close();
                        }
                        if ui.button("New MIDI track").clicked() {
                            action = Some("midi"); ui.close();
                        }
                        if ui.button("New group bus").clicked() {
                            action = Some("group"); ui.close();
                        }
                    });
                    ui.menu_button("Edit", |ui| {
                        if ui.add_enabled(self.history.can_undo(),
                            egui::Button::new("Undo project edit    Ctrl+Z"))
                            .on_hover_text("Undo the last project change, including notes, clips and track settings")
                            .clicked() { action = Some("undo"); ui.close(); }
                        if ui.add_enabled(self.history.can_redo(),
                            egui::Button::new("Redo project edit    Ctrl+Shift+Z"))
                            .on_hover_text("Reapply an undone project change")
                            .clicked() { action = Some("redo"); ui.close(); }
                        ui.separator();
                        if ui.add_enabled(self.selected_clip.is_some(),
                            egui::Button::new("Copy selection    Ctrl+C"))
                            .on_hover_text("Shift-click clips to select several. Copy preserves their spacing and tracks.")
                            .clicked() { action = Some("copy_clip"); ui.close(); }
                        if ui.add_enabled(!self.clip_clipboard.is_empty(),
                            egui::Button::new("Paste    Ctrl+V"))
                            .on_hover_text("Paste at the playhead. A multi-clip selection returns to its original tracks.")
                            .clicked() { action = Some("paste_clip"); ui.close(); }
                        if ui.add_enabled(self.selected_clip.is_some(),
                            egui::Button::new("Duplicate selection    Ctrl+D"))
                            .clicked() { action = Some("duplicate_clip"); ui.close(); }
                        if ui.add_enabled(self.view == View::Arrangement && self.selected_clip.is_some(),
                            egui::Button::new("Split at playhead    S"))
                            .clicked() { action = Some("split_clip"); ui.close(); }
                        if ui.add_enabled(self.selected_clip.is_some(),
                            egui::Button::new("Delete selection    Del"))
                            .on_hover_text("Delete selected clips. Ctrl+Z restores them.")
                            .clicked() { action = Some("delete_clip"); ui.close(); }
                        ui.separator();
                        if ui.button("Import audio").clicked() {
                            action = Some("audio"); ui.close();
                        }
                        if ui.button("Create MIDI track").clicked() {
                            action = Some("midi"); ui.close();
                        }
                    });
                    let devices = ui.selectable_label(self.show_devices, "Devices")
                        .on_hover_text("F3 opens Devices. Right-click to go directly to a section.");
                    if devices.clicked() || ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::F3)) {
                        self.show_devices = !self.show_devices;
                    }
                    devices.context_menu(|ui| {
                        for (tab, label) in [(DeviceTab::Effects, "FX Chain"), (DeviceTab::Plugins, "Plug-ins"), (DeviceTab::Hardware, "Audio / MIDI")] {
                            if ui.button(label).clicked() {
                                self.device_tab = tab;
                                self.show_devices = true;
                                ui.close();
                            }
                        }
                    });
                    if ui.button("Help").on_hover_text(
                        "Open the short, skippable Rusty tutorial").clicked() {
                        self.show_tutorial = true;
                    }
                    ui.separator();
                    let arrangement = self.view == View::Arrangement;
                    if ui.add(egui::Button::new(egui::RichText::new("Arrangement")
                        .size(12.0).color(if arrangement { ACCENT_BRIGHT } else { STUDIO_TEXT }))
                        .fill(if arrangement { ACCENT_TINT } else { INK_2 })
                        .stroke(Stroke::new(0.8_f32,
                            if arrangement { ACCENT_DARK } else { STUDIO_BORDER }))).on_hover_text("Edit the song timeline · Tab switches views")
                        .clicked() {
                        self.change_view(View::Arrangement);
                    }
                    let session = self.view == View::Session;
                    if ui.add(egui::Button::new(egui::RichText::new("Session")
                        .size(12.0).color(if session { ACCENT_BRIGHT } else { STUDIO_TEXT }))
                        .fill(if session { ACCENT_TINT } else { INK_2 })
                        .stroke(Stroke::new(0.8_f32,
                            if session { ACCENT_DARK } else { STUDIO_BORDER }))).on_hover_text("Launch clips and scenes · Tab switches views")
                        .clicked() {
                        self.change_view(View::Session);
                    }
                    ui.separator();
                    if ui.small_button("+ Audio").on_hover_text("Import audio into selected audio track (or create a new track)").clicked() {
                        action = Some("audio");
                    }
                    if ui.small_button("+ MIDI").on_hover_text("New MIDI instrument track").clicked() {
                        action = Some("midi");
                    }
                    if ui.small_button("+ Group").on_hover_text("Create an empty audio group bus").clicked() {
                        action = Some("group");
                    }
                    ui.separator();
                    if ui.add(egui::Button::new("|◀").min_size(Vec2::new(29.0, 32.0)))
                        .on_hover_text("Return to the beginning (Home)").clicked() {
                        self.playhead_frame = 0;
                        action = Some("refresh");
                    }
                    if ui.add(egui::Button::new("■").min_size(Vec2::new(29.0, 32.0)))
                        .on_hover_text("Stop").clicked() { action = Some("stop"); }
                    let is_playing = self.playing;
                    if ui.add(egui::Button::new(
                        egui::RichText::new(if is_playing { "II" } else { "▶" })
                            .size(16.0).color(INK_0).strong())
                            .fill(ACCENT).stroke(Stroke::new(0.8_f32, ACCENT_BRIGHT))
                            .min_size(Vec2::new(43.0, 32.0)))
                        .on_hover_text(if is_playing { "Pause playback" } else { "Play (Space)" }).clicked() {
                        action = Some(if is_playing { "pause" } else { "play" });
                    }
                    let recording = self.recorder.is_some() || self.midi_take.is_some();
                    let midi_track_selected = self.selected_track.is_some_and(|id| self.project.tracks.iter()
                        .any(|track| track.id == id && track.kind == TrackKind::Midi));
                    if ui.add(egui::Button::new(
                        egui::RichText::new(if recording { "■ REC" } else { "● REC" })
                            .size(12.0).color(OFFWHITE))
                            .fill(if recording { SUNSET } else { Color32::from_rgb(95, 30, 30) }))
                        .on_hover_text(if self.midi_take.is_some() {
                            "Finish MIDI take and add an editable clip to the selected MIDI track"
                        } else if self.recorder.is_some() {
                            "Stop and save recording as WAV on the selected audio track"
                        } else if midi_track_selected {
                            "Record keyboard notes into a new MIDI clip (connect MIDI in Devices)"
                        } else {
                            "Record microphone into selected audio track; creates a track if needed"
                        }).clicked() { action = Some("record"); }
                    if let Some(take) = self.recorder.as_ref() {
                        let dropped = take.dropped.load(Ordering::Relaxed);
                        ui.label(egui::RichText::new(recording_elapsed(take.progress_frames, self.sample_rate))
                            .monospace().size(11.0).color(RECORDING_RED))
                            .on_hover_text("Actual captured microphone frames, not transport time");
                        if dropped != 0 {
                            ui.colored_label(Color32::from_rgb(255, 120, 100),
                                format!("{} LOST", dropped))
                                .on_hover_text("Input ring buffer overrun: check disk load and recording source");
                        }
                    }
                    ui.separator();
                    ui.allocate_ui_with_layout(
                        Vec2::new(164.0, ui.spacing().interact_size.y),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| {
                            let click_on = self.project.metronome_enabled;
                            if ui.add(egui::Button::new(if click_on { "♩ CLICK ON" } else { "♩ CLICK" })
                                .fill(if click_on { ACCENT_TINT } else { INK_2 }))
                                .on_hover_text("Toggle the audible metronome. Beat one is accented; click follows tempo and time signature.")
                                .clicked() {
                                self.project.metronome_enabled = !click_on;
                                self.dirty = true;
                                if let Some(driver) = self.driver.as_mut() {
                                    let _ = driver.controller().immediate(EngineCommand::RampGain {
                                        node: METRONOME_ID,
                                        target_gain: if click_on { 0.0 } else {
                                            self.project.metronome_volume.clamp(0.0, 1.0)
                                        },
                                        duration_frames: 128,
                                    });
                                }
                            }
                            let previous_click_volume = self.project.metronome_volume;
                            ui.spacing_mut().slider_width = 83.0;
                            ui.add(egui::Slider::new(
                                &mut self.project.metronome_volume, 0.0..=1.0
                            ).show_value(false)).on_hover_text(
                                format!("Metronome volume: {:.0}%. Adjust independently of the mix.",
                                    previous_click_volume * 100.0)
                            );
                            if self.project.metronome_volume != previous_click_volume {
                                self.dirty = true;
                                if let Some(driver) = self.driver.as_mut() {
                                    let _ = driver.controller().immediate(EngineCommand::RampGain {
                                        node: METRONOME_ID,
                                        target_gain: if self.project.metronome_enabled {
                                            self.project.metronome_volume
                                        } else { 0.0 },
                                        duration_frames: 128,
                                    });
                                }
                            }
                        },
                    );
                    ui.separator();
                    let bpm = self.project.tempo_map.tempo_at(MusicalTime::ZERO)
                        .map_or(120.0, |tempo| tempo.bpm());
                    let mut edited_bpm = bpm;
                    if ui.add_enabled(!recording, egui::DragValue::new(&mut edited_bpm).range(20.0..=300.0)
                        .speed(0.25).fixed_decimals(1).suffix(" BPM")).on_hover_text("Drag to change tempo in beats per minute").changed()
                        && let Ok(tempo) = Tempo::from_bpm(edited_bpm) {
                            self.project.tempo_map = daw_core::TempoMap::new(tempo);
                            self.dirty = true;
                            action = Some("refresh");
                        }
                    let mut numerator = self.project.time_signature.numerator;
                    let mut denominator = self.project.time_signature.denominator;
                    ui.add_enabled_ui(!recording, |ui| {
                    egui::ComboBox::from_id_salt("sig_numerator")
                        .selected_text(format!("{numerator}"))
                        .width(34.0).show_ui(ui, |ui| {
                            for value in 1..=16 {
                                ui.selectable_value(&mut numerator, value, format!("{value}"));
                            }
                        }).response.on_hover_text("Time signature: beats per bar");
                    ui.label("/");
                    egui::ComboBox::from_id_salt("sig_denominator")
                        .selected_text(format!("{denominator}"))
                        .width(34.0).show_ui(ui, |ui| {
                            for value in [1, 2, 4, 8, 16] {
                                ui.selectable_value(&mut denominator, value, format!("{value}"));
                            }
                        }).response.on_hover_text("Time signature: note value of each beat");
                    });
                    if let Ok(sig) = TimeSignature::new(numerator, denominator)
                        && sig != self.project.time_signature {
                        self.project.time_signature = sig;
                        self.dirty = true;
                        action = Some("refresh");
                    }
                    ui.separator();
                    let beats = self.tick_at_frame(self.playhead_frame) as f64 / PPQ as f64;
                    let bars = (beats / self.beats_per_bar()).floor() as u64 + 1;
                    let beat = ((beats % self.beats_per_bar())
                        * self.project.time_signature.denominator as f64 / 4.0).floor() as u64 + 1;
                    ui.label(egui::RichText::new(format!("{bars:02}.{beat}.1"))
                        .size(18.0).monospace().strong().color(STUDIO_TEXT));
                    ui.label(egui::RichText::new(self.time_display())
                        .size(12.0).monospace().color(STUDIO_DIM))
                        .on_hover_text("Current transport position in minutes and seconds");
                    let end = self.max_project_seconds().max(10.0) as f32;
                    let frame = self.seek_preview_frame.unwrap_or(self.playhead_frame);
                    let mut position = (frame as f32 / self.sample_rate as f32).min(end);
                    let response = ui.add(egui::Slider::new(&mut position, 0.0..=end)
                        .show_value(false).trailing_fill(true).text("Seek"))
                        .on_hover_text("Click or drag to seek; audio resumes after release");
                    if response.changed() {
                        self.seek_preview_frame = Some((position as f64 * self.sample_rate as f64) as u64);
                    }
                    if (response.drag_stopped() || (response.changed() && !response.dragged()))
                        && let Some(target) = self.seek_preview_frame.take() { self.seek_to_frame(target); }
                    ui.separator();
                    ui.allocate_ui_with_layout(
                        Vec2::new(129.0, ui.spacing().interact_size.y),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| {
                            let previous = self.backend;
                            ui.add_enabled_ui(!recording, |ui| {
                            egui::ComboBox::from_id_salt("slate_output_backend")
                                .selected_text(match self.backend {
                                    Backend::Alsa => "ALSA",
                                    Backend::PulseAudio => "Pulse",
                                    Backend::Jack => "JACK",
                                })
                                .width(69.0)
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(&mut self.backend, Backend::Alsa,
                                        "ALSA · Low latency");
                                    ui.selectable_value(&mut self.backend, Backend::PulseAudio,
                                        "PulseAudio · Buffered");
                                    ui.selectable_value(&mut self.backend, Backend::Jack,
                                        "PipeWire / JACK");
                                }).response.on_hover_text("Select the output audio backend");
                            });
                            if self.backend != previous { action = Some("refresh"); }
                            let peak = self.driver.as_ref().map_or(0.0, |d| f32::from_bits(
                                d.telemetry().current_peak_bits.load(Ordering::Relaxed)
                            )).clamp(0.0, 1.0);
                            ui.add(egui::ProgressBar::new(peak)
                                .desired_width(51.0).fill(ACCENT))
                                .on_hover_text("Master output peak level");
                        },
                    );
                    ui.separator();
                    if self.dirty {
                        ui.label(egui::RichText::new("●").color(ACCENT));
                    }
                    ui.add(egui::TextEdit::singleline(&mut self.project.name)
                        .desired_width(113.0).hint_text("Project")).on_hover_text("Project name");
                    if ui.small_button("Save").on_hover_text("Save project (Ctrl+S)").clicked() { action = Some("save"); }
                });
            });
        match action {
            Some("play") => self.start_playback(),
            Some("pause") => self.pause_playback(),
            Some("stop") => self.stop_playback(),
            Some("refresh") => self.restart_for_edit(),
            Some("open") => self.open_project(),
            Some("save") => self.save_project(false),
            Some("save_as") => self.save_project(true),
            Some("recover") => self.recover_autosave(),
            Some("audio") => {
                if let Some(path) = Self::pick_path(false, true) { self.import_audio(path); }
            }
            Some("midi") => self.add_midi_track(),
            Some("record") => self.toggle_recording(),
            Some("undo") => self.history_step(false),
            Some("redo") => self.history_step(true),
            Some("copy_clip") => { self.copy_selected_clip(); }
            Some("paste_clip") => self.paste_clip(),
            Some("duplicate_clip") => self.duplicate_selected_clip(),
            Some("split_clip") => self.split_selected_clip(),
            Some("delete_clip") => self.delete_selected_clip(),
            Some("export") => self.start_mix_export(),
            Some("group") => self.apply_track_action(TrackAction::NewGroup(None)),
            _ => {}
        }
    }
}

impl Studio {
    fn studio_arrangement(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("ARRANGEMENT").strong().color(OFFWHITE).size(12.0));
            ui.separator();
            ui.label(egui::RichText::new(format!(
                "{} tracks  •  {} clips", self.project.tracks.len(), self.clip_count()
            )).weak().size(11.0));
            ui.separator();
            ui.toggle_value(&mut self.snap, "Snap 1/16").on_hover_text("Q toggles sixteenth-note snapping");
            ui.separator();
            ui.weak("−");
            ui.add(egui::Slider::new(&mut self.zoom, 28.0..=140.0)
                .show_value(false).text("Zoom").trailing_fill(true)).on_hover_text("Change horizontal timeline scale");
            ui.weak("+");
            ui.separator();
            if ui.available_width() > 360.0 { ui.label(egui::RichText::new(
                "Click ruler, lane or clip to seek · Shift-click selects · drag to move or trim"
            ).size(10.0).weak()); }
        });
        ui.separator();
        let recording = self.recorder.is_some() || self.midi_take.is_some();
        if recording {
            ui.colored_label(RECORDING_RED,
                "Seeking is disabled during a take. Stop recording before moving the playhead.");
        }

        let tempo = self.project.tempo_map.tempo_at(MusicalTime::ZERO)
            .map_or(120.0, |t| t.bpm());
        let seconds_per_beat = 60.0 / tempo;
        let input_meter = self.visible_input_meter();
        let take = self.recording_track_display();
        let recording_end = take.as_ref().map_or(0.0, |take| {
            take.start_frame.saturating_add(take.frames) as f64 / f64::from(self.sample_rate)
        });
        let end_seconds = self.max_project_seconds().max(recording_end).max(32.0);
        let beats = (self.tick_at_frame((end_seconds * f64::from(self.sample_rate)) as u64)
            .div_euclid(PPQ) as usize + 24).clamp(64, 3000);
        let width = TRACK_HEADER_W + beats as f32 * self.zoom + 12.0;
        let track_count = self.project.tracks.len();
        let track_height = self.project.tracks.iter().map(arrangement_track_height).sum::<f32>();
        let height = (TIMELINE_TOP + track_height.max(TRACK_ROW_H) + TRACK_ROW_H)
            .max(ui.available_height() - 4.0);
        let mut clip_click = None;
        let mut drag_selection = None;
        let mut clicked_empty_lane = false;
        let mut selected_track = None;
        let mut changed_tracks = Vec::new();
        let mut drop_move = None;
        let move_preview = self.clip_drag_preview.map_or(0.0, |(id, delta)| {
            if self.arrangement_clip_selected(id) {
                self.arrangement_move_delta(delta) as f32 / PPQ as f32 * self.zoom
            } else {
                0.0
            }
        });
        let mut drop_trim: Option<(u64, ClipTrimEdge, f32)> = None;
        let mut track_action: Option<TrackAction> = None;
        let mut clip_action: Option<ClipAction> = None;
        let mut open_recording_for: Option<u64> = None;
        let primary = self.selected_clip;
        let selected_ids = &self.arrangement_selection;
        let is_clip_selected = |id| {
            primary == Some(id)
                || (selected_ids.last().copied() == primary && selected_ids.contains(&id))
        };
        let groups = self.project.tracks.iter()
            .filter(|track| track.kind == TrackKind::Group)
            .map(|track| (track.id, track.name.clone())).collect::<Vec<_>>();
        let mut seek_to = None;
        let bar_ticks = self.bar_ticks();
        let frames_per_tick = self.sample_rate as f64 * seconds_per_beat / PPQ as f64;
        let play_tick = self.tick_at_frame(self.playhead_frame);
        let tempo_map = &self.project.tempo_map;
        ui.input(|input| {
            if input.pointer.primary_pressed() {
                self.arrangement_plain_press = input.modifiers == egui::Modifiers::NONE;
            }
            if input.modifiers != egui::Modifiers::NONE || input.pointer.secondary_down() {
                self.arrangement_plain_press = false;
            }
        });
        let plain_click = self.arrangement_plain_press;

        egui::ScrollArea::both()
            .id_salt("slate_arrangement_surface")
            // Dragging clips must not also drag the entire viewport sideways.
            .scroll_source(egui::scroll_area::ScrollSource::SCROLL_BAR |
                egui::scroll_area::ScrollSource::MOUSE_WHEEL)
            .auto_shrink([false, false])
            .show_viewport(ui, |ui, viewport| {
                let (canvas, canvas_response) = ui.allocate_exact_size(
                    Vec2::new(width, height), Sense::click()
                );
                let viewport_clip = ui.clip_rect();
                let ruler_y = canvas.top() + viewport.min.y;
                // Pin the ruler in the viewport, with the same horizontally
                // scrolled origin as the lanes. Clip lane painting AND input below it.
                let ruler_band = Rect::from_min_size(
                    Pos2::new(canvas.left(), ruler_y), Vec2::new(canvas.width(), TIMELINE_TOP));
                ui.set_clip_rect(viewport_clip.intersect(Rect::from_min_max(
                    Pos2::new(viewport_clip.left(), ruler_band.bottom()), viewport_clip.max)));
                let painter = ui.painter_at(canvas);
                painter.rect_filled(canvas, 0.0, BG_CANVAS);
                let header = Rect::from_min_max(canvas.min,
                    Pos2::new(canvas.left() + TRACK_HEADER_W, canvas.bottom()));
                painter.rect_filled(header, 0.0, BG_HEADER);

                let first_beat = ((ui.clip_rect().left() - canvas.left() - TRACK_HEADER_W)
                    / self.zoom).floor().max(0.0) as usize;
                let last_beat = ((ui.clip_rect().right() - canvas.left() - TRACK_HEADER_W)
                    / self.zoom).ceil().max(0.0) as usize + 1;
                for beat in first_beat..=last_beat.min(beats) {
                    let x = canvas.left() + TRACK_HEADER_W + beat as f32 * self.zoom;
                    let downbeat = false; // exact bar boundaries are drawn below
                    painter.line_segment(
                        [Pos2::new(x, canvas.top() + TIMELINE_TOP),
                         Pos2::new(x, canvas.bottom())],
                        Stroke::new(if downbeat { 1.1_f32 } else { 0.7_f32 },
                            if downbeat { GRID_BAR } else { GRID_FAINT }),
                    );
                    if self.zoom > 44.0 {
                        for quarter in 1..4 {
                            let xsub = x + self.zoom * quarter as f32 / 4.0;
                            painter.line_segment(
                                [Pos2::new(xsub, canvas.top() + TIMELINE_TOP),
                                 Pos2::new(xsub, canvas.bottom())],
                                Stroke::new(0.4_f32, GRID_FAINT),
                            );
                        }
                    }
                }

                let beats_per_bar = bar_ticks as f32 / PPQ as f32;
                let first_bar = (first_beat as f32 / beats_per_bar).floor() as usize;
                let last_bar = (last_beat as f32 / beats_per_bar).ceil() as usize;
                for bar in first_bar..=last_bar {
                    let x = canvas.left() + TRACK_HEADER_W + bar as f32 * beats_per_bar * self.zoom;
                    painter.line_segment([Pos2::new(x, canvas.top() + TIMELINE_TOP),
                        Pos2::new(x, canvas.bottom())], Stroke::new(1.35_f32, GRID_BAR));
                }

                let mut next_row_y = canvas.top() + TIMELINE_TOP;
                for (track_idx, track) in self.project.tracks.iter_mut().enumerate() {
                    let row_y = next_row_y;
                    let row_h = arrangement_track_height(track);
                    next_row_y += row_h;
                    let row_rect = Rect::from_min_size(
                        Pos2::new(canvas.left(), row_y),
                        Vec2::new(canvas.width(), row_h),
                    );
                    painter.rect_filled(row_rect, 0.0,
                        if track_idx % 2 == 0 {
                            Color32::from_rgb(12, 13, 16)
                        } else { Color32::from_rgb(16, 17, 21) });
                    for beat in first_beat..=last_beat.min(beats) {
                        let x = canvas.left() + TRACK_HEADER_W + beat as f32 * self.zoom;
                        // Bar boundaries come from the actual time signature,
                        // so 6/8 and other meters do not highlight every 4/4.
                        let is_bar = (beat as i64 * PPQ).rem_euclid(bar_ticks.max(1)) == 0;
                        painter.line_segment([
                            Pos2::new(x, row_y),
                            Pos2::new(x, row_y + row_h),
                        ], Stroke::new(if is_bar { 1.1_f32 } else { 0.65_f32 },
                            if is_bar { GRID_BAR } else { GRID_FAINT }));
                    }
                    for bar in first_bar..=last_bar {
                        let x = canvas.left() + TRACK_HEADER_W + bar as f32 * beats_per_bar * self.zoom;
                        painter.line_segment([Pos2::new(x, row_y), Pos2::new(x, row_y + row_h)],
                            Stroke::new(1.2_f32, GRID_BAR));
                    }
                    let head_rect = Rect::from_min_size(row_rect.min,
                        Vec2::new(TRACK_HEADER_W, row_h));
                    painter.rect_filled(head_rect, 0.0,
                        if self.selected_track == Some(track.id) {
                            ACCENT_TINT
                        } else {
                            INK_2
                        });
                    painter.line_segment(
                        [Pos2::new(row_rect.left(), row_rect.bottom()),
                         Pos2::new(row_rect.right(), row_rect.bottom())],
                        Stroke::new(0.6_f32, STUDIO_BORDER),
                    );
                    // The timeline *lane* belongs to this track, including
                    // empty spaces between clips. Right-click anywhere here
                    // must show this track's actions, not "create new track".
                    let lane = Rect::from_min_size(
                        Pos2::new(canvas.left() + TRACK_HEADER_W, row_y),
                        Vec2::new((canvas.right() - canvas.left() - TRACK_HEADER_W).max(1.0), row_h)
                    );
                    let lane_response = ui.interact(lane,
                        egui::Id::new(("arrangement_track_lane", track.id)), Sense::click());
                    let id = track.id;
                    let kind = track.kind;
                    let parent = track.group_id;
                    lane_response.context_menu(|ui| track_context_menu(
                        ui, id, kind, parent, &groups, &mut track_action));
                    if lane_response.clicked() {
                        selected_track = Some(id);
                        if let Some(pos) = lane_response.interact_pointer_pos() {
                            // The clip owns selection and seeking inside its body.
                            // Keep the lane fallback out of clip and trim hit targets.
                            let over_clip = track.clips.iter().any(|clip| {
                                let start = clip.start.ticks() as f32 / PPQ as f32;
                                let duration = clip.duration.ticks() as f32 / PPQ as f32;
                                let start_x = canvas.left() + TRACK_HEADER_W + start * self.zoom;
                                let full_width = (duration * self.zoom).max(14.0);
                                let preview_x = if is_clip_selected(clip.id) {
                                    move_preview
                                } else { 0.0 };
                                let trim_preview = self.clip_resize_preview
                                    .filter(|(clip_id, _)| *clip_id == clip.id)
                                    .zip(self.clip_resize_drag)
                                    .map(|((_, delta), edge)| (edge, delta));
                                let (preview_left, preview_width) = clip_preview_bounds(
                                    start_x, full_width, preview_x, trim_preview, 14.0);
                                let x = preview_left.max(canvas.left() + TRACK_HEADER_W);
                                pos.x >= x && pos.x <= x + preview_width
                                    && pos.y >= row_y + 6.0
                                    && pos.y <= row_y + row_h - 6.0
                            });
                            if !over_clip {
                                clicked_empty_lane = true;
                                if plain_click {
                                    seek_to = Some(timeline_seek_frame(
                                        pos.x, canvas.left(), self.zoom,
                                        tempo_map, self.sample_rate,
                                    ));
                                }
                            }
                        }
                    }
                    let color = match track.kind {
                        TrackKind::Midi => ACCENT,
                        TrackKind::Group => CLIP_PURPLE,
                        _ => CLIP_PINK,
                    };
                    painter.rect_filled(
                        Rect::from_min_size(row_rect.min, Vec2::new(4.0, row_h)),
                        0.0, color);
                    let track_tag = match track.kind {
                        TrackKind::Midi => "♫",
                        TrackKind::Group => "▣",
                        _ => "≈",
                    };
                    let track_name = if track.group_id.is_some() {
                        format!("↳ {}", track.name)
                    } else { track.name.clone() };
                    let title_width = if track.kind == TrackKind::Audio {
                        123.0
                    } else {
                        151.0
                    };
                    let title_rect = Rect::from_min_size(
                        row_rect.min + Vec2::new(11.0, 7.0),
                        Vec2::new(title_width, 24.0)
                    );
                    let title_response = ui.put(title_rect, egui::Button::new(
                        egui::RichText::new(format!("{track_tag}  {track_name}"))
                            .color(OFFWHITE).size(12.0)
                    ).frame(false));
                    if title_response.clicked() {
                        selected_track = Some(track.id);
                    }
                    let id = track.id;
                    let kind = track.kind;
                    let parent = track.group_id;
                    title_response.context_menu(|ui| track_context_menu(
                        ui, id, kind, parent, &groups, &mut track_action));
                    if track.kind == TrackKind::Audio {
                        let input_name = track.record_input.as_deref().unwrap_or("Auto");
                        let channels = track.record_input_channels.label();
                        if ui
                            .put(
                                Rect::from_min_size(
                                    Pos2::new(row_rect.left() + 136.0, row_y + 7.0),
                                    Vec2::new(25.0, 22.0),
                                ),
                                |ui: &mut egui::Ui| {
                                    ui.spacing_mut().button_padding = Vec2::new(4.0, 2.0);
                                    ui.add(egui::Button::new(egui::RichText::new("IN").size(9.0))
                                        .small().fill(INK_3))
                                },
                            )
                            .on_hover_text(format!("{input_name} · {channels}\nChoose recording input and channels"))
                            .clicked()
                        {
                            open_recording_for = Some(track.id);
                        }
                    }
                    let mut btn = |label: &str, x: f32, on: bool, active: Color32| {
                        ui.put(Rect::from_min_size(
                            Pos2::new(row_rect.left() + x, row_y + 7.0),
                            Vec2::new(22.0, 22.0)),
                            egui::Button::new(egui::RichText::new(label).size(10.0)
                                .color(if on { Color32::BLACK } else { OFFWHITE }))
                                .fill(if on { active } else { INK_3 })
                        ).clicked()
                    };
                    if btn("M", 163.0, track.muted, SUNSET) {
                        track.muted = !track.muted;
                        changed_tracks.push(track.id);
                    }
                    if btn("S", 189.0, track.solo, TEAL) {
                        track.solo = !track.solo;
                        changed_tracks.push(track.id);
                    }
                    painter.text(Pos2::new(row_rect.left() + 12.0, row_y + 46.0),
                        egui::Align2::LEFT_CENTER, "VOL",
                        FontId::proportional(9.0), Color32::GRAY);
                    if ui.put(Rect::from_min_size(
                        Pos2::new(row_rect.left() + 41.0, row_y + 35.0),
                        Vec2::new(120.0, 22.0)),
                        egui::Slider::new(&mut track.volume, 0.0..=1.5)
                            .show_value(false).trailing_fill(true)).changed() {
                        changed_tracks.push(track.id);
                    }
                    painter.text(Pos2::new(row_rect.left() + 169.0, row_y + 46.0),
                        egui::Align2::LEFT_CENTER, format!("{:.0}%", track.volume * 100.0),
                        FontId::monospace(10.0), OFFWHITE);
                    painter.text(Pos2::new(row_rect.left() + 12.0, row_y + 67.0),
                        egui::Align2::LEFT_CENTER, "PAN",
                        FontId::proportional(9.0), Color32::GRAY);
                    if ui.put(Rect::from_min_size(
                        Pos2::new(row_rect.left() + 41.0, row_y + 56.0),
                        Vec2::new(120.0, 22.0)),
                        egui::Slider::new(&mut track.pan, -1.0..=1.0)
                            .show_value(false)).changed() {
                        changed_tracks.push(track.id);
                    }
                    painter.text(Pos2::new(row_rect.left() + 169.0, row_y + 67.0),
                        egui::Align2::LEFT_CENTER,
                        if track.pan.abs() < 0.02 { "C".into() } else if track.pan < 0.0 {
                            format!("L{:.0}", track.pan.abs() * 100.0)
                        } else { format!("R{:.0}", track.pan * 100.0) },
                        FontId::monospace(10.0), OFFWHITE);
                    let active_take = take.as_ref().filter(|take| take.track_id == track.id);
                    if track.kind == TrackKind::Audio {
                        let meter_rect = Rect::from_min_size(row_rect.min + Vec2::new(12.0, 77.0),
                            Vec2::new(TRACK_HEADER_W - 26.0, 17.0));
                        paint_input_meter(ui, meter_rect, track.id, input_meter.as_ref(),
                            active_take.map(|take| take.elapsed.as_str()));
                    }

                    for clip in &track.clips {
                        let start = clip.start.ticks() as f32 / PPQ as f32;
                        let duration = clip.duration.ticks() as f32 / PPQ as f32;
                        let start_x = canvas.left() + TRACK_HEADER_W + start * self.zoom;
                        let full_width = (duration * self.zoom).max(14.0);
                        let is_selected = is_clip_selected(clip.id);
                        let preview_delta = if is_selected { move_preview } else { 0.0 };
                        let trim_preview = self.clip_resize_preview
                            .filter(|(id,_)| *id == clip.id)
                            .zip(self.clip_resize_drag)
                            .map(|((_, delta), edge)| (edge, delta));
                        let (preview_left, shown_width) = clip_preview_bounds(
                            start_x, full_width, preview_delta, trim_preview, 14.0);
                        let x = preview_left.max(canvas.left() + TRACK_HEADER_W);
                        let clip_rect = Rect::from_min_size(
                            Pos2::new(x, row_y + 6.0),
                            Vec2::new(shown_width, row_h - 12.0));
                        let base = if track.kind == TrackKind::Midi {
                            match track_idx % 3 { 0 => MIDI_GREEN, 1 => CLIP_PURPLE, _ => ACCENT_DARK }
                        } else if track_idx % 2 == 0 { AUDIO_BLUE } else { CLIP_PINK };
                        painter.rect_filled(clip_rect, 7.0,
                            if track.muted { INK_3 } else { base });
                        painter.rect_filled(Rect::from_min_size(clip_rect.min,
                            Vec2::new(clip_rect.width(), 2.0)), 1.0,
                            if track.kind == TrackKind::Midi { TEAL } else {
                                ACCENT_BRIGHT
                            });
                        if is_selected {
                            painter.rect_stroke(clip_rect, 7.0,
                                Stroke::new(2.0_f32, ACCENT_BRIGHT),
                                egui::StrokeKind::Inside);
                        }
                        let clip_title = match &clip.kind {
                            ClipKind::Audio(a) => a.source.file_name()
                                .unwrap_or_default().to_string_lossy().to_string(),
                            ClipKind::Midi(_) => format!("♫  {}", track.name),
                        };
                        painter.text(clip_rect.min + Vec2::new(9.0, 10.0),
                            egui::Align2::LEFT_TOP,
                            clip_title, FontId::proportional(12.0), Color32::WHITE);
                        match &clip.kind {
                            ClipKind::Audio(a) => {
                                if let Some(audio) = self.media.get(&a.source) {
                                    let peaks = &audio.peaks;
                                    if !peaks.is_empty() {
                                        let cols = ((clip_rect.width() / 3.0) as usize).clamp(1, 550);
                                        let middle_y = clip_rect.center().y + 9.0;
                                        let usable_h = clip_rect.height() * 0.28;
                                        for col in 0..cols {
                                            let source_start = a.source_offset_frames.min(audio.samples.len() as u64);
                                            let length_frames = ((clip.duration.ticks() as f64 * frames_per_tick).round()
                                                .max(1.0) as u64).min(audio.samples.len() as u64 - source_start);
                                            let source_frame = source_start.saturating_add(
                                                length_frames.saturating_mul(col as u64) / cols as u64);
                                            let index = ((source_frame as u128 * peaks.len() as u128)
                                                / audio.samples.len().max(1) as u128) as usize;
                                            let relative = length_frames.saturating_mul(col as u64) / cols as u64;
                                            let f_in = a.fade_in_ms as u64 * self.sample_rate as u64 / 1000;
                                            let f_out = a.fade_out_ms as u64 * self.sample_rate as u64 / 1000;
                                            let fade_in = if f_in==0 {1.0} else {(relative as f32 / f_in as f32).min(1.0)};
                                            let fade_out = if f_out==0 {1.0} else {(length_frames.saturating_sub(relative) as f32 / f_out as f32).min(1.0)};
                                            let amp = (peaks[index.min(peaks.len()-1)].min(1.0)
                                                * a.gain * fade_in.min(fade_out)).min(1.0);
                                            let sample_x = x + col as f32 * clip_rect.width() / cols as f32;
                                            painter.line_segment([
                                                Pos2::new(sample_x, middle_y - amp * usable_h),
                                                Pos2::new(sample_x, middle_y + amp * usable_h),
                                            ], Stroke::new(1.25_f32, ACCENT_BRIGHT));
                                        }
                                    }
                                }
                            }
                            ClipKind::Midi(midi) => {
                                for note in &midi.notes {
                                    if note.muted { continue; }
                                    let rel_x = note.start.ticks() as f32 / PPQ as f32 * self.zoom;
                                    let note_w = (note.duration.ticks() as f32 / PPQ as f32 * self.zoom).max(2.0);
                                    let note_y = (88.0 - note.pitch as f32).clamp(0.0, 38.0) * 0.55;
                                    let note_rect = Rect::from_min_size(
                                        Pos2::new(x + rel_x, row_y + 26.0 + note_y),
                                        Vec2::new(note_w.min(clip_rect.width()), 3.0));
                                    painter.rect_filled(note_rect, 1.0,
                                        ACCENT_BRIGHT);
                                }
                            }
                        }
                        let response = ui.interact(clip_rect,
                            egui::Id::new(("slate-clip", clip.id)), Sense::click_and_drag())
                            .on_hover_text(if recording {
                                "Stop recording before seeking. Shift-click selects; drag moves or trims."
                            } else {
                                "Click the body to select and seek. Shift-click selects; drag moves or trims."
                            });
                        let pointer = response.interact_pointer_pos();
                        let edge_width = (clip_rect.width() / 3.0).min(12.0);
                        if response.hover_pos().is_some_and(|pos| {
                            (pos.x - clip_rect.left()).abs().min(
                                (pos.x - clip_rect.right()).abs()) <= edge_width
                        }) {
                            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
                        }
                        if response.clicked() {
                            clip_click = Some((track.id, clip.id, ui.input(|input| input.modifiers.shift)));
                            // egui resolves clicked on release and rejects a drag.
                            // Trim edges remain selection-only, even without movement.
                            if plain_click && let Some(pos) = pointer
                                && pos.x > clip_rect.left() + edge_width
                                && pos.x < clip_rect.right() - edge_width
                                && ui.input(|input| input.pointer.press_origin()).is_none_or(|press|
                                    press.x > clip_rect.left() + edge_width
                                    && press.x < clip_rect.right() - edge_width)
                            {
                                seek_to = Some(timeline_seek_frame(pos.x, canvas.left(), self.zoom,
                                    tempo_map, self.sample_rate));
                            }
                        }
                        if response.drag_started_by(egui::PointerButton::Primary) {
                            // `interact_pointer_pos` is the current drag position.
                            // Reconstruct the press position, not an intermediate position.
                            if let Some(pos) = pointer {
                                let delta = response.total_drag_delta().map_or(0.0, |d| d.x);
                                let press_x = pos.x - delta;
                                self.clip_drag_origin_x = Some(press_x);
                                let dl = (press_x - clip_rect.left()).abs();
                                let dr = (press_x - clip_rect.right()).abs();
                                self.clip_resize_drag = (dl.min(dr) <= edge_width).then_some({
                                    if dl <= dr { ClipTrimEdge::Left }
                                    else { ClipTrimEdge::Right }
                                });
                            }
                            drag_selection = Some((track.id, clip.id,
                                self.clip_resize_drag.is_none() && is_selected));
                        }
                        if response.dragged_by(egui::PointerButton::Primary) {
                            let delta = response.total_drag_delta().map_or(0.0, |d| d.x);
                            if self.clip_resize_drag.is_some() {
                                self.clip_resize_preview = Some((clip.id, delta));
                            } else {
                                self.clip_drag_preview = Some((clip.id, delta));
                            }
                        }
                        if response.drag_stopped_by(egui::PointerButton::Primary) {
                            let released_delta = pointer.zip(self.clip_drag_origin_x.take())
                                .map(|(pos, origin)| pos.x - origin);
                            let resize_preview_for_clip = self.clip_resize_preview
                                .is_some_and(|(id, _)| id == clip.id);
                            if let Some(edge) = self.clip_resize_drag
                                .filter(|_| resize_preview_for_clip)
                            {
                                drop_trim = self.clip_resize_preview.take()
                                    .map(|(id, delta)| (id, edge, released_delta.unwrap_or(delta)));
                                self.clip_resize_drag = None;
                            } else {
                                drop_move = self.clip_drag_preview.take()
                                    .map(|(id, delta)| (id, released_delta.unwrap_or(delta)));
                            }
                        }
                        // The clip itself covers most of the lane. Give it
                        // the same track menu without swallowing left drags.
                        response.context_menu(|ui| clip_context_menu(
                            ui, ClipMenuTarget { track: id, clip: clip.id, kind,
                                parent, can_split: true }, &groups,
                            &mut track_action, &mut clip_action));
                    }
                    if let Some(take) = active_take {
                        let pixels_per_frame = self.zoom / (seconds_per_beat * f64::from(self.sample_rate)) as f32;
                        let x = lane.left() + take.start_frame as f32 * pixels_per_frame;
                        let preview = Rect::from_min_size(Pos2::new(x, row_y + 6.0),
                            Vec2::new((take.frames as f32 * pixels_per_frame).max(2.0), row_h - 12.0));
                        if painter.clip_rect().intersects(preview) {
                            painter.rect_filled(preview, 6.0, RECORDING_FILL);
                            painter.rect_stroke(preview, 6.0, Stroke::new(1.0_f32, RECORDING_RED),
                                egui::StrokeKind::Inside);
                            paint_recording_waveform(&painter, preview, take, pixels_per_frame);
                        }
                    }
                }

                if track_count == 0 {
                    painter.text(Pos2::new(canvas.left() + TRACK_HEADER_W + 50.0,
                        canvas.top() + TIMELINE_TOP + 42.0),
                        egui::Align2::LEFT_CENTER,
                        "No tracks yet. Add a MIDI instrument or import an audio file.",
                        FontId::proportional(15.0), Color32::from_gray(151));
                }
                let master_y = canvas.top() + TIMELINE_TOP
                    + track_height.max(TRACK_ROW_H);
                let master = Rect::from_min_size(Pos2::new(canvas.left(), master_y),
                    Vec2::new(TRACK_HEADER_W, TRACK_ROW_H));
                painter.rect_filled(master, 0.0, INK_3);
                painter.rect_filled(Rect::from_min_size(master.min,
                    Vec2::new(4.0, master.height())), 0.0, SUNSET);
                painter.text(master.min + Vec2::new(16.0, 22.0),
                    egui::Align2::LEFT_TOP, "MASTER",
                    FontId::proportional(14.0), OFFWHITE);
                painter.text(master.min + Vec2::new(16.0, 50.0),
                    egui::Align2::LEFT_TOP, "Stereo output  •  0 dB",
                    FontId::proportional(10.0), Color32::from_gray(169));

                let play_x = canvas.left() + TRACK_HEADER_W +
                    play_tick as f32 / PPQ as f32 * self.zoom;
                painter.line_segment([
                    Pos2::new(play_x, canvas.top()),
                    Pos2::new(play_x, canvas.bottom())],
                    Stroke::new(1.75_f32, SUNSET));

                canvas_response.context_menu(|ui| empty_track_context_menu(
                    ui, &mut track_action));
                ui.set_clip_rect(viewport_clip);
                let ruler_painter = ui.painter_at(ruler_band);
                ruler_painter.rect_filled(ruler_band, 0.0, INK_2);
                ruler_painter.rect_filled(Rect::from_min_size(ruler_band.min,
                    Vec2::new(TRACK_HEADER_W, TIMELINE_TOP)), 0.0, INK_3);
                ruler_painter.text(ruler_band.min + Vec2::new(14.0, 12.0),
                    egui::Align2::LEFT_TOP, "TRACK / MIXER",
                    FontId::proportional(10.0), STUDIO_DIM);
                for bar in first_bar..=last_bar {
                    let x = canvas.left() + TRACK_HEADER_W + bar as f32 * beats_per_bar * self.zoom;
                    ruler_painter.text(Pos2::new(x + 6.0, ruler_y + 8.0),
                        egui::Align2::LEFT_TOP, format!("{}", bar + 1),
                        FontId::monospace(12.0), OFFWHITE);
                }
                ruler_painter.circle_filled(Pos2::new(play_x, ruler_y + 6.0), 5.0, SUNSET);
                let ruler_rect = Rect::from_min_size(
                    Pos2::new(canvas.left() + TRACK_HEADER_W, ruler_y),
                    Vec2::new((canvas.width() - TRACK_HEADER_W).max(1.0), TIMELINE_TOP));
                let ruler_response = ui.interact(ruler_rect,
                    egui::Id::new("arrangement_seek_ruler"), Sense::click_and_drag())
                    .on_hover_text(if recording { "Stop recording before seeking" }
                        else { "Click the time ruler to seek playback" });
                if ruler_response.clicked() && plain_click
                    && let Some(pos) = ruler_response.interact_pointer_pos() {
                    seek_to = Some(timeline_seek_frame(
                        pos.x, canvas.left(), self.zoom, tempo_map, self.sample_rate,
                    ));
                }
            });

        if let Some(id) = selected_track { self.selected_track = Some(id); }
        if let Some((track, clip, toggle)) = clip_click {
            self.select_arrangement_clip(track, clip, toggle, false);
        } else if let Some((track, clip, keep_group)) = drag_selection {
            self.select_arrangement_clip(track, clip, false, keep_group);
        } else if clicked_empty_lane {
            self.clear_arrangement_selection();
        }
        if let Some(track) = open_recording_for {
            self.selected_track = Some(track);
            self.selected_clip = None;
            self.device_tab = DeviceTab::Hardware;
            self.show_devices = true;
        }
        if !changed_tracks.is_empty() { self.volume_changed(changed_tracks[0]); }
        if let Some((id, edge, delta)) = drop_trim {
            let raw_ticks = (delta / self.zoom * PPQ as f32).round() as i64;
            let tick_delta = snapped_tick_delta(raw_ticks, self.snap, PPQ / 4);
            self.apply_clip_trim(id, edge, tick_delta);
        }
        if let Some((id, delta)) = drop_move
            && self.arrangement_clip_selected(id)
        {
            self.move_arrangement_selection(self.arrangement_move_delta(delta));
        }
        if let Some(frame) = seek_to { self.seek_to_frame(frame); ui.ctx().request_repaint(); }
        if let Some(action) = track_action { self.apply_track_action(action); }
        if let Some(action) = clip_action { self.apply_clip_action(action); }
    }
}

#[cfg(test)]
mod track_lane_context_tests {
    use super::*;

    #[test]
    fn arrangement_shows_the_selected_tracks_input_failure_with_devices_closed() {
        let ctx = egui::Context::default();
        install_midnight_ember_theme(&ctx);
        let mut studio = Studio::with_sample_rate(44_100);
        studio.project.tracks.clear();
        studio.add_audio_track();
        studio.add_audio_track();
        studio.capture_devices = Some((Instant::now(), Err("PulseAudio unavailable".to_owned())));
        let output = ctx.run(egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(900.0, 480.0))),
            ..Default::default()
        }, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| studio.studio_arrangement(ui));
        });
        let labels = output.shapes.iter().filter_map(|shape| match &shape.shape {
            egui::epaint::Shape::Text(text) => Some(&text.galley),
            _ => None,
        }).collect::<Vec<_>>();
        assert!(!studio.show_devices);
        assert!(labels.iter().any(|label| label.text() == "Input unavailable"));
        assert!(labels.iter().any(|label| label.text() == "Select track to meter"));
        assert_eq!(labels.iter().filter(|label| label.text() == "IN" && label.rows.len() == 1).count(), 2);
    }

    #[test]
    fn arrangement_shift_click_and_drag_outside_moves_selection_without_seeking() {
        let ctx = egui::Context::default();
        let mut studio = Studio::with_sample_rate(44_100);
        let render = |studio: &mut Studio, events, modifiers| {
            let _ = ctx.run(egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1100.0, 480.0))),
                modifiers,
                events,
                ..Default::default()
            }, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    studio.studio_arrangement(ui);
                });
            });
        };
        render(&mut studio, vec![], egui::Modifiers::NONE);
        let position = |id| {
            ctx.read_response(egui::Id::new(("slate-clip", id)))
                .unwrap().rect.center()
        };
        let click = |pos, pressed, modifiers| egui::Event::PointerButton {
            pos, button: egui::PointerButton::Primary, pressed, modifiers,
        };
        let other = position(4_u64);
        let shift = egui::Modifiers::SHIFT;
        render(&mut studio, vec![egui::Event::PointerMoved(other)], shift);
        render(&mut studio, vec![click(other, true, shift)], shift);
        render(&mut studio, vec![click(other, false, shift)], shift);
        assert_eq!(studio.editor_selection().clips, vec![2, 4]);
        assert_eq!(studio.playhead_frame, 0);

        let first = position(2_u64);
        let plain = egui::Modifiers::NONE;
        render(&mut studio, vec![egui::Event::PointerMoved(first)], plain);
        render(&mut studio, vec![click(first, true, plain)], plain);
        let outside = Pos2::new(first.x + 2.0 * studio.zoom, 550.0);
        render(&mut studio, vec![egui::Event::PointerMoved(outside)], plain);
        render(&mut studio, vec![click(outside, false, plain)], plain);
        assert_eq!(studio.project.tracks[0].clips[0].start.ticks(), 2 * PPQ);
        assert_eq!(studio.project.tracks[1].clips[0].start.ticks(), 2 * PPQ);
        assert_eq!(studio.project.tracks[2].clips[0].start.ticks(), 0);
        assert_eq!(studio.playhead_frame, 0);

        let empty = Pos2::new(first.x + 8.0 * studio.zoom, first.y);
        render(&mut studio, vec![egui::Event::PointerMoved(empty)], plain);
        render(&mut studio, vec![click(empty, true, plain)], plain);
        render(&mut studio, vec![click(empty, false, plain)], plain);
        assert!(studio.selected_clip.is_none());
        assert!(studio.playhead_frame > 0);
    }

    fn arrangement_frame(ctx: &egui::Context, studio: &mut Studio, events: Vec<egui::Event>, modifiers: egui::Modifiers) {
        let _ = ctx.run(egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1100.0, 480.0))),
            events, modifiers, ..Default::default()
        }, |ctx| { egui::CentralPanel::default().show(ctx, |ui| studio.studio_arrangement(ui)); });
    }

    fn pointer_button(pos: Pos2, pressed: bool, modifiers: egui::Modifiers) -> egui::Event {
        egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers }
    }

    #[test]
    fn clip_body_seeks_on_release_but_edges_modifiers_and_drags_do_not() {
        let ctx = egui::Context::default();
        let mut studio = Studio::with_sample_rate(44_100);
        studio.project.tempo_map = daw_core::TempoMap::new(Tempo::from_bpm(90.0).unwrap());
        studio.project.tempo_map.insert(MusicalTime::from_ticks(PPQ * 2), Tempo::from_bpm(60.0).unwrap()).unwrap();
        studio.project.time_signature = TimeSignature::new(7, 8).unwrap();
        for zoom in [28.0, 100.0] {
            studio.zoom = zoom;
            for _ in 0..2 { arrangement_frame(&ctx, &mut studio, vec![], egui::Modifiers::NONE); }
            let rect = ctx.read_response(egui::Id::new(("slate-clip", 2_u64))).unwrap().rect;
            let pos = rect.center();
            let expected = studio.tick_to_frame(4 * PPQ);
            studio.playhead_frame = 0;
            arrangement_frame(&ctx, &mut studio, vec![egui::Event::PointerMoved(pos), pointer_button(pos, true, egui::Modifiers::NONE)], egui::Modifiers::NONE);
            assert_eq!(studio.playhead_frame, 0, "must wait for release");
            arrangement_frame(&ctx, &mut studio, vec![pointer_button(pos, false, egui::Modifiers::NONE)], egui::Modifiers::NONE);
            assert_eq!(studio.playhead_frame, expected);
            assert_eq!(studio.selected_clip, Some(2));
            // An edge click selects but does not seek.
            let edge = Pos2::new(rect.right() - 2.0, pos.y);
            for pressed in [true, false] {
                arrangement_frame(&ctx, &mut studio, vec![egui::Event::PointerMoved(edge), pointer_button(edge, pressed, egui::Modifiers::NONE)], egui::Modifiers::NONE);
            }
            assert_eq!(studio.playhead_frame, expected);
            // Even releasing Shift before the mouse must not turn selection into a seek.
            let target = Pos2::new(pos.x - zoom, pos.y);
            arrangement_frame(&ctx, &mut studio, vec![egui::Event::PointerMoved(target), pointer_button(target, true, egui::Modifiers::SHIFT)], egui::Modifiers::SHIFT);
            arrangement_frame(&ctx, &mut studio, vec![pointer_button(target, false, egui::Modifiers::NONE)], egui::Modifiers::NONE);
            assert_eq!(studio.playhead_frame, expected);
        }
    }

    #[test]
    fn ruler_stays_pinned_and_seeks_with_both_scroll_offsets() {
        let ctx = egui::Context::default();
        let mut studio = Studio::with_sample_rate(48_000);
        for id in 10..30 {
            studio.project.tracks.push(Track::new(id, "Empty", TrackKind::Audio));
        }
        arrangement_frame(&ctx, &mut studio, vec![], egui::Modifiers::NONE);
        let before = ctx.read_response(egui::Id::new("arrangement_seek_ruler")).unwrap().rect;
        let mouse = Pos2::new(850.0, 300.0);
        arrangement_frame(&ctx, &mut studio, vec![egui::Event::PointerMoved(mouse), egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point, delta: Vec2::new(-300.0, -240.0), modifiers: egui::Modifiers::NONE,
        }], egui::Modifiers::NONE);
        for _ in 0..30 { arrangement_frame(&ctx, &mut studio, vec![], egui::Modifiers::NONE); }
        let after = ctx.read_response(egui::Id::new("arrangement_seek_ruler")).unwrap().rect;
        assert!((before.top() - after.top()).abs() < 0.1);
        assert!(after.left() < before.left() - 100.0);
        let pos = Pos2::new(700.0, after.center().y);
        for pressed in [true, false] {
            arrangement_frame(&ctx, &mut studio, vec![egui::Event::PointerMoved(pos), pointer_button(pos, pressed, egui::Modifiers::NONE)], egui::Modifiers::NONE);
        }
        let expected = timeline_seek_frame(pos.x, after.left() - TRACK_HEADER_W,
            studio.zoom, &studio.project.tempo_map, studio.sample_rate);
        assert_eq!(studio.playhead_frame, expected);
        // A ruler drag never becomes a click-to-seek.
        let dest = pos + Vec2::new(80.0, 0.0);
        arrangement_frame(&ctx, &mut studio, vec![pointer_button(pos, true, egui::Modifiers::NONE)], egui::Modifiers::NONE);
        arrangement_frame(&ctx, &mut studio, vec![egui::Event::PointerMoved(dest)], egui::Modifiers::NONE);
        arrangement_frame(&ctx, &mut studio, vec![pointer_button(dest, false, egui::Modifiers::NONE)], egui::Modifiers::NONE);
        assert_eq!(studio.playhead_frame, expected);
    }

    #[test]
    fn timeline_x_maps_to_exact_playback_seconds() {
        let origin = 20.0;
        let start = origin + TRACK_HEADER_W;
        assert_eq!(timeline_seek_frame(start, origin, 56.0, &daw_core::TempoMap::default(), 44_100), 0);
        assert_eq!(timeline_seek_frame(start + 8.0 * 56.0, origin, 56.0, &daw_core::TempoMap::default(), 44_100), 176_400);
        assert_eq!(timeline_seek_frame(start - 50.0, origin, 56.0, &daw_core::TempoMap::default(), 44_100), 0);
    }

    #[test]
    fn clicking_empty_timeline_lane_can_seek_to_four_seconds() {
        let ctx = egui::Context::default();
        let click = Pos2::new(20.0 + TRACK_HEADER_W + 8.0 * 56.0, 185.0);
        let render = |events: Vec<egui::Event>| {
            let mut seek = None;
            let _ = ctx.run(egui::RawInput {
                screen_rect: Some(Rect::from_min_size(
                    Pos2::ZERO, Vec2::new(1100.0, 480.0))),
                events,
                ..Default::default()
            }, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let response = ui.interact(
                        Rect::from_min_size(Pos2::new(20.0 + TRACK_HEADER_W, 135.0),
                            Vec2::new(800.0, 100.0)),
                        egui::Id::new("empty_timeline_lane"), Sense::click());
                    if response.clicked()
                        && let Some(pos) = response.interact_pointer_pos()
                    {
                        seek = Some(timeline_seek_frame(pos.x, 20.0, 56.0, &daw_core::TempoMap::default(), 44_100));
                    }
                });
            });
            seek
        };
        assert_eq!(render(vec![]), None);
        assert_eq!(render(vec![egui::Event::PointerMoved(click)]), None);
        assert_eq!(render(vec![egui::Event::PointerButton {
            pos: click, button: egui::PointerButton::Primary,
            pressed: true, modifiers: Default::default(),
        }]), None);
        assert_eq!(render(vec![egui::Event::PointerButton {
            pos: click, button: egui::PointerButton::Primary,
            pressed: false, modifiers: Default::default(),
        }]), Some(176_400));
    }

    #[test]
    fn right_click_on_a_track_lane_opens_that_tracks_menu() {
        let ctx = egui::Context::default();
        let right_click_at = Pos2::new(550.0, 185.0);
        let render = |events: Vec<egui::Event>| {
            let mut menu_open = false;
            let _ = ctx.run(egui::RawInput {
                screen_rect: Some(Rect::from_min_size(
                    Pos2::ZERO, Vec2::new(900.0, 480.0))),
                events,
                ..Default::default()
            }, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let response = ui.interact(
                        Rect::from_min_size(Pos2::new(180.0, 135.0),
                            Vec2::new(690.0, 100.0)),
                        egui::Id::new("test_track_content_lane"), Sense::click());
                    let mut command = None;
                    response.context_menu(|ui| {
                        track_context_menu(ui, 7, TrackKind::Midi, None, &[], &mut command);
                    });
                    menu_open = response.context_menu_opened();
                });
            });
            menu_open
        };
        assert!(!render(vec![]));
        assert!(!render(vec![egui::Event::PointerMoved(right_click_at)]));
        let _ = render(vec![egui::Event::PointerButton {
            pos: right_click_at,
            button: egui::PointerButton::Secondary,
            pressed: true,
            modifiers: Default::default(),
        }]);
        assert!(render(vec![egui::Event::PointerButton {
            pos: right_click_at,
            button: egui::PointerButton::Secondary,
            pressed: false,
            modifiers: Default::default(),
        }]), "Right-click inside the arrangement lane failed to open its track menu");
    }
}
