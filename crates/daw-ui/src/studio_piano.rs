// Native, scale-aware MIDI note editor. Original drawing/editing implementation.
const PIANO_NAMES: [&str; 12] = [
    "C", "C♯", "D", "D♯", "E", "F", "F♯", "G", "G♯", "A", "A♯", "B",
];
const PIANO_ROW_H: f32 = 18.0;
const PIANO_RULER_H: f32 = 22.0;
const PIANO_KEY_W: f32 = 58.0;
const PIANO_PX_PER_BEAT: f32 = 92.0;

fn snap_piano_tick(tick: i64, quantum: i64) -> i64 {
    let step = quantum.max(1);
    ((tick.max(0) + step / 2) / step) * step
}

/// Map a pointer Y inside the velocity lane to a bounded MIDI velocity.
/// Releasing outside the panel still yields 1..=127 rather than 0 or 255.
fn velocity_from_lane_y(y: f32, top: f32, bottom: f32) -> u8 {
    if !y.is_finite() || bottom <= top {
        return 100;
    }
    let normalized = ((bottom - y) / (bottom - top)).clamp(0.0, 1.0);
    (normalized * 127.0).round().clamp(1.0, 127.0) as u8
}

/// Identify the exact note index owning the velocity bar nearest the pointer.
/// Notes share the lane's start-position mapping, so this never edits a
/// different note than the bar that was grabbed.
fn velocity_note_at_x(
    notes: &[MidiNote],
    x: f32,
    left: f32,
    right: f32,
    duration_ticks: f32,
    tolerance_px: f32,
    preferred: Option<usize>,
) -> Option<usize> {
    let span = (right - left).max(1.0);
    let duration = duration_ticks.max(1.0);
    notes.iter().enumerate()
        .filter(|(_, note)| !note.muted)
        .map(|(index, note)| {
            let bar_x = left + (note.start.ticks() as f32 / duration).clamp(0.0, 1.0) * span;
            (index, (bar_x - x).abs())
        })
        .filter(|(_, distance)| *distance <= tolerance_px.max(1.0))
        .min_by(|a, b| a.1.total_cmp(&b.1).then_with(||
            (Some(a.0) != preferred).cmp(&(Some(b.0) != preferred))))
        .map(|(index, _)| index)
}

// Center the first view on the selected clip's middle pitch, so expanding
// the editor reveals neighboring chord tones instead of an arbitrary octave.
fn initial_piano_scroll_offset(rows: &[u8], pitches: &[u8], viewport_height: f32) -> f32 {
    let mut sorted = pitches.iter().copied().filter(|p| *p <= 127).collect::<Vec<_>>();
    sorted.sort_unstable();
    let center_pitch = sorted.get(sorted.len() / 2).copied().unwrap_or(60);
    let center_row = rows.iter().position(|pitch| *pitch == center_pitch)
        .or_else(|| rows.iter().enumerate().min_by_key(|(_, pitch)|
            pitch.abs_diff(center_pitch)).map(|(index, _)| index))
        .unwrap_or(0);
    (PIANO_RULER_H + (center_row as f32 + 0.5) * PIANO_ROW_H
        - viewport_height * 0.5).max(0.0)
}

const VELOCITY_LANE_H: f32 = 32.0;

enum PianoEdit {
    Add(u8, i64),
    Delete(usize),
    Move(usize, u8, i64),
    Resize(usize, i64),
    Velocity(usize, u8),
    Duplicate(usize),
}

