use daw_core::session::{LaunchGrid, MAX_SCENES};
use daw_engine::session::{SessionLaunch, TrackLaunch};

impl Studio {
    fn change_view(&mut self, next: View) {
        if self.view == next {
            return;
        }
        if self.recorder.is_some() {
            self.error = Some("Finish the WAV take before switching views.".into());
            return;
        }
        self.view = next;
        self.detail = if next == View::Session {
            Detail::Mixer
        } else {
            Detail::Clip
        };
        self.selected_clip = self
            .selected_track
            .and_then(|id| self.project.tracks.iter().find(|t| t.id == id))
            .and_then(|t| t.playback_clips(next == View::Session).next())
            .map(|c| c.id);
        self.piano_selected = None;
    }

    fn first_empty_scene(&self, track: u64) -> Option<u64> {
        let track = self.project.tracks.iter().find(|t| t.id == track)?;
        self.project
            .session
            .as_ref()?
            .scenes
            .iter()
            .copied()
            .find(|s| !track.session_slots.contains_key(s))
    }

    fn insert_session_clip(&mut self, track_id: u64, scene: u64, mut clip: Clip) {
        if self
            .project
            .tracks
            .iter()
            .map(|t| t.session_slots.len())
            .sum::<usize>()
            >= MAX_CLIPS
        {
            self.error = Some("Session clip limit reached (48)".into());
            return;
        }
        let Some(track) = self.project.track_mut(track_id) else {
            return;
        };
        if track.session_slots.contains_key(&scene) {
            return;
        }
        clip.start = MusicalTime::ZERO;
        clip.id = self.next_id;
        self.selected_track = Some(track_id);
        self.selected_clip = Some(clip.id);
        self.next_id += 1;
        track.session_slots.insert(scene, clip);
        self.detail = Detail::Clip;
        self.piano_selected = None;
        self.dirty = true;
        self.restart_for_edit();
    }

    fn session_append_midi(&mut self, track: u64, scene: u64) {
        self.insert_session_clip(
            track,
            scene,
            Clip {
                id: 0,
                start: MusicalTime::ZERO,
                duration: MusicalTime::from_ticks(4 * self.bar_ticks()),
                kind: ClipKind::Midi(MidiClip::default()),
                scale_override: self.project.global_scale,
            },
        );
    }

    fn session_append_audio(&mut self, track: u64, path: PathBuf) {
        let scene = if self.view == View::Session {
            let Some(scene) = self.first_empty_scene(track) else {
                self.error = Some("Add an empty scene to import this clip".into());
                return;
            };
            Some(scene)
        } else {
            None
        };
        self.import_audio_cell(track, scene, path);
    }

    fn import_audio_cell(&mut self, track_id: u64, scene: Option<u64>, path: PathBuf) {
        if self.visible_clip_count() >= MAX_CLIPS {
            self.error = Some("Clip limit reached (48)".into());
            return;
        }
        let decoded = self.decode_cached(&path);
        if self.apply_error(decoded).is_none() {
            return;
        }
        let seconds = self.media[&path].samples.len() as f64 / self.sample_rate as f64;
        let bpm = self
            .project
            .tempo_map
            .tempo_at(MusicalTime::ZERO)
            .map_or(120.0, |t| t.bpm());
        let mut clip = Clip {
            id: self.next_id,
            start: MusicalTime::ZERO,
            duration: MusicalTime::from_ticks(
                (seconds * bpm / 60.0 * PPQ as f64).round().max(1.0) as i64
            ),
            kind: ClipKind::Audio(AudioClip {
                source: path,
                warped: false,
                source_offset_frames: 0,
                gain: 1.0,
                fade_in_ms: 0,
                fade_out_ms: 0,
            }),
            scale_override: None,
        };
        if let Some(scene) = scene {
            self.insert_session_clip(track_id, scene, clip);
            return;
        }
        let Some(track) = self.project.track_mut(track_id) else {
            return;
        };
        if track.kind != TrackKind::Audio {
            return;
        }
        clip.start = MusicalTime::from_ticks(
            track
                .clips
                .last()
                .map_or(0, |c| c.start.ticks().saturating_add(c.duration.ticks())),
        );
        self.selected_clip = Some(clip.id);
        self.next_id += 1;
        track.clips.push(clip);
        self.selected_track = Some(track_id);
        self.detail = Detail::Clip;
        self.dirty = true;
        self.restart_for_edit();
    }

