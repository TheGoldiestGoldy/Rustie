// Track management used by BOTH Arrangement and Session. Group strips are
// actual graph return buses, not labels or duplicated child faders.
#[derive(Debug, Clone, PartialEq, Eq)]
enum TrackAction {
    AddMidi,
    AddAudio,
    ImportAudioToTrack(u64),
    NewGroup(Option<u64>),
    AddMidiToGroup(u64),
    AddAudioToGroup(u64),
    BeginRename(u64),
    MoveToGroup(u64, u64),
    Ungroup(u64),
    Remove(u64),
}

fn track_context_menu(
    ui: &mut egui::Ui,
    track_id: u64,
    kind: TrackKind,
    current_group: Option<u64>,
    groups: &[(u64, String)],
    action: &mut Option<TrackAction>,
) {
    if ui.button("Rename track…").clicked() {
        *action = Some(TrackAction::BeginRename(track_id));
        ui.close();
    }
    ui.separator();
    if ui.button("＋ New MIDI track").clicked() {
        *action = Some(TrackAction::AddMidi);
        ui.close();
    }
    if ui.button("＋ New audio track…").clicked() {
        *action = Some(TrackAction::AddAudio);
        ui.close();
    }
    if kind == TrackKind::Audio && ui.button("Import audio clip into this track…").clicked() {
        *action = Some(TrackAction::ImportAudioToTrack(track_id));
        ui.close();
    }
    if kind == TrackKind::Group {
        if ui.button("＋ MIDI track in this group").clicked() {
            *action = Some(TrackAction::AddMidiToGroup(track_id));
            ui.close();
        }
        if ui.button("＋ Audio track in this group…").clicked() {
            *action = Some(TrackAction::AddAudioToGroup(track_id));
            ui.close();
        }
    } else {
        if ui.button("Group this track into new bus").clicked() {
            *action = Some(TrackAction::NewGroup(Some(track_id)));
            ui.close();
        }
        if !groups.is_empty() {
            ui.menu_button("Move to group ▸", |ui| {
                for (id, name) in groups {
                    if Some(*id) != current_group && ui.button(name).clicked() {
                        *action = Some(TrackAction::MoveToGroup(track_id, *id));
                        ui.close();
                    }
                }
            });
        }
        if current_group.is_some() && ui.button("Remove from group").clicked() {
            *action = Some(TrackAction::Ungroup(track_id));
            ui.close();
        }
    }
    ui.separator();
    if ui.button(if kind == TrackKind::Group {
        "Delete group (keep its tracks)"
    } else { "Delete track" }).clicked() {
        *action = Some(TrackAction::Remove(track_id));
        ui.close();
    }
}

fn empty_track_context_menu(ui: &mut egui::Ui, action: &mut Option<TrackAction>) {
    if ui.button("＋ New MIDI track").clicked() {
        *action = Some(TrackAction::AddMidi);
        ui.close();
    }
    if ui.button("＋ New empty audio track").clicked() {
        *action = Some(TrackAction::AddAudio);
        ui.close();
    }
    if ui.button("＋ New empty group bus").clicked() {
        *action = Some(TrackAction::NewGroup(None));
        ui.close();
    }
}