impl Studio {
    fn piano_editor(&mut self, ui: &mut egui::Ui, clip: &Clip) {
        let ClipKind::Midi(midi) = &clip.kind else { return };
        let mut scale = clip.scale_override.or(self.project.global_scale)
            .unwrap_or(ScaleDefinition::new(PitchClass::C, ScaleMode::Major));
        let mut root = scale.root.semitone();
        let mut mode = scale.mode;
        let mut changed_scale = false;
        let mut action: Option<PianoEdit> = None;
        let notes = midi.notes.clone();

        ui.horizontal_wrapped(|ui| {
            ui.label(egui::RichText::new("▥  PIANO ROLL")
                .strong().size(12.0).color(ACCENT));
            ui.separator();
            ui.label(egui::RichText::new("SCALE").size(10.0).weak());
            egui::ComboBox::from_id_salt("piano_root")
                .selected_text(PIANO_NAMES[root as usize])
                .width(46.0).show_ui(ui, |ui| {
                    for (i, name) in PIANO_NAMES.iter().enumerate() {
                        if ui.selectable_value(&mut root, i as u8, *name).changed() {
                            changed_scale = true;
                        }
                    }
                });
            egui::ComboBox::from_id_salt("piano_mode")
                .selected_text(format!("{mode:?}"))
                .width(116.0).show_ui(ui, |ui| {
                    for value in ScaleMode::ALL {
                        if ui.selectable_value(&mut mode, value, format!("{value:?}"))
                            .changed() { changed_scale = true; }
                    }
                });
            ui.separator();
            ui.toggle_value(&mut self.scale_highlight, "◈ Highlight");
            ui.toggle_value(&mut self.fold, "Fold");
            ui.toggle_value(&mut self.scale_lock, "♮ In key");
            ui.separator();
            ui.label("Grid");
            egui::ComboBox::from_id_salt("piano_grid").selected_text(self.piano_grid.label())
                .width(55.0).show_ui(ui, |ui| {
                    for grid in [PianoGrid::Quarter, PianoGrid::Eighth,
                        PianoGrid::Sixteenth, PianoGrid::ThirtySecond] {
                        ui.selectable_value(&mut self.piano_grid, grid, grid.label());
                    }
                });
            ui.label("Length");
            egui::ComboBox::from_id_salt("piano_length")
                .selected_text(self.piano_note_length.label()).width(55.0)
                .show_ui(ui, |ui| {
                    for grid in [PianoGrid::Quarter, PianoGrid::Eighth,
                        PianoGrid::Sixteenth, PianoGrid::ThirtySecond] {
                        ui.selectable_value(&mut self.piano_note_length, grid, grid.label());
                    }
                });
        });

        if changed_scale {
            scale = ScaleDefinition::new(PitchClass::new(root).unwrap_or(PitchClass::C), mode);
            if let Some(id) = self.selected_clip
                && let Some(target) = self.project.tracks.iter_mut()
                    .flat_map(|track| track.all_clips_mut()).find(|c| c.id == id) {
                    target.scale_override = Some(scale);
                    self.dirty = true;
                }
        }
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Click empty space to draw  ·  Drag notes to move  ·  Drag right edge to resize  ·  Right-click to delete")
                .size(10.0).color(Color32::from_gray(144)));
            if let Some(index) = self.piano_selected.filter(|index| *index < notes.len()) {
                ui.separator();
                if ui.small_button("Duplicate").clicked() { action = Some(PianoEdit::Duplicate(index)); }
                if ui.small_button("Delete").clicked() { action = Some(PianoEdit::Delete(index)); }
                let mut velocity = notes[index].velocity;
                if ui.add(egui::Slider::new(&mut velocity, 1..=127)
                    .text("Velocity").show_value(true)).changed() {
                    action = Some(PianoEdit::Velocity(index, velocity));
                }
            }
        });
        // Show all 128 MIDI pitches instead of clipping instruments and chords
        // outside C2–C7. Fold still filters rows to the current scale.
        let rows = (0_u8..=127_u8).rev()
            .filter(|&note| !self.fold || scale.contains_midi_note(note))
            .collect::<Vec<_>>();
        let hidden = notes.iter().filter(|note| !rows.contains(&note.pitch)).count();
        if hidden > 0 {
            ui.label(egui::RichText::new(format!(
                "{hidden} notes outside the current fold or MIDI range are hidden, not deleted."
            )).size(10.0).color(ACCENT_BRIGHT));
        }
        let bar_ticks = self.bar_ticks().max(1);
        let bar_beats = bar_ticks as f32 / PPQ as f32;
        let bars = ((clip.duration.ticks() as f32 / bar_ticks as f32).ceil() as usize).clamp(8,128);
        let steps = (bars as f32 * bar_beats).ceil() as usize;
        let width = PIANO_KEY_W + steps as f32 * PIANO_PX_PER_BEAT;
        let height = PIANO_RULER_H + rows.len() as f32 * PIANO_ROW_H + 4.0;
        let quantum = self.piano_grid.ticks();
        let note_length = self.piano_note_length.ticks();

        let viewport_height = (ui.available_height() - 66.0).max(58.0);
        let scroll_area = egui::ScrollArea::both()
            .id_salt("piano_scroll")
            // A note drag should move the note, never the scroll viewport.
            .scroll_source(egui::scroll_area::ScrollSource::SCROLL_BAR |
                egui::scroll_area::ScrollSource::MOUSE_WHEEL)
            .auto_shrink([false, false])
            .min_scrolled_height(58.0)
            .max_height(viewport_height);
        let scroll_area = if self.piano_scroll_clip != Some(clip.id) {
            self.piano_scroll_clip = Some(clip.id);
            let pitches = notes.iter().map(|note| note.pitch).collect::<Vec<_>>();
            scroll_area.vertical_scroll_offset(initial_piano_scroll_offset(
                &rows, &pitches, viewport_height))
        } else { scroll_area };
        scroll_area.show(ui, |ui| {
                let (rect, canvas_response) = ui.allocate_exact_size(
                    Vec2::new(width, height), Sense::click()
                );
                let painter = ui.painter_at(rect);
                painter.rect_filled(rect, 0.0, INK_0);
                painter.rect_filled(
                    Rect::from_min_size(rect.min, Vec2::new(width, PIANO_RULER_H)),
                    0.0, INK_2,
                );
                painter.text(
                    rect.min + Vec2::new(9.0, 5.0), egui::Align2::LEFT_TOP,
                    "NOTE", FontId::proportional(10.0), STUDIO_DIM,
                );
                for (i, &pitch) in rows.iter().enumerate() {
                    let y = rect.top() + PIANO_RULER_H + i as f32 * PIANO_ROW_H;
                    let key = (pitch % 12) as usize;
                    let is_black = matches!(key, 1 | 3 | 6 | 8 | 10);
                    let in_key = scale.contains_midi_note(pitch);
                    let root_note = (pitch % 12) == scale.root.semitone();
                    let row_color = if self.scale_highlight && root_note {
                        Color32::from_rgb(54, 31, 24)
                    } else if self.scale_highlight && in_key {
                        Color32::from_rgb(31, 28, 26)
                    } else if is_black {
                        INK_1
                    } else {
                        INK_2
                    };
                    let row_rect = Rect::from_min_size(
                        Pos2::new(rect.left() + PIANO_KEY_W, y),
                        Vec2::new(width - PIANO_KEY_W, PIANO_ROW_H - 1.0)
                    );
                    painter.rect_filled(row_rect, 0.0, row_color);
                    let key_rect = Rect::from_min_size(Pos2::new(rect.left(), y),
                        Vec2::new(PIANO_KEY_W - 1.0, PIANO_ROW_H - 1.0));
                    painter.rect_filled(key_rect, 2.0,
                        if root_note && self.scale_highlight {
                            Color32::from_rgb(182, 88, 36)
                        } else if is_black {
                            Color32::from_rgb(41, 42, 47)
                        } else {
                            Color32::from_rgb(196, 198, 202)
                        });
                    let label = format!("{}{}", PIANO_NAMES[key], pitch / 12 - 1);
                    painter.text(Pos2::new(rect.left() + 9.0, y + 2.0),
                        egui::Align2::LEFT_TOP, label, FontId::monospace(11.0),
                        if (root_note && self.scale_highlight) || is_black {
                            STUDIO_TEXT
                        } else { Color32::from_rgb(51, 52, 58) });
                    painter.line_segment([
                        Pos2::new(rect.left(), y + PIANO_ROW_H - 1.0),
                        Pos2::new(rect.right(), y + PIANO_ROW_H - 1.0)
                    ], Stroke::new(0.4_f32, Color32::from_rgb(35, 36, 42)));
                }
                let cells_per_beat = PPQ / quantum;
                let num_cells = steps as i64 * cells_per_beat;
                for cell in 0..=num_cells {
                    let x = rect.left() + PIANO_KEY_W +
                        cell as f32 * PIANO_PX_PER_BEAT / cells_per_beat as f32;
                    let is_beat = cell % cells_per_beat == 0;
                    painter.line_segment([
                        Pos2::new(x, rect.top() + PIANO_RULER_H),
                        Pos2::new(x, rect.bottom())
                    ], Stroke::new(0.55_f32,
                        if is_beat { Color32::from_rgb(53, 48, 46) }
                        else { Color32::from_rgb(32, 32, 36) }));
                }
                for bar in 0..=bars {
                    let x = rect.left() + PIANO_KEY_W +
                        bar as f32 * bar_beats * PIANO_PX_PER_BEAT;
                    painter.line_segment([Pos2::new(x, rect.top() + PIANO_RULER_H),
                        Pos2::new(x, rect.bottom())],
                        Stroke::new(1.35_f32, Color32::from_rgb(90, 67, 57)));
                }

                for (index, note) in notes.iter().enumerate() {
                    let Some(row) = rows.iter().position(|&p| p == note.pitch) else { continue };
                    let x = rect.left() + PIANO_KEY_W +
                        note.start.ticks() as f32 / PPQ as f32 * PIANO_PX_PER_BEAT;
                    let y = rect.top() + PIANO_RULER_H + row as f32 * PIANO_ROW_H + 1.0;
                    let note_w = (note.duration.ticks() as f32 / PPQ as f32 * PIANO_PX_PER_BEAT)
                        .max(6.0);
                    let note_rect = Rect::from_min_size(Pos2::new(x, y),
                        Vec2::new(note_w, PIANO_ROW_H - 3.0));
                    let response = ui.interact(note_rect, egui::Id::new(
                        ("piano_note", clip.id, index)), Sense::click_and_drag());
                    let hovered = response.hovered();
                    if response.drag_started()
                        && let Some(pos) = response.interact_pointer_pos() {
                            self.piano_resize_note = if pos.x >= note_rect.right() - 9.0 {
                                Some(index)
                            } else { None };
                        }
                    let resize = self.piano_resize_note == Some(index);
                    let delta = response.drag_delta();
                    let dt = snap_piano_tick(
                        (delta.x / PIANO_PX_PER_BEAT * PPQ as f32).abs() as i64,
                        quantum
                    ) * if delta.x < 0.0 { -1 } else { 1 };
                    let moved_row = (row as i32 + (delta.y / PIANO_ROW_H).round() as i32)
                        .clamp(0, rows.len() as i32 - 1) as usize;
                    let preview_start = (note.start.ticks() + dt).max(0);
                    let preview_duration = (note.duration.ticks() + dt).max(quantum);
                    let preview_pitch = rows[moved_row];
                    let preview_rect = if response.dragged() && resize {
                        Rect::from_min_size(note_rect.min,
                            Vec2::new(preview_duration as f32 / PPQ as f32 *
                                PIANO_PX_PER_BEAT, note_rect.height()))
                    } else if response.dragged() {
                        Rect::from_min_size(Pos2::new(
                            rect.left() + PIANO_KEY_W + preview_start as f32 /
                                PPQ as f32 * PIANO_PX_PER_BEAT,
                            rect.top() + PIANO_RULER_H + moved_row as f32 * PIANO_ROW_H + 1.0),
                            note_rect.size())
                    } else { note_rect };
                    let selected = self.piano_selected == Some(index);
                    painter.rect_filled(preview_rect, 4.0, if note.muted {
                        Color32::from_gray(71)
                    } else if selected || hovered {
                        ACCENT_BRIGHT
                    } else {
                        ACCENT
                    });
                    painter.rect_stroke(preview_rect, 4.0,
                        Stroke::new(1.0_f32, if selected {
                            STUDIO_TEXT
                        } else { ACCENT_DARK }),
                        egui::StrokeKind::Inside);
                    painter.rect_filled(Rect::from_min_size(
                        Pos2::new(preview_rect.right() - 4.0, preview_rect.top() + 2.0),
                        Vec2::new(2.0, (preview_rect.height() - 4.0).max(1.0))
                    ), 1.0, ACCENT_DARK);
                    if response.clicked() { self.piano_selected = Some(index); }
                    if response.secondary_clicked() { action = Some(PianoEdit::Delete(index)); }
                    if response.drag_stopped() {
                        if resize {
                            action = Some(PianoEdit::Resize(index, preview_duration));
                        } else if preview_pitch != note.pitch || preview_start != note.start.ticks() {
                            action = Some(PianoEdit::Move(index, preview_pitch, preview_start));
                        }
                        self.piano_selected = Some(index);
                        self.piano_resize_note = None;
                    }
                    response.on_hover_text(format!(
                        "{} • beat {:.2} • length {:.2} beats • velocity {}",
                        PIANO_NAMES[(note.pitch % 12) as usize],
                        note.start.ticks() as f32 / PPQ as f32 + 1.0,
                        note.duration.ticks() as f32 / PPQ as f32, note.velocity
                    ));
                }

                // Pin the bar ruler to the scroll viewport. A ruler painted
                // only at rect.top() disappears when scrolling vertically.
                let pinned_top = ui.clip_rect().top();
                let pinned = Rect::from_min_size(
                    Pos2::new(rect.left(), pinned_top),
                    Vec2::new(width, PIANO_RULER_H),
                );
                painter.rect_filled(pinned, 0.0, INK_2);
                painter.line_segment([
                    Pos2::new(pinned.left(), pinned.bottom() - 1.0),
                    Pos2::new(pinned.right(), pinned.bottom() - 1.0),
                ], Stroke::new(0.8_f32, STUDIO_BORDER));
                painter.text(
                    pinned.min + Vec2::new(9.0, 5.0),
                    egui::Align2::LEFT_TOP, "NOTE",
                    FontId::proportional(10.0), STUDIO_DIM,
                );
                for bar in 0..=bars {
                    let x = rect.left() + PIANO_KEY_W
                        + bar as f32 * bar_beats * PIANO_PX_PER_BEAT;
                    if x + 28.0 < ui.clip_rect().left() || x > ui.clip_rect().right() {
                        continue;
                    }
                    painter.text(
                        Pos2::new(x + 6.0, pinned_top + 5.0),
                        egui::Align2::LEFT_TOP, format!("{}", bar + 1),
                        FontId::monospace(11.0), STUDIO_TEXT,
                    );
                }

                if canvas_response.clicked()
                    && let Some(pos) = canvas_response.interact_pointer_pos() {
                        let x = pos.x - rect.left() - PIANO_KEY_W;
                        let row = ((pos.y - rect.top() - PIANO_RULER_H) / PIANO_ROW_H).floor() as usize;
                        if x >= 0.0 && pos.y >= pinned_top + PIANO_RULER_H
                            && pos.y >= rect.top() + PIANO_RULER_H
                            && let Some(&pitch) = rows.get(row)
                            && (!self.scale_lock || scale.contains_midi_note(pitch))
                        {
                            let raw = (x / PIANO_PX_PER_BEAT * PPQ as f32).floor() as i64;
                            let tick = (raw.max(0) / quantum) * quantum;
                            action = Some(PianoEdit::Add(pitch, tick));
                        }
                    }
            });
        ui.add_space(4.0);
        let panel_w = ui.available_width().max(240.0);
        let (velocity_rect, _) = ui.allocate_exact_size(
            Vec2::new(panel_w, 52.0), Sense::hover());
        let velocity_painter = ui.painter_at(velocity_rect);
        velocity_painter.rect_filled(velocity_rect, 6.0, INK_2);
        velocity_painter.text(
            velocity_rect.min + Vec2::new(10.0, 8.0),
            egui::Align2::LEFT_TOP, "VELOCITY",
            FontId::proportional(10.0), STUDIO_DIM);
        let left = velocity_rect.left() + 75.0;
        let right = velocity_rect.right() - 11.0;
        let bottom = velocity_rect.bottom() - 7.0;
        let lane_top = bottom - VELOCITY_LANE_H;
        let duration = clip.duration.ticks().max(4 * PPQ) as f32;
        let lane_response = ui.interact(velocity_rect,
            egui::Id::new(("piano_velocity_lane", clip.id)), Sense::click_and_drag())
            .on_hover_text("Drag a velocity bar up or down to change that note's velocity (1–127). Release outside the panel is clamped safely.");
        let lane_pointer = lane_response.interact_pointer_pos();
        if lane_response.drag_started()
            && let Some(pos) = lane_pointer {
            // egui reports the moved pointer after crossing the drag
            // threshold. Pick the bar at press position, not the new X.
            let press_x = pos.x - lane_response.total_drag_delta()
                .map_or(0.0, |delta| delta.x);
            self.piano_velocity_drag = velocity_note_at_x(
                &notes, press_x, left, right, duration, 10.0, self.piano_selected);
            if let Some(index) = self.piano_velocity_drag {
                self.piano_selected = Some(index);
                self.piano_velocity_drag_value = Some(velocity_from_lane_y(
                    pos.y, lane_top, bottom,
                ));
            }
        }
        if lane_response.clicked()
            && let Some(pos) = lane_pointer
            && let Some(index) = velocity_note_at_x(
                &notes, pos.x, left, right, duration, 10.0, self.piano_selected,
            )
        {
            self.piano_selected = Some(index);
            action = Some(PianoEdit::Velocity(
                index, velocity_from_lane_y(pos.y, lane_top, bottom)));
        }
        if lane_response.dragged()
            && self.piano_velocity_drag.is_some()
            && let Some(pos) = lane_pointer
        {
            self.piano_velocity_drag_value = Some(velocity_from_lane_y(
                pos.y, lane_top, bottom,
            ));
        }
        if lane_response.drag_stopped()
            && let Some(index) = self.piano_velocity_drag.take()
            && let Some(velocity) = self.piano_velocity_drag_value.take()
            && index < notes.len()
        {
            self.piano_selected = Some(index);
            action = Some(PianoEdit::Velocity(index, velocity));
        }
        let velocity_preview = self.piano_velocity_drag
            .zip(self.piano_velocity_drag_value);
        for (index, note) in notes.iter().enumerate() {
            if note.muted { continue; }
            let x = left + ((note.start.ticks() as f32 / duration).clamp(0.0, 1.0))
                * (right - left);
            let velocity = velocity_preview
                .filter(|(dragged, _)| *dragged == index)
                .map_or(note.velocity, |(_, value)| value);
            let height = (velocity as f32 / 127.0) * VELOCITY_LANE_H;
            let active = self.piano_selected == Some(index)
                || self.piano_velocity_drag == Some(index);
            velocity_painter.line_segment([
                Pos2::new(x, bottom), Pos2::new(x, bottom - height)
            ], Stroke::new(if active { 3.5_f32 } else { 2.5_f32 },
                if active { STUDIO_TEXT } else { ACCENT }));
        }
        if self.piano_velocity_drag.is_some()
            && let Some((_, value)) = velocity_preview {
            velocity_painter.text(
                Pos2::new(velocity_rect.left() + 10.0, velocity_rect.top() + 25.0),
                egui::Align2::LEFT_TOP, format!("{value}"),
                FontId::monospace(11.0), ACCENT_BRIGHT);
        }
        if !ui.ctx().wants_keyboard_input()
            && ui.input(|input| input.key_pressed(egui::Key::Delete))
            && let Some(index) = self.piano_selected.filter(|index| *index < notes.len()) {
            action = Some(PianoEdit::Delete(index));
        }
        if let Some(action) = action {
            let Some(target) = self.project.tracks.iter_mut()
                .flat_map(|track| track.all_clips_mut())
                .find(|c| c.id == clip.id) else { return };
            let ClipKind::Midi(edited) = &mut target.kind else { return };
            match action {
                PianoEdit::Add(pitch, tick) => {
                    if !edited.notes.iter().any(|n| n.pitch == pitch && n.start.ticks() == tick) {
                        edited.notes.push(MidiNote {
                            pitch, start: MusicalTime::from_ticks(tick),
                            duration: MusicalTime::from_ticks(note_length),
                            velocity: 100, muted: false,
                        });
                        self.piano_selected = Some(edited.notes.len() - 1);
                    }
                }
                PianoEdit::Delete(index) => {
                    if index < edited.notes.len() { edited.notes.remove(index); }
                    self.piano_selected = None;
                }
                PianoEdit::Move(index, pitch, start) => {
                    if let Some(note) = edited.notes.get_mut(index) {
                        note.pitch = pitch;
                        note.start = MusicalTime::from_ticks(start);
                    }
                }
                PianoEdit::Resize(index, duration) => {
                    if let Some(note) = edited.notes.get_mut(index) {
                        note.duration = MusicalTime::from_ticks(duration.max(quantum));
                    }
                }
                PianoEdit::Velocity(index, velocity) => {
                    if let Some(note) = edited.notes.get_mut(index) {
                        note.velocity = velocity;
                    }
                }
                PianoEdit::Duplicate(index) => {
                    if let Some(mut note) = edited.notes.get(index).copied() {
                        note.start = MusicalTime::from_ticks(note.start.ticks() + note.duration.ticks());
                        edited.notes.push(note);
                        self.piano_selected = Some(edited.notes.len() - 1);
                    }
                }
            }
            let end = edited.notes.iter().map(|n| n.start.ticks()
                .saturating_add(n.duration.ticks())).max().unwrap_or(0);
            if end > target.duration.ticks() { target.duration = MusicalTime::from_ticks(end); }
            self.dirty = true;
            self.restart_for_edit();
        }
    }
}

