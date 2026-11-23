// LNF-1 native device panel. Its visual language is intentionally separate
// from the Midnight Ember DAW chrome: a light industrial chassis, black OLED
// display, and four positional control colours. Parameters are applied by
// rebuilding only after the drag ends, keeping the audio callback lock-free.

const LNF_CHASSIS: Color32 = Color32::from_rgb(228, 228, 224);
const LNF_EDGE: Color32 = Color32::from_rgb(207, 207, 202);
const LNF_KEY: Color32 = Color32::from_rgb(245, 245, 242);
const LNF_KEY_DARK: Color32 = Color32::from_rgb(42, 42, 44);
const LNF_INK: Color32 = Color32::from_rgb(20, 20, 20);
const LNF_DIM: Color32 = Color32::from_rgb(138, 138, 133);
const LNF_BLUE: Color32 = Color32::from_rgb(47, 107, 255);
const LNF_GREEN: Color32 = Color32::from_rgb(25, 178, 107);
const LNF_WHITE: Color32 = Color32::from_rgb(242, 242, 238);
const LNF_ORANGE: Color32 = Color32::from_rgb(255, 90, 31);

impl Studio {
    fn draw_lnf1_device(&mut self, ui: &mut egui::Ui) {
        let selected = self.selected_track.and_then(|id| {
            self.project
                .tracks
                .iter()
                .find(|track| track.id == id && track.kind == TrackKind::Midi)
                .map(|track| track.id)
        });
        let Some(track_id) = selected else {
            ui.weak("Select a MIDI track to place the LNF-1 piano device.");
            return;
        };
        let assigned = self
            .project
            .tracks
            .iter()
            .find(|track| track.id == track_id)
            .and_then(|track| track.instrument.as_ref())
            .is_some_and(|instrument| instrument.bank == LNF1_INSTRUMENT_BANK);
        if !assigned {
            ui.label(
                egui::RichText::new("LNF-1 · PIANO DEVICE")
                    .size(12.0)
                    .color(LNF_INK),
            );
            ui.weak("Original two-mode piano engine. No GPL source or sample data is bundled.");
            if ui.button("Load LNF-1 on selected MIDI track").clicked() {
                if let Some(track) = self.project.track_mut(track_id) {
                    track.instrument = Some(BuiltinInstrument::lnf1(
                        *daw_engine::Lnf1Params::default().values(),
                    ));
                }
                self.lnf1_handles.entry(track_id).or_insert_with(|| {
                    daw_engine::Lnf1ParamStore::new(daw_engine::Lnf1Params::default())
                });
                self.soundfont_rebuild_pending = true;
                self.dirty = true;
                self.status =
                    "LNF-1 loaded — start playback or use the keyboard to audition".into();
            }
            ui.separator();
            ui.weak("The existing Instrument tab remains available for SoundFont tracks.");
            return;
        }

        let mut params = self
            .project
            .tracks
            .iter()
            .find(|track| track.id == track_id)
            .and_then(|track| track.instrument.as_ref())
            .and_then(|instrument| instrument.lnf1_params)
            .map(daw_engine::Lnf1Params::from_values)
            .unwrap_or_default();
        let mut changed = false;
        let electric = params.get(daw_engine::Lnf1ParamId::Instrument) >= 0.5;
        egui::Frame::new()
            .fill(LNF_CHASSIS)
            .stroke(Stroke::new(1.0_f32, LNF_EDGE))
            .corner_radius(egui::CornerRadius::same(6))
            .inner_margin(egui::Margin::same(12))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("lnf-1  piano")
                            .monospace()
                            .size(13.0)
                            .color(LNF_INK),
                    );
                    ui.add_space(8.0);
                    ui.weak("native instrument · 32 voices");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            egui::RichText::new(format!("{}k", self.sample_rate / 1000))
                                .monospace()
                                .size(10.0)
                                .color(LNF_DIM),
                        );
                    });
                });
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    let mut acoustic = !electric;
                    if ui.selectable_label(acoustic, "A · acoustic").clicked() {
                        acoustic = true;
                        params.set(daw_engine::Lnf1ParamId::Instrument, 0.0);
                        changed = true;
                    }
                    if ui.selectable_label(!acoustic, "E · electric").clicked() {
                        params.set(daw_engine::Lnf1ParamId::Instrument, 1.0);
                        changed = true;
                    }
                    ui.add_space(14.0);
                    for page in daw_engine::Lnf1Page::ALL {
                        let label = match page {
                            daw_engine::Lnf1Page::Tune => "1 tune",
                            daw_engine::Lnf1Page::Envelope => "2 env",
                            daw_engine::Lnf1Page::Tone => "3 tone",
                            daw_engine::Lnf1Page::Fx => "4 fx",
                            daw_engine::Lnf1Page::Room => "5 room",
                        };
                        if ui.selectable_label(self.lnf1_page == page, label).clicked() {
                            self.lnf1_page = page;
                        }
                    }
                });
                ui.add_space(6.0);
                draw_lnf1_display(ui, &params, self.lnf1_page);
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    let colours = [LNF_BLUE, LNF_GREEN, LNF_WHITE, LNF_ORANGE];
                    for (slot, param) in self
                        .lnf1_page
                        .controls(params.get(daw_engine::Lnf1ParamId::Instrument) >= 0.5)
                        .into_iter()
                        .enumerate()
                    {
                        ui.vertical_centered(|ui| {
                            if let Some(id) = param {
                                let spec = daw_engine::PARAM_SPECS[id.index()];
                                let mut value = params.get(id);
                                if draw_lnf1_encoder(
                                    ui,
                                    slot,
                                    spec.name,
                                    &mut value,
                                    spec,
                                    colours[slot],
                                ) {
                                    params.set(id, value);
                                    changed = true;
                                }
                            } else {
                                draw_lnf1_empty_encoder(ui, slot, colours[slot]);
                            }
                        });
                        if slot != 3 {
                            ui.add_space(8.0);
                        }
                    }
                });
                ui.separator();
                self.draw_lnf1_keyboard(ui);
            });
        if changed {
            if let Some(track) = self.project.track_mut(track_id)
                && let Some(instrument) = track.instrument.as_mut()
            {
                instrument.lnf1_params = Some(*params.values());
            }
            self.lnf1_handles
                .entry(track_id)
                .or_insert_with(|| daw_engine::Lnf1ParamStore::new(params))
                .set_params(params);
            self.dirty = true;
        }
    }

    fn draw_lnf1_keyboard(&mut self, ui: &mut egui::Ui) -> bool {
        let whites: [u8; 14] = [0, 2, 4, 5, 7, 9, 11, 12, 14, 16, 17, 19, 21, 23];
        let base = 60 + i32::from(self.keyboard_octave) * 12;
        ui.label(
            egui::RichText::new("keyboard · z/x octave · sustain via MIDI CC64")
                .monospace()
                .size(9.0)
                .color(LNF_DIM),
        );
        let width = ui.available_width().min(620.0);
        let key_width = width / whites.len() as f32;
        let (area, _) = ui.allocate_exact_size(Vec2::new(width, 56.0), Sense::hover());
        let painter = ui.painter_at(area);
        let mut target = None;
        for (index, semitone) in whites.iter().enumerate() {
            let rect = Rect::from_min_size(
                Pos2::new(area.left() + index as f32 * key_width, area.top()),
                Vec2::new(key_width - 2.0, 54.0),
            );
            let note = (base + i32::from(*semitone)).clamp(0, 127) as u8;
            let response = ui.interact(
                rect,
                ui.id().with(("lnf_white", index)),
                Sense::click_and_drag(),
            );
            if response.is_pointer_button_down_on() {
                target = Some(note);
            }
            let held = self
                .keyboard_mouse_note
                .is_some_and(|(_, active)| active == note);
            painter.rect_filled(
                rect,
                3.0,
                if held {
                    LNF_ORANGE.gamma_multiply(0.65)
                } else {
                    LNF_KEY
                },
            );
            painter.rect_stroke(
                rect,
                3.0,
                Stroke::new(1.0_f32, LNF_EDGE),
                egui::StrokeKind::Inside,
            );
            if semitone % 12 == 0 {
                painter.text(
                    rect.center_bottom() - Vec2::new(0.0, 9.0),
                    egui::Align2::CENTER_CENTER,
                    format!(
                        "C{}",
                        4 + self.keyboard_octave + i8::try_from(*semitone / 12).unwrap_or(0)
                    ),
                    FontId::monospace(9.0),
                    LNF_DIM,
                );
            }
        }
        // Deliberately flat black-key row: this preserves the LNF device-grid
        // language instead of drawing a perspective piano keyboard.
        for before_white in [0_usize, 1, 3, 4, 5, 7, 8, 10, 11, 12] {
            let x = area.left() + (before_white + 1) as f32 * key_width - key_width * 0.24;
            let black =
                Rect::from_min_size(Pos2::new(x, area.top()), Vec2::new(key_width * 0.46, 26.0));
            painter.rect_filled(black, 2.0, LNF_KEY_DARK);
            painter.rect_stroke(
                black,
                2.0,
                Stroke::new(1.0_f32, Color32::BLACK),
                egui::StrokeKind::Inside,
            );
        }
        let changed = self.keyboard_mouse_note.map(|(_, note)| note) != target;
        if changed {
            if let Some(previous) = self.keyboard_mouse_note.take() {
                self.stop_audition_note(previous);
            }
            self.keyboard_mouse_note = target.and_then(|note| self.start_audition_note(note));
        }
        changed
    }
}