    fn grid_ticks(&self) -> i64 {
        match self.project.session.as_ref().unwrap().quantization {
            LaunchGrid::Sixteenth => PPQ / 4,
            LaunchGrid::Quarter => PPQ,
            LaunchGrid::Bar => self.bar_ticks(),
        }
    }

    fn sync_session_active(&mut self) {
        self.session_active.clear();
        for track in &self.project.tracks {
            if let Some(clip) = track.session_slots.values().find(|clip| {
                self.session_meters
                    .get(&clip.id)
                    .is_some_and(|m| m.state().0)
            }) {
                self.session_active.insert(track.id, clip.id);
            }
        }
    }

    fn session_targets(&mut self, targets: &[(u64, Option<u64>)]) {
        if self.playing && self.playback_view == View::Session {
            let mut launch = SessionLaunch::new(self.grid_ticks());
            for (lane, track) in self.project.tracks.iter().enumerate() {
                if let Some((_, clip)) = targets.iter().find(|(id, _)| *id == track.id) {
                    launch.tracks[lane] = clip.map_or(TrackLaunch::Stop, TrackLaunch::Clip);
                }
            }
            self.submit_session_launch(launch);
            return;
        }
        if self.recorder.is_some() || self.midi_take.is_some() {
            self.error = Some("Finish recording before switching playback to Session".into());
            return;
        }
        for &(track, clip) in targets {
            if let Some(id) = clip {
                self.session_active.insert(track, id);
            } else {
                self.session_active.remove(&track);
            }
        }
        if !self.session_active.is_empty() {
            if !self.playing {
                self.playhead_frame = 0;
            }
            self.start_playback();
        }
    }

    fn submit_session_launch(&mut self, launch: SessionLaunch) {
        if let Some(driver) = self.driver.as_mut()
            && driver.controller().launch_session(launch).is_err()
        {
            self.error =
                Some("Session queue full. Entire request rejected. Retry the launch.".into());
        }
    }

    fn session_launch_clip(&mut self, track: u64, clip: u64) {
        self.selected_track = Some(track);
        self.selected_clip = Some(clip);
        self.session_targets(&[(track, Some(clip))]);
    }

    fn session_launch_scene(&mut self, scene: u64) {
        let targets: Vec<_> = self
            .project
            .tracks
            .iter()
            .filter(|t| matches!(t.kind, TrackKind::Midi | TrackKind::Audio))
            .map(|t| (t.id, t.session_slots.get(&scene).map(|c| c.id)))
            .collect();
        self.session_targets(&targets);
    }