#[cfg(test)]
mod piano_view_tests {
    use super::*;

    #[test]
    fn initial_scroll_centers_middle_note_across_full_midi_range() {
        let rows = (0..=127).rev().collect::<Vec<u8>>();
        let viewport = 300.0;
        for pitch in [0, 24, 60, 84, 108, 127] {
            let offset = initial_piano_scroll_offset(&rows, &[pitch], viewport);
            let note_y = PIANO_RULER_H
                + (rows.iter().position(|p| *p == pitch).unwrap() as f32 + 0.5)
                    * PIANO_ROW_H;
            assert!(note_y >= offset && note_y - offset <= viewport,
                "Pitch {pitch} should be visible after initial scroll");
        }
        assert!(initial_piano_scroll_offset(&rows, &[48, 60, 72], viewport)
            > initial_piano_scroll_offset(&rows, &[84], viewport));
    }

    #[test]
    fn note_editor_background_drag_does_not_pan_viewport() {
        use egui::{Event, PointerButton, RawInput};
        let ctx = egui::Context::default();
        let frame = |events: Vec<Event>| {
            let mut offset = 0.0;
            let _ = ctx.run(RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(560.0, 340.0))),
                events,
                ..Default::default()
            }, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let output = egui::ScrollArea::both()
                        .id_salt("no_editor_drift")
                        .scroll_source(egui::scroll_area::ScrollSource::SCROLL_BAR |
                            egui::scroll_area::ScrollSource::MOUSE_WHEEL)
                        .show(ui, |ui| {
                            ui.allocate_exact_size(Vec2::new(4000.0, 800.0), Sense::hover());
                        });
                    offset = output.state.offset.x;
                });
            });
            offset
        };
        frame(vec![]);
        let start = Pos2::new(290.0, 130.0);
        let end = Pos2::new(75.0, 130.0);
        frame(vec![Event::PointerMoved(start)]);
        frame(vec![Event::PointerButton {
            pos: start, button: PointerButton::Primary,
            pressed: true, modifiers: Default::default(),
        }]);
        frame(vec![Event::PointerMoved(end)]);
        frame(vec![Event::PointerButton {
            pos: end, button: PointerButton::Primary,
            pressed: false, modifiers: Default::default(),
        }]);
        let offset = frame(vec![]);
        assert_eq!(offset, 0.0, "Background drag unexpectedly scrolled editor to {offset}");
    }

    fn velocity_note(start: i64, velocity: u8, muted: bool) -> MidiNote {
        MidiNote {
            pitch: 60, velocity, muted,
            start: MusicalTime::from_ticks(start),
            duration: MusicalTime::from_ticks(PPQ),
        }
    }

    #[test]
    fn velocity_lane_maps_pointer_to_bounded_1_127() {
        let top = 100.0;
        let bottom = 132.0;
        assert_eq!(velocity_from_lane_y(132.0, top, bottom), 1);
        assert_eq!(velocity_from_lane_y(100.0, top, bottom), 127);
        assert_eq!(velocity_from_lane_y(180.0, top, bottom), 1,
            "release below the panel must not yield zero");
        assert_eq!(velocity_from_lane_y(-40.0, top, bottom), 127);
        assert_eq!(velocity_from_lane_y(f32::NAN, top, bottom), 100);
        let middle = velocity_from_lane_y(116.0, top, bottom);
        assert!((62..=66).contains(&middle), "middle should be ~64, got {middle}");
    }

    #[test]
    fn velocity_lane_picks_the_exact_nearest_note_and_ignores_muted() {
        let notes = [
            velocity_note(0, 100, false),
            velocity_note(2 * PPQ, 80, false),
            velocity_note(4 * PPQ, 60, false),
        ];
        let left = 0.0;
        let right = 480.0;
        let duration = 8.0 * PPQ as f32;
        let x_for = |tick: i64| left + tick as f32 / duration * (right - left);
        assert_eq!(velocity_note_at_x(&notes, x_for(0), left, right, duration, 10.0, None), Some(0));
        assert_eq!(velocity_note_at_x(&notes, x_for(2 * PPQ), left, right, duration, 10.0, None), Some(1));
        assert_eq!(velocity_note_at_x(&notes, x_for(4 * PPQ), left, right, duration, 10.0, None), Some(2));
        assert_eq!(velocity_note_at_x(&notes, x_for(PPQ), left, right, duration, 10.0, None), None,
            "a pointer between bars must not edit an unrelated note");
        let muted = [velocity_note(0, 100, true), velocity_note(2 * PPQ, 80, false)];
        assert_eq!(velocity_note_at_x(&muted, x_for(0), left, right, duration, 10.0, None), None);
        // Chord notes begin on the same tick, so their bars overlap. The
        // selected piano-roll note wins that tie instead of always note zero.
        let chord = [velocity_note(0, 80, false), velocity_note(0, 110, false)];
        assert_eq!(velocity_note_at_x(&chord, x_for(0), left, right,
            duration, 10.0, Some(1)), Some(1));
        assert_eq!(velocity_note_at_x(&chord, x_for(0), left, right,
            duration, 10.0, None), Some(0));
    }

    #[test]
    fn resizable_bottom_panel_keeps_dragged_height_after_release() {
        use egui::{Event, PointerButton, RawInput};
        let ctx = egui::Context::default();
        let frame = |events: Vec<Event>| {
            let mut boundary = 0.0;
            let _ = ctx.run(RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1000.0, 900.0))),
                events,
                ..Default::default()
            }, |ctx| {
                egui::TopBottomPanel::bottom("resize_regression")
                    .resizable(true)
                    .default_height(300.0)
                    .min_height(280.0)
                    .show(ctx, |ui| {
                        // The same layout contract used in detail_panel.
                        ui.set_min_height(ui.available_height());
                        ui.label("Piano roll");
                    });
                boundary = ctx.available_rect().max.y;
            });
            boundary
        };
        let original_boundary = frame(vec![]);
        let start = Pos2::new(500.0, original_boundary);
        frame(vec![Event::PointerMoved(start)]);
        frame(vec![Event::PointerButton {
            pos: start, button: PointerButton::Primary,
            pressed: true, modifiers: Default::default(),
        }]);
        let end = Pos2::new(500.0, original_boundary - 180.0);
        frame(vec![Event::PointerMoved(end)]);
        frame(vec![Event::PointerButton {
            pos: end, button: PointerButton::Primary,
            pressed: false, modifiers: Default::default(),
        }]);
        let after_release = frame(vec![]);
        assert!(after_release < original_boundary - 130.0,
            "Panel rubber-banded after drag: original={original_boundary} final={after_release}");
    }
}