impl Studio {
    fn apply_track_action(&mut self, action: TrackAction) {
        match action {
            TrackAction::AddMidi => self.add_midi_track(),
            TrackAction::AddAudio => self.add_audio_track(),
            TrackAction::ImportAudioToTrack(id) => {
                if let Some(path) = Self::pick_path(false, true) {
                    self.session_append_audio(id, path);
                }
            }
            TrackAction::AddMidiToGroup(group) => {
                if self.project.tracks.iter().any(|t| t.id == group && t.kind == TrackKind::Group) {
                    self.add_midi_track();
                    if let Some(id) = self.selected_track { self.assign_group(id, Some(group)); }
                }
            }
            TrackAction::AddAudioToGroup(group) => {
                if self.project.tracks.iter().any(|t| t.id == group && t.kind == TrackKind::Group) {
                    self.add_audio_track();
                    if let Some(id) = self.selected_track { self.assign_group(id, Some(group)); }
                }
            }
            TrackAction::NewGroup(member) => {
                if self.project.tracks.len() >= 48 {
                    self.error = Some("Maximum 48 tracks and groups".into());
                    return;
                }
                let id = self.next_id;
                self.next_id = self.next_id.saturating_add(1);
                let n = self.project.tracks.iter()
                    .filter(|t| t.kind == TrackKind::Group).count() + 1;
                let group = Track::new(id, format!("Group {n}"), TrackKind::Group);
                let index = member.and_then(|member| self.project.tracks.iter()
                    .position(|track| track.id == member))
                    .unwrap_or(self.project.tracks.len());
                self.project.tracks.insert(index, group);
                if let Some(member) = member
                    && let Some(t) = self.project.track_mut(member)
                        && t.kind != TrackKind::Group { t.group_id = Some(id); }
                self.selected_track = Some(id);
                self.dirty = true;
                self.restart_for_edit();
            }
            TrackAction::BeginRename(id) => {
                if let Some(track) = self.project.tracks.iter().find(|track| track.id == id) {
                    self.track_rename = Some((id, track.name.clone()));
                }
            }
            TrackAction::MoveToGroup(id, group) => self.assign_group(id, Some(group)),
            TrackAction::Ungroup(id) => self.assign_group(id, None),
            TrackAction::Remove(id) => self.track_delete = Some(id),
        }
    }

    fn remove_track_confirmed(&mut self, id: u64) {
        if self.recorder.is_some() {
            self.error = Some("Finish the current WAV take before deleting tracks; otherwise its destination could disappear.".into());
            return;
        }
        let Some(index) = self.project.tracks.iter().position(|t| t.id == id) else { return };
        self.project.tracks.remove(index);
        for child in &mut self.project.tracks {
            if child.group_id == Some(id) { child.group_id = None; }
        }
        self.session_active.remove(&id);
        self.selected_track = self.project.tracks.first().map(|t| t.id);
        self.selected_clip = self.project.tracks.iter()
            .flat_map(|t| &t.clips).next().map(|clip| clip.id);
        self.piano_selected = None;
        self.dirty = true;
        self.restart_for_edit();
    }

    fn draw_delete_window(&mut self, ctx: &egui::Context) {
        let Some(id) = self.track_delete else { return };
        let Some(track) = self.project.tracks.iter().find(|t| t.id == id) else {
            self.track_delete = None;
            return;
        };
        let is_group = track.kind == TrackKind::Group;
        let name = track.name.clone();
        let mut confirmed = false;
        let mut cancelled = false;
        egui::Window::new("Confirm track removal")
            .id(egui::Id::new("slate_delete_track"))
            .collapsible(false).resizable(false)
            .show(ctx, |ui| {
                ui.label(format!("Remove ‘{name}’?"));
                if is_group {
                    ui.weak("Tracks inside this group will be kept, but ungrouped.");
                } else {
                    ui.colored_label(ACCENT_BRIGHT,
                        "This deletes the track and its clips. Undo is not yet available.");
                }
                ui.horizontal(|ui| {
                    if ui.button("Remove").clicked() { confirmed = true; }
                    if ui.button("Cancel").clicked() { cancelled = true; }
                });
                if ui.input(|i| i.key_pressed(egui::Key::Escape)) { cancelled = true; }
            });
        if cancelled { self.track_delete = None; }
        if confirmed {
            self.track_delete = None;
            self.remove_track_confirmed(id);
        }
    }

    fn assign_group(&mut self, track_id: u64, group_id: Option<u64>) {
        if let Some(id) = group_id
            && !self.project.tracks.iter().any(|t|
                t.id == id && t.kind == TrackKind::Group) {
            return;
        }
        let Some(index) = self.project.tracks.iter().position(|t|
            t.id == track_id && t.kind != TrackKind::Group) else { return };
        let mut track = self.project.tracks.remove(index);
        track.group_id = group_id;
        let target = if let Some(group) = group_id {
            self.project.tracks.iter().enumerate().filter(|(_, t)|
                t.id == group || t.group_id == Some(group))
                .map(|(index, _)| index + 1).max().unwrap_or(self.project.tracks.len())
        } else { self.project.tracks.len() };
        self.project.tracks.insert(target, track);
        self.dirty = true;
        self.restart_for_edit();
    }