fn draw_lnf1_display(
    ui: &mut egui::Ui,
    params: &daw_engine::Lnf1Params,
    page: daw_engine::Lnf1Page,
) {
    let width = ui.available_width().min(448.0);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 168.0), Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 4.0, Color32::BLACK);
    let title = match page {
        daw_engine::Lnf1Page::Tune => "tune",
        daw_engine::Lnf1Page::Envelope => "env",
        daw_engine::Lnf1Page::Tone => "tone",
        daw_engine::Lnf1Page::Fx => "fx",
        daw_engine::Lnf1Page::Room => "room",
    };
    let instrument = if params.get(daw_engine::Lnf1ParamId::Instrument) >= 0.5 {
        "E"
    } else {
        "A"
    };
    painter.text(
        rect.left_top() + Vec2::new(10.0, 8.0),
        egui::Align2::LEFT_TOP,
        title,
        FontId::monospace(11.0),
        Color32::WHITE,
    );
    painter.text(
        rect.right_top() + Vec2::new(-10.0, 8.0),
        egui::Align2::RIGHT_TOP,
        format!("{}  01 grand", instrument),
        FontId::monospace(10.0),
        Color32::WHITE,
    );
    let center = rect.center() + Vec2::new(0.0, 10.0);
    match page {
        daw_engine::Lnf1Page::Tune => {
            painter.line_segment(
                [
                    Pos2::new(rect.left() + 18.0, center.y),
                    Pos2::new(rect.right() - 18.0, center.y),
                ],
                Stroke::new(1.0_f32, Color32::DARK_GRAY),
            );
            let stretch = params.get(daw_engine::Lnf1ParamId::StretchTuning) / 50.0;
            let fine = params.get(daw_engine::Lnf1ParamId::FineTuning) / 50.0;
            let mut last = Pos2::new(rect.left() + 18.0, center.y - fine * 26.0);
            for step in 1..64 {
                let x = rect.left() + 18.0 + step as f32 * (rect.width() - 36.0) / 63.0;
                let fold = ((step as f32 / 63.0) - 0.48).max(0.0);
                let point = Pos2::new(x, center.y - fine * 26.0 - stretch * fold * fold * 34.0);
                painter.line_segment([last, point], Stroke::new(1.0_f32, Color32::WHITE));
                last = point;
            }
        }
        daw_engine::Lnf1Page::Envelope => {
            let start = Pos2::new(rect.left() + 44.0, rect.bottom() - 38.0);
            for (offset, shade) in [(0.0, 255), (11.0, 175), (22.0, 95)] {
                let end = Pos2::new(rect.right() - 36.0, rect.top() + 48.0 + offset);
                painter.line_segment(
                    [start + Vec2::new(0.0, -offset), end],
                    Stroke::new(1.0_f32, Color32::from_gray(shade)),
                );
            }
            painter.line_segment(
                [
                    Pos2::new(center.x, rect.top() + 34.0),
                    Pos2::new(center.x, rect.bottom() - 28.0),
                ],
                Stroke::new(1.0_f32, Color32::DARK_GRAY),
            );
        }
        daw_engine::Lnf1Page::Tone => {
            painter.circle_stroke(center, 28.0, Stroke::new(1.0_f32, Color32::WHITE));
            painter.line_segment(
                [
                    center + Vec2::new(-68.0, 0.0),
                    center + Vec2::new(68.0, 0.0),
                ],
                Stroke::new(2.0_f32, Color32::WHITE),
            );
            painter.text(
                center + Vec2::new(0.0, 42.0),
                egui::Align2::CENTER_CENTER,
                if instrument == "A" {
                    "hammer / string"
                } else {
                    "tine / tonebar"
                },
                FontId::monospace(9.0),
                Color32::GRAY,
            );
        }
        daw_engine::Lnf1Page::Fx => {
            for x in [0.25, 0.5, 0.75] {
                painter.line_segment(
                    [
                        Pos2::new(rect.left() + rect.width() * x, rect.top() + 42.0),
                        Pos2::new(rect.left() + rect.width() * x, rect.bottom() - 32.0),
                    ],
                    Stroke::new(1.0_f32, Color32::DARK_GRAY),
                );
            }
            painter.line_segment(
                [
                    Pos2::new(rect.left() + 38.0, rect.bottom() - 40.0),
                    Pos2::new(center.x, rect.top() + 48.0),
                ],
                Stroke::new(1.0_f32, Color32::WHITE),
            );
            painter.line_segment(
                [
                    Pos2::new(center.x, rect.top() + 48.0),
                    Pos2::new(rect.right() - 38.0, rect.bottom() - 70.0),
                ],
                Stroke::new(1.0_f32, Color32::WHITE),
            );
        }
        daw_engine::Lnf1Page::Room => {
            let cube = Rect::from_center_size(center, Vec2::new(78.0, 58.0));
            painter.rect_stroke(
                cube,
                0.0,
                Stroke::new(1.0_f32, Color32::WHITE),
                egui::StrokeKind::Inside,
            );
            painter.line_segment(
                [cube.left_top(), cube.left_top() + Vec2::new(18.0, -12.0)],
                Stroke::new(1.0_f32, Color32::WHITE),
            );
            painter.line_segment(
                [cube.right_top(), cube.right_top() + Vec2::new(18.0, -12.0)],
                Stroke::new(1.0_f32, Color32::WHITE),
            );
        }
    }
}

