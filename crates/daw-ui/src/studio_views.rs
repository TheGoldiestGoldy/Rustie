// Slate Studio's custom editor surface. Functional architecture follows DAW
// conventions but uses original visual styling and no proprietary assets.

const TRACK_HEADER_W: f32 = 226.0;
const TIMELINE_TOP: f32 = 38.0;
const TRACK_ROW_H: f32 = 92.0;
const BG_CANVAS: Color32 = Color32::from_rgb(19, 24, 32);
const BG_HEADER: Color32 = Color32::from_rgb(28, 34, 43);
const GRID_FAINT: Color32 = Color32::from_rgb(36, 43, 53);
const GRID_BAR: Color32 = Color32::from_rgb(72, 83, 94);
const TEAL: Color32 = Color32::from_rgb(113, 197, 195);
const AUDIO_BLUE: Color32 = Color32::from_rgb(62, 115, 150);
const MIDI_GREEN: Color32 = Color32::from_rgb(58, 115, 102);
const OFFWHITE: Color32 = Color32::from_rgb(212, 222, 226);
const SUNSET: Color32 = Color32::from_rgb(244, 184, 100);

impl Studio {
    fn studio_transport(&mut self, ctx: &egui::Context) {
        let mut action: Option<&'static str> = None;
        egui::TopBottomPanel::top("slate_main_transport")
            .frame(egui::Frame::default().fill(Color32::from_rgb(24, 30, 39))
                .inner_margin(egui::Margin::symmetric(12, 8)))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("S L A T E").size(16.0)
                        .color(TEAL).strong());
                    ui.label(egui::RichText::new("STUDIO").size(11.0)
                        .color(Color32::from_rgb(139, 157, 174)));
                    ui.separator();
                    ui.menu_button("File", |ui| {
                        if ui.button("Open project…    Ctrl+O").clicked() {
                            action = Some("open"); ui.close();
                        }
                        if ui.button("Save project       Ctrl+S").clicked() {
                            action = Some("save"); ui.close();
                        }
                        if ui.button("Save as…").clicked() {
                            action = Some("save_as"); ui.close();
                        }
                        ui.separator();
                        if ui.button("Import audio…").clicked() {
                            action = Some("audio"); ui.close();
                        }
                        if ui.button("New MIDI track").clicked() {
                            action = Some("midi"); ui.close();
                        }
                    });
                    ui.menu_button("View", |ui| {
                        if ui.button("Arrangement  Tab").clicked() {
                            self.view = View::Arrangement; ui.close();
                        }
                        if ui.button("Session").clicked() {
                            self.view = View::Session; ui.close();
                        }
                        ui.separator();
                        if ui.button("Clip editor").clicked() {
                            self.detail = Detail::Clip; ui.close();
                        }
                        if ui.button("Device chain").clicked() {
                            self.detail = Detail::Devices; ui.close();
                        }
                    });
                    ui.separator();
                    if ui.selectable_label(self.view == View::Arrangement, "▤  Arrangement").clicked() {
                        self.view = View::Arrangement;
                    }
                    if ui.selectable_label(self.view == View::Session, "▦  Session").clicked() {
                        self.view = View::Session;
                    }
                    ui.separator();
                    if ui.button("＋ Audio").on_hover_text("Import an audio file").clicked() {
                        action = Some("audio");
                    }
                    if ui.button("＋ MIDI").on_hover_text("New MIDI track").clicked() {
                        action = Some("midi");
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if self.dirty {
                            ui.label(egui::RichText::new("● unsaved").color(SUNSET).size(11.0));
                        }
                        if ui.button("Save").clicked() { action = Some("save"); }
                        ui.add(egui::TextEdit::singleline(&mut self.project.name)
                            .desired_width(185.0).hint_text("Project name"));
                    });
                });
                ui.add_space(9.0);
                ui.separator();
                ui.add_space(5.0);
                ui.horizontal(|ui| {
                    let is_playing = self.playing;
                    let play = egui::Button::new(
                        egui::RichText::new(if is_playing { "Ⅱ  Pause" } else { "▶  Play" })
                            .size(15.0).strong(),
                    ).fill(if is_playing { Color32::from_rgb(57, 136, 119) }
                        else { Color32::from_rgb(53, 89, 107) })
                        .min_size(Vec2::new(96.0, 34.0));
                    if ui.add(play).clicked() {
                        action = Some(if is_playing { "pause" } else { "play" });
                    }
                    if ui.add_sized([65.0, 34.0], egui::Button::new("■ Stop")).clicked() {
                        action = Some("stop");
                    }
                    ui.separator();
                    let seconds = self.playhead_frame as f64 / self.sample_rate as f64;
                    let bpm = self.project.tempo_map.tempo_at(MusicalTime::ZERO)
                        .map_or(120.0, |t| t.bpm());
                    let beats = seconds * bpm / 60.0;
                    let bars = (beats / 4.0).floor() as u64 + 1;
                    let beat = (beats % 4.0).floor() as u64 + 1;
                    ui.vertical(|ui| {
                        ui.label(egui::RichText::new(format!("{bars:03} . {beat}"))
                            .size(19.0).monospace().strong().color(OFFWHITE));
                        ui.label(egui::RichText::new("BARS / BEATS").size(9.0).weak());
                    });
                    ui.separator();
                    let mut edited_bpm = bpm;
                    ui.vertical(|ui| {
                        if ui.add(egui::DragValue::new(&mut edited_bpm).range(20.0..=300.0)
                            .speed(0.25).fixed_decimals(1).suffix(" BPM")).changed() {
                            if let Ok(tempo) = Tempo::from_bpm(edited_bpm) {
                                self.project.tempo_map = daw_core::TempoMap::new(tempo);
                                self.dirty = true;
                                action = Some("refresh");
                            }
                        }
                        ui.label(egui::RichText::new("4/4  •  GLOBAL TEMPO").weak().size(9.0));
                    });
                    ui.separator();
                    ui.vertical(|ui| {
                        ui.label(egui::RichText::new(self.time_display())
                            .size(19.0).monospace().color(OFFWHITE));
                        ui.label(egui::RichText::new("MIN : SEC").weak().size(9.0));
                    });
                    ui.separator();
                    let before_backend = self.backend;
                    egui::ComboBox::from_id_salt("slate_output_backend")
                        .selected_text(match self.backend {
                            Backend::Alsa => "ALSA · Low latency",
                            Backend::PulseAudio => "PulseAudio · Buffered",
                            Backend::Jack => "PipeWire / JACK",
                        })
                        .width(160.0)
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.backend, Backend::Alsa, "ALSA · Low latency");
                            ui.selectable_value(&mut self.backend, Backend::PulseAudio,
                                "PulseAudio · Buffered");
                            ui.selectable_value(&mut self.backend, Backend::Jack, "PipeWire / JACK");
                        });
                    if self.backend != before_backend { action = Some("refresh"); }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let peak = self.driver.as_ref().map_or(0.0, |d| f32::from_bits(
                            d.telemetry().current_peak_bits.load(Ordering::Relaxed)
                        )).clamp(0.0, 1.0);
                        ui.add(egui::ProgressBar::new(peak)
                            .desired_width(100.0).show_percentage());
                        ui.small("MASTER");
                    });
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
            Some("audio") => {
                if let Some(path) = Self::pick_path(false, true) { self.add_audio(path); }
            }
            Some("midi") => self.add_midi_track(),
            _ => {}
        }
    }

    fn studio_browser(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("studio_browser")
            .frame(egui::Frame::default().fill(Color32::from_rgb(24, 30, 39))
                .inner_margin(egui::Margin::symmetric(10, 8)))
            .min_width(180.0).max_width(325.0).default_width(208.0).resizable(true)
            .show(ctx, |ui| {
                ui.heading(egui::RichText::new("LIBRARY").size(12.0).color(TEAL));
                ui.add_space(5.0);
                ui.add(egui::TextEdit::singleline(&mut self.search)
                    .hint_text("Search clips…").id(egui::Id::new("source_search")));
                ui.add_space(8.0);
                ui.label(egui::RichText::new("PLACES").size(10.0).color(Color32::GRAY));
                ui.horizontal(|ui| {
                    if ui.button("＋ Import file").clicked() {
                        if let Some(path) = Self::pick_path(false, true) { self.add_audio(path); }
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("📁"); ui.label("Sample Library");
                });
                ui.separator();
                ui.label(egui::RichText::new("INSTRUMENTS").size(10.0).color(Color32::GRAY));
                if ui.selectable_label(false, "♫  Basic Sine").clicked() {
                    self.add_midi_track();
                }
                ui.add_enabled(false, egui::Button::new("◇  CLAP / VST3 (coming soon)"));
                ui.add_space(8.0);
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("AUDIO FILES").size(10.0).color(Color32::GRAY));
                    ui.weak(self.media.len().to_string());
                });
                let filter = self.search.to_lowercase();
                let mut media_files = self.media.keys().filter(|p| {
                    p.display().to_string().to_lowercase().contains(&filter)
                }).cloned().collect::<Vec<_>>();
                media_files.sort();
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for path in media_files {
                        let label = path.file_name().unwrap_or_default().to_string_lossy();
                        if ui.selectable_label(false, format!("▤  {label}"))
                            .on_hover_text(path.display().to_string()).clicked() {
                            self.add_audio(path);
                        }
                    }
                });
                ui.separator();
                ui.add_space(4.0);
                ui.label(egui::RichText::new("Import or drop an audio file to add a new track.\n\nPlugin hosting and recording inputs are future milestones.")
                    .size(10.0).color(Color32::from_gray(116)));
                ui.add_space(5.0);
                ui.add(egui::TextEdit::singleline(&mut self.path_field)
                    .hint_text("Absolute audio path…"));
                if ui.small_button("Import path").clicked() && !self.path_field.trim().is_empty() {
                    self.add_audio(PathBuf::from(self.path_field.trim()));
                }
            });
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
            ui.toggle_value(&mut self.snap, "▦ Snap 1/16");
            ui.separator();
            ui.weak("−");
            ui.add(egui::Slider::new(&mut self.zoom, 28.0..=140.0)
                .show_value(false).text("Zoom").trailing_fill(true));
            ui.weak("+");
            ui.separator();
            ui.label(egui::RichText::new(
                "Drag clips to move · click ruler to seek"
            ).size(10.0).weak());
        });
        ui.separator();

        let tempo = self.project.tempo_map.tempo_at(MusicalTime::ZERO)
            .map_or(120.0, |t| t.bpm());
        let seconds_per_beat = 60.0 / tempo;
        let end_seconds = self.max_project_seconds().max(32.0);
        let beats = ((end_seconds / seconds_per_beat).ceil() as usize + 24)
            .clamp(64, 3000);
        let width = TRACK_HEADER_W + beats as f32 * self.zoom + 12.0;
        let track_count = self.project.tracks.len();
        let height = TIMELINE_TOP + track_count.max(1) as f32 * TRACK_ROW_H + TRACK_ROW_H;
        let mut selected_clip = None;
        let mut selected_track = None;
        let mut changed_tracks = Vec::new();
        let mut drop_move: Option<(u64, f32)> = None;
        let mut remove_track = None;
        let mut seek_to = None;

        egui::ScrollArea::both()
            .id_salt("slate_arrangement_surface")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let (canvas, canvas_response) = ui.allocate_exact_size(
                    Vec2::new(width, height), Sense::click()
                );
                let painter = ui.painter_at(canvas);
                painter.rect_filled(canvas, 0.0, BG_CANVAS);

                // Fixed-width track-control column, aligned exactly to timeline rows.
                let header = Rect::from_min_max(canvas.min,
                    Pos2::new(canvas.left() + TRACK_HEADER_W, canvas.bottom()));
                painter.rect_filled(header, 0.0, BG_HEADER);
                painter.rect_filled(Rect::from_min_size(canvas.min,
                    Vec2::new(canvas.width(), TIMELINE_TOP)),
                    0.0, Color32::from_rgb(28, 35, 43));
                painter.rect_filled(Rect::from_min_size(canvas.min,
                    Vec2::new(TRACK_HEADER_W, TIMELINE_TOP)),
                    0.0, Color32::from_rgb(38, 46, 55));
                painter.text(canvas.min + Vec2::new(14.0, 12.0),
                    egui::Align2::LEFT_TOP, "TRACK / MIXER",
                    FontId::proportional(10.0), Color32::from_rgb(143, 162, 179));

                let first_beat = ((ui.clip_rect().left() - canvas.left() - TRACK_HEADER_W)
                    / self.zoom).floor().max(0.0) as usize;
                let last_beat = ((ui.clip_rect().right() - canvas.left() - TRACK_HEADER_W)
                    / self.zoom).ceil().max(0.0) as usize + 1;
                for beat in first_beat..=last_beat.min(beats) {
                    let x = canvas.left() + TRACK_HEADER_W + beat as f32 * self.zoom;
                    let downbeat = beat % 4 == 0;
                    painter.line_segment(
                        [Pos2::new(x, canvas.top() + TIMELINE_TOP),
                         Pos2::new(x, canvas.bottom())],
                        Stroke::new(if downbeat { 1.1_f32 } else { 0.7_f32 },
                            if downbeat { GRID_BAR } else { GRID_FAINT }),
                    );
                    if downbeat {
                        painter.text(Pos2::new(x + 7.0, canvas.top() + 8.0),
                            egui::Align2::LEFT_TOP,
                            format!("{}", beat / 4 + 1),
                            FontId::monospace(12.0), OFFWHITE);
                    }
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

                for (track_idx, track) in self.project.tracks.iter_mut().enumerate() {
                    let row_y = canvas.top() + TIMELINE_TOP + track_idx as f32 * TRACK_ROW_H;
                    let row_rect = Rect::from_min_size(
                        Pos2::new(canvas.left(), row_y),
                        Vec2::new(canvas.width(), TRACK_ROW_H),
                    );
                    painter.rect_filled(row_rect, 0.0,
                        if track_idx % 2 == 0 {
                            Color32::from_rgb(20, 25, 33)
                        } else { Color32::from_rgb(24, 29, 37) });
                    for beat in first_beat..=last_beat.min(beats) {
                        let x = canvas.left() + TRACK_HEADER_W + beat as f32 * self.zoom;
                        painter.line_segment([
                            Pos2::new(x, row_y),
                            Pos2::new(x, row_y + TRACK_ROW_H),
                        ], Stroke::new(if beat % 4 == 0 { 1.1_f32 } else { 0.65_f32 },
                            if beat % 4 == 0 { GRID_BAR } else { GRID_FAINT }));
                    }
                    let head_rect = Rect::from_min_size(row_rect.min,
                        Vec2::new(TRACK_HEADER_W, TRACK_ROW_H));
                    painter.rect_filled(head_rect, 0.0,
                        if self.selected_track == Some(track.id) {
                            Color32::from_rgb(44, 56, 65)
                        } else {
                            Color32::from_rgb(33, 41, 50)
                        });
                    painter.line_segment(
                        [Pos2::new(row_rect.left(), row_rect.bottom()),
                         Pos2::new(row_rect.right(), row_rect.bottom())],
                        Stroke::new(0.6_f32, Color32::from_rgb(54, 62, 72)),
                    );
                    let color = if track.kind == TrackKind::Midi { MIDI_GREEN } else { AUDIO_BLUE };
                    painter.rect_filled(
                        Rect::from_min_size(row_rect.min, Vec2::new(4.0, TRACK_ROW_H)),
                        0.0, color);
                    let track_tag = if track.kind == TrackKind::Midi { "♫" } else { "≈" };
                    let title_rect = Rect::from_min_size(
                        row_rect.min + Vec2::new(11.0, 7.0),
                        Vec2::new(151.0, 24.0)
                    );
                    let title_response = ui.put(title_rect, egui::Button::new(
                        egui::RichText::new(format!("{track_tag}  {}", track.name))
                            .color(OFFWHITE).size(12.0)
                    ).frame(false));
                    if title_response.clicked() {
                        selected_track = Some(track.id);
                    }
                    title_response.context_menu(|ui| {
                        if ui.button("Remove track").clicked() {
                            remove_track = Some(track.id);
                            ui.close();
                        }
                    });
                    let mut btn = |label: &str, x: f32, on: bool, active: Color32| {
                        ui.put(Rect::from_min_size(
                            Pos2::new(row_rect.left() + x, row_y + 7.0),
                            Vec2::new(22.0, 22.0)),
                            egui::Button::new(egui::RichText::new(label).size(10.0)
                                .color(if on { Color32::BLACK } else { OFFWHITE }))
                                .fill(if on { active } else { Color32::from_rgb(44, 51, 59) })
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
                    painter.text(Pos2::new(row_rect.left() + 12.0, row_y + 73.0),
                        egui::Align2::LEFT_CENTER, "PAN",
                        FontId::proportional(9.0), Color32::GRAY);
                    if ui.put(Rect::from_min_size(
                        Pos2::new(row_rect.left() + 41.0, row_y + 62.0),
                        Vec2::new(120.0, 22.0)),
                        egui::Slider::new(&mut track.pan, -1.0..=1.0)
                            .show_value(false)).changed() {
                        changed_tracks.push(track.id);
                    }
                    painter.text(Pos2::new(row_rect.left() + 169.0, row_y + 73.0),
                        egui::Align2::LEFT_CENTER,
                        if track.pan.abs() < 0.02 { "C".into() } else if track.pan < 0.0 {
                            format!("L{:.0}", track.pan.abs() * 100.0)
                        } else { format!("R{:.0}", track.pan * 100.0) },
                        FontId::monospace(10.0), OFFWHITE);

                    for clip in &track.clips {
                        let start = clip.start.ticks() as f32 / PPQ as f32;
                        let duration = clip.duration.ticks() as f32 / PPQ as f32;
                        let start_x = canvas.left() + TRACK_HEADER_W + start * self.zoom;
                        let full_width = (duration * self.zoom).max(14.0);
                        let preview_delta = self.clip_drag_preview
                            .filter(|(id, _)| *id == clip.id)
                            .map_or(0.0, |(_, delta)| delta);
                        let x = (start_x + preview_delta)
                            .max(canvas.left() + TRACK_HEADER_W);
                        let clip_rect = Rect::from_min_size(
                            Pos2::new(x, row_y + 9.0),
                            Vec2::new(full_width, TRACK_ROW_H - 18.0));
                        let is_selected = self.selected_clip == Some(clip.id);
                        let base = if track.kind == TrackKind::Midi { MIDI_GREEN } else { AUDIO_BLUE };
                        painter.rect_filled(clip_rect, 4.0,
                            if track.muted { Color32::from_rgb(42, 49, 58) } else { base });
                        painter.rect_filled(Rect::from_min_size(clip_rect.min,
                            Vec2::new(clip_rect.width(), 4.0)), 2.0,
                            if track.kind == TrackKind::Midi { TEAL } else {
                                Color32::from_rgb(152, 203, 227)
                            });
                        if is_selected {
                            painter.rect_stroke(clip_rect, 4.0,
                                Stroke::new(2.0_f32, Color32::from_rgb(224, 240, 241)),
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
                                            let amp = peaks[col * peaks.len() / cols].min(1.0);
                                            let sample_x = x + col as f32 * clip_rect.width() / cols as f32;
                                            painter.line_segment([
                                                Pos2::new(sample_x, middle_y - amp * usable_h),
                                                Pos2::new(sample_x, middle_y + amp * usable_h),
                                            ], Stroke::new(1.25_f32, Color32::from_rgb(187, 222, 240)));
                                        }
                                    }
                                }
                            }
                            ClipKind::Midi(midi) => {
                                for note in &midi.notes {
                                    if note.muted { continue; }
                                    let rel_x = note.start.ticks() as f32 / PPQ as f32 * self.zoom;
                                    let note_w = (note.duration.ticks() as f32 / PPQ as f32 * self.zoom).max(2.0);
                                    let note_y = (100.0 - note.pitch as f32).clamp(0.0, 52.0) * 0.6;
                                    let note_rect = Rect::from_min_size(
                                        Pos2::new(x + rel_x, row_y + 31.0 + note_y),
                                        Vec2::new(note_w.min(clip_rect.width()), 3.0));
                                    painter.rect_filled(note_rect, 1.0,
                                        Color32::from_rgb(183, 238, 202));
                                }
                            }
                        }
                        let response = ui.interact(clip_rect,
                            egui::Id::new(("slate-clip", clip.id)), Sense::click_and_drag());
                        if response.clicked() {
                            selected_clip = Some((track.id, clip.id));
                        }
                        if response.dragged() {
                            self.clip_drag_preview = Some((
                                clip.id,
                                response.total_drag_delta().map_or(0.0, |delta| delta.x)
                            ));
                            selected_clip = Some((track.id, clip.id));
                        }
                        if response.drag_stopped() {
                            drop_move = self.clip_drag_preview.take();
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
                    + track_count.max(1) as f32 * TRACK_ROW_H;
                let master = Rect::from_min_size(Pos2::new(canvas.left(), master_y),
                    Vec2::new(TRACK_HEADER_W, TRACK_ROW_H));
                painter.rect_filled(master, 0.0, Color32::from_rgb(42, 48, 56));
                painter.rect_filled(Rect::from_min_size(master.min,
                    Vec2::new(4.0, master.height())), 0.0, SUNSET);
                painter.text(master.min + Vec2::new(16.0, 22.0),
                    egui::Align2::LEFT_TOP, "MASTER",
                    FontId::proportional(14.0), OFFWHITE);
                painter.text(master.min + Vec2::new(16.0, 50.0),
                    egui::Align2::LEFT_TOP, "Stereo output  •  0 dB",
                    FontId::proportional(10.0), Color32::from_gray(169));

                let play_seconds = self.playhead_frame as f64 / self.sample_rate as f64;
                let play_x = canvas.left() + TRACK_HEADER_W +
                    (play_seconds / seconds_per_beat) as f32 * self.zoom;
                painter.line_segment([
                    Pos2::new(play_x, canvas.top()),
                    Pos2::new(play_x, canvas.bottom())],
                    Stroke::new(1.75_f32, SUNSET));
                painter.circle_filled(Pos2::new(play_x, canvas.top() + 6.0), 5.0, SUNSET);

                // The ruler is the seek surface; clip and fader clicks are never seeks.
                if canvas_response.clicked()
                    && let Some(pos) = canvas_response.interact_pointer_pos()
                    && pos.y < canvas.top() + TIMELINE_TOP
                    && pos.x >= canvas.left() + TRACK_HEADER_W {
                    let beats = ((pos.x - canvas.left() - TRACK_HEADER_W) / self.zoom).max(0.0);
                    let seconds = beats as f64 * seconds_per_beat;
                    seek_to = Some((seconds * self.sample_rate as f64).round() as u64);
                }
            });

        if let Some(id) = selected_track { self.selected_track = Some(id); }
        if let Some((track, clip)) = selected_clip {
            self.selected_track = Some(track);
            self.selected_clip = Some(clip);
        }
        if !changed_tracks.is_empty() { self.volume_changed(changed_tracks[0]); }
        if let Some((id, delta)) = drop_move {
            let beat_delta = delta / self.zoom;
            let tick_delta = if self.snap {
                (beat_delta * 4.0).round() as i64 * (PPQ / 4)
            } else { (beat_delta * PPQ as f32).round() as i64 };
            for track in &mut self.project.tracks {
                if let Some(clip) = track.clips.iter_mut().find(|clip| clip.id == id) {
                    clip.start = MusicalTime::from_ticks(
                        clip.start.ticks().saturating_add(tick_delta).max(0)
                    );
                    self.dirty = true;
                    break;
                }
            }
            self.restart_for_edit();
        }
        if let Some(frame) = seek_to {
            self.playhead_frame = frame;
            self.restart_for_edit();
        }
        if let Some(id) = remove_track {
            self.project.tracks.retain(|t| t.id != id);
            self.dirty = true;
            self.restart_for_edit();
        }
    }
}