    fn draw_rename_window(&mut self, ctx: &egui::Context) {
        let Some((id, text)) = self.track_rename.as_mut() else { return };
        let id = *id;
        let mut confirm = false;
        let mut cancel = false;
        egui::Window::new("Rename track")
            .id(egui::Id::new("slate_track_rename"))
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label("Track / group name");
                let field = ui.add(egui::TextEdit::singleline(text).desired_width(270.0));
                if field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    confirm = true;
                }
                ui.horizontal(|ui| {
                    if ui.button("Save name").clicked() { confirm = true; }
                    if ui.button("Cancel").clicked() { cancel = true; }
                });
                if ui.input(|i| i.key_pressed(egui::Key::Escape)) { cancel = true; }
            });
        if cancel { self.track_rename = None; }
        else if confirm {
            let name = self.track_rename.as_ref()
                .map(|(_, name)| name.trim()).unwrap_or("").to_string();
            if !name.is_empty() {
                if let Some(t) = self.project.track_mut(id) { t.name = name; }
                self.dirty = true;
                self.track_rename = None;
            }
        }
    }

    fn mixer_editor(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("MIXER")
                .size(12.0).strong().color(ACCENT));
            ui.separator();
            ui.label(egui::RichText::new("CH. STRIPS  /  AUDIO ROUTING")
                .size(10.0).color(STUDIO_DIM));
            ui.separator();
            if ui.small_button("＋ MIDI").clicked() { self.add_midi_track(); }
            if ui.small_button("＋ AUDIO").clicked() { self.add_audio_track(); }
            if ui.small_button("＋ GROUP").clicked() {
                self.apply_track_action(TrackAction::NewGroup(None));
            }
        });
        ui.separator();

        let mut changed = false;
        let mut select = None;
        let mut action = None;
        let groups = self.project.tracks.iter()
            .filter(|t| t.kind == TrackKind::Group)
            .map(|t| (t.id, t.name.clone())).collect::<Vec<_>>();
        let selected = self.selected_track;
        let input_meter = self.visible_input_meter();
        let recording = self.recorder.as_ref().map(|take| {
            (take.track_id, recording_elapsed(take.progress_frames, self.sample_rate))
        });
        let peak = self.driver.as_ref().map_or(0.0, |d| f32::from_bits(
            d.telemetry().current_peak_bits.load(Ordering::Relaxed)
        )).clamp(0.0, 1.0);
        let fader_height = (ui.available_height() - 150.0).clamp(92.0, 218.0);
        // A console has tall, separate channel strips. In a narrow window,
        // the strips scroll horizontally *without* squeezing their contents.
        egui::ScrollArea::both()
            .id_salt("mixer_channel_strips")
            .scroll_source(egui::scroll_area::ScrollSource::SCROLL_BAR |
                egui::scroll_area::ScrollSource::MOUSE_WHEEL)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = 7.0;
                    for track in &mut self.project.tracks {
                        let is_group = track.kind == TrackKind::Group;
                        let kind = track.kind;
                        let id = track.id;
                        let group_id = track.group_id;
                        let color = if is_group { CLIP_PURPLE }
                            else if kind == TrackKind::Midi { ACCENT }
                            else { CLIP_PINK };
                        egui::Frame::new()
                            .fill(if selected == Some(id) { ACCENT_TINT } else { INK_2 })
                            .stroke(Stroke::new(1.0_f32, if selected == Some(id) {
                                ACCENT_DARK
                            } else { STUDIO_BORDER }))
                            .corner_radius(7)
                            .inner_margin(egui::Margin::symmetric(9, 10))
                            .show(ui, |ui| {
                                ui.set_width(117.0);
                                ui.vertical_centered(|ui| {
                                    let (tab, _) = ui.allocate_exact_size(
                                        Vec2::new(114.0, 3.0), Sense::hover());
                                    ui.painter().rect_filled(tab, 1.0, color);
                                    let label = if is_group {
                                        format!("▣ {}", track.name)
                                    } else if group_id.is_some() {
                                        format!("↳ {}", track.name)
                                    } else { track.name.clone() };
                                    let short = if label.chars().count() > 14 {
                                        format!("{}…", label.chars().take(13).collect::<String>())
                                    } else { label.clone() };
                                    let name = ui.add_sized([115.0, 28.0],
                                        egui::Button::new(egui::RichText::new(short)
                                            .size(11.0).color(OFFWHITE))
                                            .fill(INK_3)).on_hover_text(&label);
                                    if name.clicked() { select = Some(id); }
                                    name.context_menu(|ui| track_context_menu(
                                        ui, id, kind, group_id, &groups, &mut action));
                                    ui.label(egui::RichText::new(if is_group { "GROUP BUS" }
                                        else if kind == TrackKind::Midi { "INSTRUMENT" }
                                        else { "AUDIO" }).color(STUDIO_DIM).size(9.0));
                                    if kind == TrackKind::Audio {
                                        let (meter_rect, _) = ui.allocate_exact_size(Vec2::new(109.0, 20.0), Sense::hover());
                                        let elapsed = recording.as_ref().filter(|(track, _)| *track == id)
                                            .map(|(_, elapsed)| elapsed.as_str());
                                        paint_input_meter(ui, meter_rect, id, input_meter.as_ref(), elapsed);
                                    }
                                    ui.add_space(5.0);
                                    ui.horizontal(|ui| {
                                        if ui.add_sized([49.0, 24.0],
                                            egui::Button::new("M")
                                                .fill(if track.muted { SUNSET } else { INK_3 }))
                                            .on_hover_text("Mute channel").clicked() {
                                            track.muted = !track.muted;
                                            changed = true;
                                        }
                                        if ui.add_sized([49.0, 24.0],
                                            egui::Button::new("S")
                                                .fill(if track.solo { TEAL } else { INK_3 }))
                                            .on_hover_text("Solo channel").clicked() {
                                            track.solo = !track.solo;
                                            changed = true;
                                        }
                                    });
                                    ui.add_space(7.0);
                                    ui.label(egui::RichText::new("LEVEL")
                                        .size(9.0).color(STUDIO_DIM));
                                    ui.horizontal(|ui| {
                                        ui.add_space(39.0);
                                        if ui.add_sized([34.0, fader_height],
                                            egui::Slider::new(&mut track.volume, 0.0..=1.5)
                                                .vertical().show_value(false).trailing_fill(true))
                                            .on_hover_text("Channel level (0–150%)").changed() {
                                            changed = true;
                                        }
                                    });
                                    ui.label(egui::RichText::new(format!("{:.0}%", track.volume * 100.0))
                                        .monospace().size(11.0).color(OFFWHITE));
                                    ui.add_space(5.0);
                                    ui.label(egui::RichText::new("PAN")
                                        .size(9.0).color(STUDIO_DIM));
                                    if ui.add_sized([109.0, 18.0],
                                        egui::Slider::new(&mut track.pan, -1.0..=1.0)
                                            .show_value(false)).changed() {
                                        changed = true;
                                    }
                                    let pan_text = if track.pan.abs() < 0.02 {
                                        "CENTER".to_string()
                                    } else if track.pan < 0.0 {
                                        format!("L {:.0}", track.pan.abs() * 100.0)
                                    } else { format!("R {:.0}", track.pan * 100.0) };
                                    ui.label(egui::RichText::new(pan_text)
                                        .monospace().size(10.0).color(OFFWHITE));
                                    ui.add_space(3.0);
                                    ui.label(egui::RichText::new(if group_id.is_some() {
                                        "↳ GROUP"
                                    } else if is_group { "BUS → MASTER" }
                                    else { "→ MASTER" }).size(9.0).color(STUDIO_DIM));
                                });
                            });
                    }
                    egui::Frame::new()
                        .fill(INK_3)
                        .stroke(Stroke::new(1.0_f32, ACCENT_DARK))
                        .corner_radius(7)
                        .inner_margin(egui::Margin::symmetric(10, 10))
                        .show(ui, |ui| {
                            ui.set_width(117.0);
                            ui.vertical_centered(|ui| {
                                let (tab, _) = ui.allocate_exact_size(
                                    Vec2::new(114.0, 3.0), Sense::hover());
                                ui.painter().rect_filled(tab, 1.0, ACCENT);
                                ui.add_space(6.0);
                                ui.label(egui::RichText::new("MASTER")
                                    .size(13.0).strong().color(OFFWHITE));
                                ui.label(egui::RichText::new("STEREO OUT")
                                    .size(9.0).color(STUDIO_DIM));
                                ui.add_space(48.0);
                                ui.label(egui::RichText::new("PEAK")
                                    .size(9.0).color(STUDIO_DIM));
                                let meter_height = fader_height + 14.0;
                                let (r, resp) = ui.allocate_exact_size(
                                    Vec2::new(30.0, meter_height), Sense::hover());
                                let painter = ui.painter();
                                painter.rect_filled(r, 3.0, INK_0);
                                let filled = Rect::from_min_max(
                                    Pos2::new(r.left() + 5.0,
                                        r.bottom() - 5.0 - (r.height() - 10.0) * peak),
                                    Pos2::new(r.right() - 5.0, r.bottom() - 5.0));
                                painter.rect_filled(filled, 2.0,
                                    if peak > 0.88 { SUNSET } else { ACCENT });
                                for i in 1..=4 {
                                    let y = r.top() + i as f32 * r.height() / 5.0;
                                    painter.line_segment([
                                        Pos2::new(r.left() - 5.0, y),
                                        Pos2::new(r.right() + 5.0, y),
                                    ], Stroke::new(0.7_f32, STUDIO_BORDER));
                                }
                                resp.on_hover_text("Live master output peak, not a simulated meter");
                                ui.label(egui::RichText::new(format!("{:.1}%", peak * 100.0))
                                    .monospace().size(11.0).color(OFFWHITE));
                                ui.add_space(16.0);
                                ui.label(egui::RichText::new("No master FX")
                                    .size(9.0).color(STUDIO_DIM));
                            });
                        });
                });
            });
        if let Some(id) = select { self.selected_track = Some(id); }
        if changed { self.volume_changed(self.selected_track.unwrap_or(0)); }
        if let Some(action) = action { self.apply_track_action(action); }
    }

}

#[cfg(test)]
mod group_mixer_tests {
    use super::*;

    #[test]
    fn group_return_bus_controls_real_audio_from_child_midi_node() {
        let child_id = Studio::track_midi_node(2).unwrap();
        let group_id = Studio::group_node(1).unwrap();
        let mut builder = GraphBuilder::new(48_000);
        builder.add_node(NodeSpec::midi(child_id));
        builder.add_node(NodeSpec::returns(group_id));
        builder.add_node(NodeSpec::master(MASTER_ID));
        builder.route(child_id, group_id, 1.0);
        builder.route(group_id, MASTER_ID, 1.0);
        let mut graph = builder.build().unwrap();
        graph.note_on(child_id, 69, 100);
        let mut peak = 0.0_f32;
        for frame in 0..256_u64 {
            let rendered = graph.render_frame(frame);
            peak = peak.max(rendered.left.abs());
        }
        assert!(peak > 0.001, "Child synth did not reach group bus/master");
        graph.set_gain(group_id, 0.0);
        for frame in 256..512 {
            let rendered = graph.render_frame(frame);
            assert_eq!(rendered, StereoFrame::ZERO,
                "Muted group leaked child audio at frame {frame}");
        }
    }
}