fn draw_lnf1_encoder(
    ui: &mut egui::Ui,
    slot: usize,
    label: &str,
    value: &mut f32,
    spec: daw_engine::Lnf1ParamSpec,
    colour: Color32,
) -> bool {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(88.0, 88.0), Sense::click_and_drag());
    let mut changed = false;
    if response.double_clicked() {
        *value = spec.default;
        changed = true;
    }
    if response.dragged() {
        let fine = ui.input(|input| input.modifiers.shift);
        let ratio = if fine { 0.1 } else { 1.0 };
        *value = (*value
            - ui.input(|input| input.pointer.delta().y) / 200.0 * (spec.max - spec.min) * ratio)
            .clamp(spec.min, spec.max);
        changed = true;
    }
    let painter = ui.painter_at(rect);
    painter.circle_filled(rect.center(), 37.0, colour);
    painter.circle_stroke(rect.center(), 37.0, Stroke::new(1.0_f32, LNF_EDGE));
    let normal = ((*value - spec.min) / (spec.max - spec.min)).clamp(0.0, 1.0);
    let angle = -std::f32::consts::FRAC_PI_2 - std::f32::consts::FRAC_PI_4
        + normal * std::f32::consts::PI * 1.5;
    let direction = Vec2::angled(angle);
    let notch_colour = if colour == LNF_WHITE {
        LNF_INK
    } else {
        Color32::WHITE
    };
    painter.line_segment(
        [
            rect.center() + direction * 7.0,
            rect.center() + direction * 27.0,
        ],
        Stroke::new(3.0_f32, notch_colour),
    );
    ui.label(
        egui::RichText::new(format!("{}  {:.1}", label, *value))
            .monospace()
            .size(9.0)
            .color(LNF_INK),
    );
    let _ = slot;
    changed
}

fn draw_lnf1_empty_encoder(ui: &mut egui::Ui, _slot: usize, colour: Color32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(88.0, 88.0), Sense::hover());
    ui.painter_at(rect)
        .circle_filled(rect.center(), 37.0, colour.gamma_multiply(0.35));
    ui.label(
        egui::RichText::new("—")
            .monospace()
            .size(9.0)
            .color(LNF_DIM),
    );
}

[executed on device: Johnnysstation (1200896c-45a0-4cb2-8dfa-479e25d803a4)]