impl Studio {
    fn studio_session(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("SESSION").strong().color(OFFWHITE).size(12.0));
            ui.separator();
            ui.label(egui::RichText::new(
                "Clip launch grid preview • independent scene launching is not yet implemented"
            ).weak().size(10.0));
        });
        ui.separator();
        let mut selected = None;
        let track_width = 172.0;
        let scene_label_width = 82.0;
        let header_h = 54.0;
        let cell_h = 62.0;
        let scene_count = self.project.tracks.iter().map(|t| t.clips.len())
            .max().unwrap_or(0).clamp(8, 48);
        let width = scene_label_width + self.project.tracks.len() as f32 * track_width;
        let height = header_h + scene_count as f32 * cell_h;
        egui::ScrollArea::both().id_salt("slate_session_surface")
            .show(ui, |ui| {
                let (rect, _) = ui.allocate_exact_size(
                    Vec2::new(width.max(400.0), height), Sense::hover()
                );
                let painter = ui.painter_at(rect);
                painter.rect_filled(rect, 0.0, BG_CANVAS);
                painter.rect_filled(Rect::from_min_size(rect.min,
                    Vec2::new(rect.width(), header_h)), 0.0, BG_HEADER);
                painter.text(rect.min + Vec2::new(10.0, 20.0),
                    egui::Align2::LEFT_TOP, "SCENES",
                    FontId::proportional(10.0), Color32::GRAY);
                for (index, track) in self.project.tracks.iter().enumerate() {
                    let x = rect.left() + scene_label_width + index as f32 * track_width;
                    let title_rect = Rect::from_min_size(Pos2::new(x + 2.0, rect.top() + 4.0),
                        Vec2::new(track_width - 4.0, header_h - 8.0));
                    let selected_track = self.selected_track == Some(track.id);
                    painter.rect_filled(title_rect, 3.0,
                        if selected_track { Color32::from_rgb(52, 70, 79) }
                        else { Color32::from_rgb(37, 47, 58) });
                    painter.rect_filled(Rect::from_min_size(title_rect.min,
                        Vec2::new(4.0, title_rect.height())), 0.0,
                        if track.kind == TrackKind::Midi { TEAL } else {
                            Color32::from_rgb(119, 182, 212)
                        });
                    painter.text(title_rect.min + Vec2::new(10.0, 9.0),
                        egui::Align2::LEFT_TOP, &track.name,
                        FontId::proportional(12.0), OFFWHITE);
                    painter.text(title_rect.min + Vec2::new(10.0, 29.0),
                        egui::Align2::LEFT_TOP,
                        if track.kind == TrackKind::Midi { "MIDI" } else { "AUDIO" },
                        FontId::proportional(9.0), Color32::GRAY);
                }
                for scene in 0..scene_count {
                    let y = rect.top() + header_h + scene as f32 * cell_h;
                    let scene_rect = Rect::from_min_size(Pos2::new(rect.left(), y),
                        Vec2::new(scene_label_width, cell_h));
                    painter.rect_filled(scene_rect, 0.0,
                        if scene % 2 == 0 { Color32::from_rgb(30, 37, 46) }
                        else { Color32::from_rgb(26, 33, 42) });
                    painter.text(scene_rect.min + Vec2::new(17.0, 18.0),
                        egui::Align2::LEFT_TOP, format!("Scene {:02}", scene + 1),
                        FontId::proportional(11.0), Color32::from_gray(180));
                    for (track_idx, track) in self.project.tracks.iter().enumerate() {
                        let x = rect.left() + scene_label_width
                            + track_idx as f32 * track_width + 3.0;
                        let slot = Rect::from_min_size(Pos2::new(x, y + 4.0),
                            Vec2::new(track_width - 6.0, cell_h - 8.0));
                        if let Some(clip) = track.clips.get(scene) {
                            painter.rect_filled(slot, 3.0,
                                if track.kind == TrackKind::Midi { MIDI_GREEN } else { AUDIO_BLUE });
                            if self.selected_clip == Some(clip.id) {
                                painter.rect_stroke(slot, 3.0,
                                    Stroke::new(2.0_f32, OFFWHITE), egui::StrokeKind::Inside);
                            }
                            painter.text(slot.min + Vec2::new(11.0, 10.0),
                                egui::Align2::LEFT_TOP,
                                if track.kind == TrackKind::Midi {
                                    format!("♫  {}", track.name)
                                } else {
                                    match &clip.kind {
                                        ClipKind::Audio(audio) => audio.source.file_name()
                                            .unwrap_or_default().to_string_lossy().to_string(),
                                        _ => "Audio".into()
                                    }
                                },
                                FontId::proportional(12.0), Color32::WHITE);
                            painter.text(slot.min + Vec2::new(11.0, 32.0),
                                egui::Align2::LEFT_TOP, "Select clip",
                                FontId::proportional(9.0), Color32::from_gray(205));
                            if ui.interact(slot, egui::Id::new(("session_clip", clip.id)),
                                Sense::click()).clicked() {
                                selected = Some((track.id, clip.id));
                            }
                        } else {
                            painter.rect_stroke(slot, 2.0,
                                Stroke::new(0.75_f32, GRID_FAINT),
                                egui::StrokeKind::Inside);
                        }
                    }
                }
            });
        if let Some((track, clip)) = selected {
            self.selected_track = Some(track);
            self.selected_clip = Some(clip);
        }
    }
}