    fn session_stop_track(&mut self, track: u64) {
        if self.playing && self.playback_view == View::Session {
            self.session_targets(&[(track, None)]);
        } else {
            self.session_active.remove(&track);
        }
    }
    fn studio_session(&mut self, ui: &mut egui::Ui) {
        let mut launch_scene = None;
        let mut launch_clip = None;
        let mut stop_track = None;
        let mut select = None;
        let mut changed_tracks = Vec::new();
        let mut stop_all = false;
        let mut append_midi = None;
        let mut append_audio = None;
        let mut track_action = None;
        let mut clip_action = None;
        let groups = self.project.tracks.iter()
            .filter(|t| t.kind == TrackKind::Group)
            .map(|t| (t.id, t.name.clone())).collect::<Vec<_>>();

        ui.horizontal_wrapped(|ui| {
            ui.label(egui::RichText::new("LIVE CLIPS").strong().size(13.0).color(OFFWHITE))
                .on_hover_text("Launch clips on the selected grid with a 128-frame fade.");
            ui.separator();
            let layout = self.project.session.as_mut().unwrap();
            let before = layout.quantization;
            egui::ComboBox::from_id_salt("launch_grid").selected_text(format!("{:?}", before)).show_ui(ui, |ui| {
                ui.selectable_value(&mut layout.quantization, LaunchGrid::Sixteenth, "Sixteenth");
                ui.selectable_value(&mut layout.quantization, LaunchGrid::Quarter, "Quarter");
                ui.selectable_value(&mut layout.quantization, LaunchGrid::Bar, "Bar");
            }).response.on_hover_text("Session launch quantization");
            self.dirty |= before != layout.quantization;
            if ui.add_enabled(layout.scenes.len() < MAX_SCENES, egui::Button::new("＋ Scene")).clicked() {
                let id = (1..=u64::MAX).find(|id| !layout.scenes.contains(id)).unwrap();
                layout.scenes.push(id);
                self.dirty = true;
            }
            if ui.button("Cancel queued").on_hover_text("Keep currently playing clips and cancel pending launches").clicked() {
                let mut launch = SessionLaunch::new(1);
                launch.tracks.fill(TrackLaunch::Cancel);
                self.submit_session_launch(launch);
            }
            if ui.button("■  Stop all").clicked() { stop_all = true; }
            if ui.button("Mixer").clicked() { self.detail = Detail::Mixer; }
            if ui.button("+ MIDI").clicked() { track_action = Some(TrackAction::AddMidi); }
            if ui.button("+ Audio").clicked() { track_action = Some(TrackAction::AddAudio); }
            if ui.button("+ Group").clicked() { track_action = Some(TrackAction::NewGroup(None)); }
        });
        ui.separator();

        let scene_w = 114.0;
        let count = self.project.tracks.len().max(1) as f32;
        // Fill the available stage with real tracks, without stretching narrow
        // screens beyond usable control sizes. Overflow remains scrollable.
        let track_w = ((ui.available_width() - scene_w - 6.0) / count).clamp(192.0, 410.0);
        let input_meter = self.visible_input_meter();
        let take = self.recording_track_display();
        let has_audio_tracks = self.project.tracks.iter().any(|track| track.kind == TrackKind::Audio);
        let header_h = 88.0 + if has_audio_tracks { 20.0 } else { 0.0 }
            + if take.is_some() { 22.0 } else { 0.0 };
        let row_h = 65.0;
        let scene_ids = &self.project.session.as_ref().unwrap().scenes;
        let scenes = scene_ids.len();
        let width = (scene_w + self.project.tracks.len() as f32 * track_w).max(420.0);
        let height = header_h + scenes as f32 * row_h + 66.0;
        let playhead = self.playhead_frame;
        let playing = self.playing && self.playback_view == View::Session;
        let selected_track = self.selected_track;
        let selected_clip = self.selected_clip;
        let active = &self.session_active;
        let meters = &self.session_meters;

        egui::ScrollArea::both()
            .id_salt("slate_session_surface")
            .scroll_source(egui::scroll_area::ScrollSource::SCROLL_BAR |
                egui::scroll_area::ScrollSource::MOUSE_WHEEL)
            .auto_shrink([false, false]).show(ui, |ui| {
                let (rect, background_response) = ui.allocate_exact_size(
                    Vec2::new(width, height), Sense::click());
                background_response.context_menu(|ui| empty_track_context_menu(
                    ui, &mut track_action));
                let painter = ui.painter_at(rect);
                painter.rect_filled(rect, 0.0, BG_CANVAS);
                painter.rect_filled(Rect::from_min_size(rect.min,
                    Vec2::new(width, header_h)), 0.0, BG_HEADER);
                painter.text(rect.min + Vec2::new(15.0, 16.0),
                    egui::Align2::LEFT_TOP, "SCENES",
                    FontId::proportional(11.0), STUDIO_DIM);

                for (track_idx, track) in self.project.tracks.iter_mut().enumerate() {
                    let x = rect.left() + scene_w + track_idx as f32 * track_w;
                    let header = Rect::from_min_size(
                        Pos2::new(x + 3.0, rect.top() + 4.0),
                        Vec2::new(track_w - 6.0, header_h - 8.0));
                    let color = match track.kind {
                        TrackKind::Midi => TEAL,
                        TrackKind::Group => CLIP_PURPLE,
                        _ => CLIP_PINK,
                    };
                    painter.rect_filled(header, 9.0,
                        if selected_track == Some(track.id) {
                            ACCENT_TINT
                        } else { INK_2 });
                    painter.rect_filled(Rect::from_min_size(header.min,
                        Vec2::new(3.0, header.height())), 0.0, color);
                    let name = if track.name.chars().count() > 17 {
                        format!("{}…", track.name.chars().take(16).collect::<String>())
                    } else { track.name.clone() };
                    let name = if track.group_id.is_some() { format!("↳ {name}") }
                        else if track.kind == TrackKind::Group { format!("▣ {name}") }
                        else { name };
                    let header_response = ui.interact(
                        Rect::from_min_size(header.min, Vec2::new(track_w - 10.0, 26.0)),
                        egui::Id::new(("session_track_header", track.id)), Sense::click());
                    if header_response.clicked() { self.selected_track = Some(track.id); }
                    let id = track.id;
                    let kind = track.kind;
                    let parent = track.group_id;
                    header_response.context_menu(|ui| track_context_menu(
                        ui, id, kind, parent, &groups, &mut track_action));
                    painter.text(header.min + Vec2::new(12.0, 9.0),
                        egui::Align2::LEFT_TOP, name,
                        FontId::proportional(12.0), OFFWHITE);
                    painter.text(header.min + Vec2::new(12.0, 29.0),
                        egui::Align2::LEFT_TOP,
                        match track.kind {
                            TrackKind::Midi => "MIDI", TrackKind::Group => "GROUP BUS", _ => "AUDIO",
                        },
                        FontId::proportional(9.0), Color32::from_gray(153));
                    let mut mute_button = egui::Button::new("M").small();
                    if track.muted { mute_button = mute_button.fill(SUNSET); }
                    if ui.put(Rect::from_min_size(
                        header.min + Vec2::new(track_w - 69.0, 27.0), Vec2::new(24.0, 21.0)),
                        mute_button).clicked() {
                        track.muted = !track.muted;
                        changed_tracks.push(track.id);
                    }
                    let mut solo_button = egui::Button::new("S").small();
                    if track.solo { solo_button = solo_button.fill(TEAL); }
                    if ui.put(Rect::from_min_size(
                        header.min + Vec2::new(track_w - 42.0, 27.0), Vec2::new(24.0, 21.0)),
                        solo_button).clicked() {
                        track.solo = !track.solo;
                        changed_tracks.push(track.id);
                    }
                    if ui.put(Rect::from_min_size(
                        header.min + Vec2::new(12.0, 53.0),
                        Vec2::new(track_w - 65.0, 19.0)),
                        egui::Slider::new(&mut track.volume, 0.0..=1.5)
                            .show_value(false).trailing_fill(true)).changed() {
                        changed_tracks.push(track.id);
                    }
                    if ui.put(Rect::from_min_size(
                        header.min + Vec2::new(track_w - 40.0, 53.0),
                        Vec2::new(29.0, 19.0)),
                        egui::Button::new("■").small())
                        .on_hover_text("Stop this track").clicked() {
                        stop_track = Some(track.id);
                    }
                    if track.kind == TrackKind::Audio {
                        let active_take = take.as_ref().filter(|take| take.track_id == track.id);
                        let meter_rect = Rect::from_min_size(header.min + Vec2::new(12.0, 77.0),
                            Vec2::new(header.width() - 24.0, 17.0));
                        paint_input_meter(ui, meter_rect, track.id, input_meter.as_ref(),
                            active_take.map(|take| take.elapsed.as_str()));
                        if let Some(take) = active_take {
                            let visible_frames = take.frames.max(10 * u64::from(self.sample_rate));
                            let pixels_per_frame = meter_rect.width() / visible_frames as f32;
                            let preview = Rect::from_min_size(
                                Pos2::new(meter_rect.left(), meter_rect.bottom() + 6.0),
                                Vec2::new((take.frames as f32 * pixels_per_frame).max(2.0), 17.0),
                            );
                            painter.rect_filled(preview, 3.0, RECORDING_FILL);
                            paint_recording_waveform(&painter, preview, take, pixels_per_frame);
                        }
                    }
                }

                for (scene, &scene_id) in scene_ids.iter().enumerate() {
                    let y = rect.top() + header_h + scene as f32 * row_h;
                    let scene_header = Rect::from_min_size(
                        Pos2::new(rect.left(), y),
                        Vec2::new(scene_w, row_h));
                    painter.rect_filled(scene_header, 0.0,
                        if scene % 2 == 0 { INK_2 }
                        else { INK_1 });
                    painter.text(scene_header.min + Vec2::new(12.0, 10.0),
                        egui::Align2::LEFT_TOP, format!("{:02}", scene + 1),
                        FontId::monospace(12.0), OFFWHITE);
                    if ui.put(Rect::from_min_size(
                        scene_header.min + Vec2::new(12.0, 29.0),
                        Vec2::new(91.0, 27.0)),
                        egui::Button::new("▶  Launch").small())
                        .on_hover_text("Launch all clips on this row")
                        .clicked() { launch_scene = Some(scene_id); }

                    for (track_idx, track) in self.project.tracks.iter().enumerate() {
                        let x = rect.left() + scene_w + track_idx as f32 * track_w + 4.0;
                        let slot = Rect::from_min_size(Pos2::new(x, y + 4.0),
                            Vec2::new(track_w - 8.0, row_h - 8.0));
                        let lane_response = ui.interact(slot,
                            egui::Id::new(("session_lane", scene, track.id)), Sense::click());
                        let id = track.id;
                        lane_response.context_menu(|ui| track_context_menu(
                            ui, id, track.kind, track.group_id, &groups, &mut track_action));
                        if !matches!(track.kind, TrackKind::Midi | TrackKind::Audio) {
                            painter.rect_filled(slot, 8.0, INK_2);
                            painter.text(slot.center(), egui::Align2::CENTER_CENTER,
                                "GROUP BUS", FontId::proportional(11.0), STUDIO_DIM);
                            continue;
                        }
                        if let Some(clip) = track.session_slots.get(&scene_id) {
                            let is_active = active.get(&track.id) == Some(&clip.id);
                            let (_, is_queued, _) = meters.get(&clip.id).map_or((false, false, 0), |m| m.state());
                            let is_selected = selected_clip == Some(clip.id);
                            let color = if track.kind == TrackKind::Midi {
                                match track_idx % 3 {
                                    0 => MIDI_GREEN,
                                    1 => CLIP_PURPLE,
                                    _ => ACCENT_DARK,
                                }
                            } else { CLIP_PINK };
                            painter.rect_filled(slot, 8.0,
                                if is_active { ACCENT_DARK }
                                else { color });
                            painter.rect_stroke(slot, 8.0,
                                Stroke::new(if is_selected { 1.5_f32 } else { 0.7_f32 },
                                    if is_active { ACCENT_BRIGHT }
                                    else if is_selected { OFFWHITE }
                                    else { STUDIO_BORDER }),
                                egui::StrokeKind::Inside);
                            let title = match &clip.kind {
                                ClipKind::Midi(_) => track.name.clone(),
                                ClipKind::Audio(audio) => audio.source.file_stem()
                                    .unwrap_or_default().to_string_lossy().to_string(),
                            };
                            let label = if title.chars().count() > 15 {
                                format!("{}…", title.chars().take(14).collect::<String>())
                            } else { title };
                            painter.text(slot.min + Vec2::new(12.0, 8.0),
                                egui::Align2::LEFT_TOP, label,
                                FontId::proportional(12.0), OFFWHITE);
                            painter.text(slot.min + Vec2::new(12.0, 30.0),
                                egui::Align2::LEFT_TOP,
                                if is_queued && is_active { "◷ stopping" } else if is_queued { "◷ queued" }
                                else if is_active && playing { "● looping" }
                                else if is_active { "● armed" } else { "launch clip" },
                                FontId::proportional(10.0),
                                if is_active { ACCENT_BRIGHT }
                                else { Color32::from_gray(161) });
                            if is_active && playing {
                                let progress = meters.get(&clip.id).map_or(0.0, |m| m.progress(playhead));
                                painter.rect_filled(Rect::from_min_size(
                                    Pos2::new(slot.left() + 6.0, slot.bottom() - 4.0),
                                    Vec2::new((slot.width() - 12.0) * progress, 2.0)),
                                    1.0, ACCENT);
                            }
                            let clip_response = ui.interact(slot,
                                egui::Id::new(("session_select", clip.id)), Sense::click());
                            if clip_response.clicked() {
                                select = Some((track.id, clip.id));
                            }
                            let id = track.id;
                            clip_response.context_menu(|ui| clip_context_menu(
                                ui, ClipMenuTarget { track: id, clip: clip.id, kind: track.kind,
                                    parent: track.group_id, can_split: false }, &groups,
                                &mut track_action, &mut clip_action));
                            if ui.put(Rect::from_min_size(Pos2::new(slot.right() - 39.0,
                                slot.top() + 8.0), Vec2::new(32.0, 34.0)),
                                egui::Button::new("▶").small())
                                .on_hover_text("Launch this clip in the live engine")
                                .clicked() {
                                launch_clip = Some((track.id, clip.id));
                            }
                        } else {
                            painter.rect_filled(slot, 8.0, INK_1);
                            painter.rect_stroke(slot, 8.0,
                                Stroke::new(0.65_f32, GRID_FAINT), egui::StrokeKind::Inside);
                            {
                                let title = if track.kind == TrackKind::Midi {
                                    "＋ New MIDI clip"
                                } else {
                                    "＋ Import audio"
                                };
                                if ui.put(Rect::from_min_size(
                                    Pos2::new(slot.left() + 9.0, slot.top() + 12.0),
                                    Vec2::new(slot.width() - 18.0, 30.0)),
                                    egui::Button::new(title).small()).clicked() {
                                    if track.kind == TrackKind::Midi {
                                        append_midi = Some((track.id, scene_id));
                                    } else {
                                        append_audio = Some((track.id, scene_id));
                                    }
                                }
                            }
                        }
                    }
                }
                let bottom = rect.top() + header_h + scenes as f32 * row_h;
                let master_rect = Rect::from_min_size(Pos2::new(rect.left() + 3.0, bottom + 5.0),
                    Vec2::new(scene_w - 6.0, 53.0));
                painter.rect_filled(master_rect, 8.0, INK_3);
                painter.text(master_rect.min + Vec2::new(11.0, 11.0),
                    egui::Align2::LEFT_TOP, "MASTER",
                    FontId::proportional(12.0), OFFWHITE);
                painter.text(master_rect.min + Vec2::new(11.0, 30.0),
                    egui::Align2::LEFT_TOP, "Stereo", FontId::proportional(10.0),
                    Color32::from_gray(150));
            });
        if let Some((track, clip)) = select {
            self.selected_track = Some(track);
            self.selected_clip = Some(clip);
            self.detail = Detail::Clip;
        }
        if !changed_tracks.is_empty() { self.volume_changed(changed_tracks[0]); }
        if stop_all {
            if playing {
                let mut launch = SessionLaunch::new(self.grid_ticks());
                launch.tracks.fill(TrackLaunch::Stop);
                self.submit_session_launch(launch);
            } else { self.session_active.clear(); }
        }
        if let Some(track) = stop_track { self.session_stop_track(track); }
        if let Some(scene) = launch_scene { self.session_launch_scene(scene); }
        if let Some((track, clip)) = launch_clip { self.session_launch_clip(track, clip); }
        if let Some((track, scene)) = append_midi { self.session_append_midi(track, scene); }
        if let Some((track, scene)) = append_audio
            && let Some(path) = Self::pick_path(false, true) {
            self.import_audio_cell(track, Some(scene), path);
        }
        if let Some(action) = track_action { self.apply_track_action(action); }
        if let Some(action) = clip_action { self.apply_clip_action(action); }
    }
}

#[cfg(test)]
mod piano_tests {
    use super::*;
    #[test]
    fn quantization_rounds_to_nearest_positive_step() {
        assert_eq!(snap_piano_tick(270, 240), 240);
        assert_eq!(snap_piano_tick(399, 240), 480);
        assert_eq!(snap_piano_tick(-100, 240), 0);
    }
}
