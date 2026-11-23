#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DeviceTab {
    Instrument,
    Lnf1,
    Effects,
    Plugins,
    Hardware,
}

const BUILTIN_EFFECTS: [BuiltinEffectKind; 4] = [
    BuiltinEffectKind::Eq3,
    BuiltinEffectKind::Eq8,
    BuiltinEffectKind::VocalReverb,
    BuiltinEffectKind::GuitarReverb,
];

fn effect_description(kind: BuiltinEffectKind) -> &'static str {
    match kind {
        BuiltinEffectKind::Eq3 => "Three-band equalizer with low, mid and high gain",
        BuiltinEffectKind::Eq8 => "Eight fixed-frequency bell filters with draggable gains",
        BuiltinEffectKind::VocalReverb => "Vocal ambience with mix, decay and damping",
        BuiltinEffectKind::GuitarReverb => "Guitar ambience with mix, decay and damping",
    }
}

impl Studio {
    fn can_add_effect(&self) -> bool {
        self.recorder.is_none()
            && self.midi_take.is_none()
            && self
                .selected_track
                .and_then(|id| self.project.tracks.iter().find(|track| track.id == id))
                .is_some_and(|track| track.effects.len() < 8)
    }

    fn add_builtin_effect(&mut self, kind: BuiltinEffectKind) {
        if self.can_add_effect()
            && let Some(track) = self
                .selected_track
                .and_then(|id| self.project.track_mut(id))
        {
            track.effects.push(BuiltinEffect::new(kind));
            self.dirty = true;
            self.fx_rebuild_pending = true;
            self.device_tab = DeviceTab::Effects;
            self.show_devices = true;
        }
    }

    fn device_editor_compact(&mut self, ui: &mut egui::Ui) {
        ui.spacing_mut().item_spacing = Vec2::new(6.0, 5.0);
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.device_tab, DeviceTab::Instrument, "Instrument")
                .on_hover_text(
                    "Built-in SoundFont instrument rack: bank, preset, tone and keyboard",
                );
            ui.selectable_value(&mut self.device_tab, DeviceTab::Lnf1, "LNF-1")
                .on_hover_text("Native two-mode piano device; original engine, no bundled samples");
            ui.selectable_value(&mut self.device_tab, DeviceTab::Effects, "FX Chain")
                .on_hover_text("Built-in audio processors assigned to the selected track");
            ui.selectable_value(&mut self.device_tab, DeviceTab::Plugins, "Plug-ins")
                .on_hover_text("Assign a live CLAP/VST3 insert or render the selected clip to WAV");
            ui.selectable_value(&mut self.device_tab, DeviceTab::Hardware, "Audio / MIDI")
                .on_hover_text("Microphone and MIDI interface selection");
        });
        ui.separator();
        if self.device_tab != DeviceTab::Hardware {
            self.device_track_selector(ui);
        }
        egui::ScrollArea::vertical()
            .id_salt("slate_device_scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| match self.device_tab {
                DeviceTab::Instrument => self.instrument_device_editor(ui),
                DeviceTab::Lnf1 => self.draw_lnf1_device(ui),
                DeviceTab::Effects => self.draw_effect_chain(ui),
                DeviceTab::Plugins => self.plugin_device_ui(ui),
                DeviceTab::Hardware => self.draw_hardware_controls(ui),
            });
    }

    fn draw_hardware_controls(&mut self, ui: &mut egui::Ui) {
        let recording_active = self.recorder.is_some();
        ui.label(
            egui::RichText::new("AUDIO RECORDING")
                .size(11.0)
                .color(STUDIO_DIM),
        );

        let audio_tracks = self
            .project
            .tracks
            .iter()
            .filter(|track| track.kind == TrackKind::Audio)
            .map(|track| (track.id, track.name.clone()))
            .collect::<Vec<_>>();
        let active_recording_track = self.recorder.as_ref().map(|recording| recording.track_id);
        let mut target_track = active_recording_track
            .or_else(|| {
                self.selected_track
                    .filter(|id| audio_tracks.iter().any(|(track_id, _)| track_id == id))
            })
            .or_else(|| audio_tracks.first().map(|(id, _)| *id));
        if !recording_active && self.selected_track != target_track {
            self.selected_track = target_track;
            self.selected_clip = None;
        }

        ui.horizontal_wrapped(|ui| {
            ui.label(
                egui::RichText::new("RECORD TO")
                    .size(10.0)
                    .color(STUDIO_DIM),
            );
            ui.add_enabled_ui(!recording_active && !audio_tracks.is_empty(), |ui| {
                egui::ComboBox::from_id_salt("recording_audio_track")
                    .width(265.0)
                    .selected_text(
                        target_track
                            .and_then(|id| {
                                audio_tracks.iter().find(|(track_id, _)| *track_id == id)
                            })
                            .map_or("Choose an Audio track", |(_, name)| name.as_str()),
                    )
                    .show_ui(ui, |ui| {
                        for (id, name) in &audio_tracks {
                            ui.selectable_value(&mut target_track, Some(*id), name);
                        }
                    });
            });
            if ui
                .add_enabled(!recording_active, egui::Button::new("＋ New Audio track"))
                .clicked()
            {
                self.add_audio_track();
                target_track = self.selected_track;
            }
        });
        if !recording_active && self.selected_track != target_track {
            self.selected_track = target_track;
            self.selected_clip = None;
        }

        if !recording_active {
            self.capture_device_cache();
            ui.ctx().request_repaint_after(Duration::from_secs(2));
        }
        let (devices, discovery_error) =
            match self.capture_devices.as_ref().map(|(_, result)| result) {
                Some(Ok(devices)) => (devices.as_slice(), None),
                Some(Err(error)) => (&[][..], Some(error.clone())),
                None => (&[][..], None),
            };
        let (mut input_id, mut input_label, mut channels) = target_track
            .and_then(|id| self.project.tracks.iter().find(|track| track.id == id))
            .map(|track| {
                (
                    track.record_input_id.clone(),
                    track.record_input.clone(),
                    track.record_input_channels,
                )
            })
            .unwrap_or_default();
        let auto_label = "Auto — prefer AIR 192|6, then first physical input";
        let resolved_input =
            select_track_capture_source(input_id.as_deref(), input_label.as_deref(), devices).ok();
        let selected_label = if input_id.is_none() && input_label.is_none() {
            auto_label.to_owned()
        } else if let Some(source) = resolved_input {
            source.label.clone()
        } else {
            input_label
                .clone()
                .unwrap_or_else(|| "Selected input unavailable".into())
        };
        let mut changed = false;
        ui.horizontal_wrapped(|ui| {
            ui.label(egui::RichText::new("INPUT").size(10.0).color(STUDIO_DIM));
            ui.add_enabled_ui(!recording_active && target_track.is_some(), |ui| {
                egui::ComboBox::from_id_salt("mic_input_choice")
                    .width(330.0)
                    .selected_text(&selected_label)
                    .show_ui(ui, |ui| {
                        if ui
                            .selectable_label(
                                input_id.is_none() && input_label.is_none(),
                                auto_label,
                            )
                            .clicked()
                        {
                            input_id = None;
                            input_label = None;
                            changed = true;
                        }
                        for source in devices {
                            let is_selected = input_id.as_deref() == Some(source.id.id());
                            let label = if devices
                                .iter()
                                .filter(|other| other.label == source.label)
                                .nth(1)
                                .is_some()
                            {
                                format!("{} ({})", source.label, source.id.id())
                            } else {
                                source.label.clone()
                            };
                            if ui.selectable_label(is_selected, label).clicked() {
                                input_id = Some(source.id.id().to_owned());
                                input_label = Some(source.label.clone());
                                changed = true;
                            }
                        }
                    })
                    .response
                    .on_hover_text("This input is saved on the selected Audio track");
            });
        });
        let selected_source =
            select_track_capture_source(input_id.as_deref(), input_label.as_deref(), devices).ok();
        let physical_channels = selected_source.map_or(0, |source| source.channels);
        let choices = daw_core::RecordInputChannels::options(physical_channels);
        if changed
            && !channels
                .for_source(physical_channels)
                .supported_by(physical_channels)
            && let Some(&first) = choices.first()
        {
            channels = first;
        }
        ui.horizontal_wrapped(|ui| {
            ui.label(egui::RichText::new("CHANNELS").size(10.0).color(STUDIO_DIM));
            ui.add_enabled_ui(
                !recording_active && target_track.is_some() && !choices.is_empty(),
                |ui| {
                    egui::ComboBox::from_id_salt("mic_input_channels")
                        .width(153.0)
                        .selected_text(channels.for_source(physical_channels).label())
                        .show_ui(ui, |ui| {
                            for &choice in choices {
                                if ui
                                    .selectable_label(
                                        channels.for_source(physical_channels) == choice,
                                        choice.label(),
                                    )
                                    .clicked()
                                {
                                    channels = choice;
                                    changed = true;
                                }
                            }
                        })
                        .response
                        .on_hover_text("Physical channels captured on this audio track");
                },
            );
        });
        let source_count = devices.len();
        let missing_selected_input =
            selected_source.is_none() && (input_id.is_some() || input_label.is_some());
        let unsupported_channels = selected_source.is_some()
            && !channels
                .for_source(physical_channels)
                .supported_by(physical_channels);
        if changed && let Some(track) = target_track.and_then(|id| self.project.track_mut(id)) {
            track.record_input_id.clone_from(&input_id);
            track.record_input.clone_from(&input_label);
            track.record_input_channels = channels;
            self.dirty = true;
            self.status = format!(
                "{} • {}",
                track.name,
                channels.for_source(physical_channels).label()
            );
        }

        if !recording_active && let Some(error) = discovery_error {
            ui.colored_label(
                RECORDING_RED,
                format!("Cannot query PulseAudio recording inputs: {error}"),
            );
        } else if !recording_active && source_count == 0 {
            ui.colored_label(
                RECORDING_RED,
                "No physical PulseAudio recording inputs found. Connect or enable an interface, then return here.",
            );
        } else if !recording_active && missing_selected_input {
            ui.colored_label(
                RECORDING_RED,
                "The saved input is unavailable. Choose another input before recording.",
            );
        } else if !recording_active && unsupported_channels {
            ui.colored_label(RECORDING_RED, "Choose a channel available on this input.");
        } else if target_track.is_none() {
            ui.weak("Create an Audio track to choose its input and record a WAV take.");
        }

        let record_enabled = recording_active
            || (target_track.is_some()
                && source_count != 0
                && !missing_selected_input
                && !unsupported_channels);
        let record_clicked = ui
            .add_enabled(
                record_enabled,
                egui::Button::new(if recording_active {
                    "■ Stop recording and keep take"
                } else {
                    "● Record into selected Audio track"
                }),
            )
            .on_disabled_hover_text("Choose an available input on an Audio track before recording.")
            .clicked();
        let monitor = ui.add_enabled(
            !recording_active,
            egui::Checkbox::new(
                &mut self.input_monitor_enabled,
                "Monitor input during recording",
            ),
        );
        monitor.on_hover_text("Listen through Rusty's mixer and effects during a take. The input meter works with monitoring off.");
        if let Some(recording) = self.recorder.as_ref() {
            ui.colored_label(
                RECORDING_RED,
                recording_elapsed(recording.progress_frames, self.sample_rate),
            );
            ui.small(format!(
                "Alignment estimate: first buffer {:.2} ms after arm • output delay {:.2} ms • take start frame {} • {} dropped",
                recording.alignment.first_buffer_ms(self.sample_rate),
                recording.alignment.output_delay_ms(self.sample_rate),
                recording.start_frame,
                recording.dropped.load(Ordering::Relaxed),
            )).on_hover_text(
                "Estimated from PulseAudio capture timing and the output stream's reported delay. Not an acoustic round-trip measurement.");
        }
        if let Some(take) = &self.midi_take {
            ui.colored_label(
                ACCENT_BRIGHT,
                format!("Recording MIDI · {} completed notes", take.notes.len()),
            );
        }
        if record_clicked {
            self.toggle_recording();
        }
        ui.separator();
        self.midi_device_editor(ui);
    }

    fn device_track_selector(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.label(egui::RichText::new("TRACK").size(10.0).color(STUDIO_DIM));
            egui::ComboBox::from_id_salt("slate_fx_track")
                .width(235.0)
                .selected_text(
                    self.selected_track
                        .and_then(|id| self.project.tracks.iter().find(|track| track.id == id))
                        .map_or("Choose a track", |track| track.name.as_str()),
                )
                .show_ui(ui, |ui| {
                    for track in &self.project.tracks {
                        ui.selectable_value(&mut self.selected_track, Some(track.id), &track.name);
                    }
                })
                .response
                .on_hover_text("Choose the track for live inserts and built-in effects");
        });
    }

    fn draw_effect_chain(&mut self, ui: &mut egui::Ui) {
        let Some(id) = self.selected_track else {
            ui.weak("Select a track to add audio effects.");
            return;
        };
        let Some(track) = self.project.tracks.iter().find(|t| t.id == id) else {
            return;
        };
        let count = track.effects.len();
        if track.kind == TrackKind::Midi
            && track
                .live_clap
                .as_ref()
                .is_some_and(|insert| insert.enabled)
        {
            ui.weak(
                "These effects precede the live instrument. Use a group for effects on its output.",
            );
        }
        ui.horizontal_wrapped(|ui| {
            ui.label(
                egui::RichText::new("ADD DEVICE")
                    .size(10.0)
                    .color(STUDIO_DIM),
            );
            for kind in BUILTIN_EFFECTS {
                if ui
                    .add_enabled(self.can_add_effect(), egui::Button::new(kind.label()))
                    .on_hover_text(effect_description(kind))
                    .on_disabled_hover_text(
                        "Select a track with fewer than eight inserts and finish any active take.",
                    )
                    .clicked()
                {
                    self.add_builtin_effect(kind);
                }
            }
        });
        ui.separator();
        if count == 0 {
            ui.weak("No built-in effects on this track.");
        }
        let mut changed = false;
        let mut operation: Option<(usize, &'static str)> = None;
        let sample_rate = self.sample_rate;
        let editing = self.recorder.is_none() && self.midi_take.is_none();
        if !editing {
            ui.weak("Finish the take to edit this chain.");
        }
        if let Some(track) = self.project.track_mut(id) {
            for (index, effect) in track.effects.iter_mut().enumerate() {
                ui.group(|ui| {
                    if !editing {
                        ui.disable();
                    }
                    ui.horizontal(|ui| {
                        let state = if effect.enabled {
                            "Enabled"
                        } else {
                            "Bypassed"
                        };
                        changed |= ui
                            .checkbox(&mut effect.enabled, state)
                            .on_hover_text("Bypass or enable this insert")
                            .changed();
                        ui.label(
                            egui::RichText::new(effect.kind.label())
                                .strong()
                                .color(ACCENT_BRIGHT)
                                .size(12.0),
                        );
                        ui.weak(format!("  {:02}", index + 1));
                        ui.add_space(5.0);
                        if ui
                            .add_enabled(index > 0, egui::Button::new("Up"))
                            .on_hover_text("Move effect earlier in chain")
                            .clicked()
                        {
                            operation = Some((index, "up"));
                        }
                        if ui
                            .add_enabled(index + 1 < count, egui::Button::new("Down"))
                            .on_hover_text("Move effect later in chain")
                            .clicked()
                        {
                            operation = Some((index, "down"));
                        }
                        if ui
                            .button("Remove")
                            .on_hover_text("Remove effect from this track")
                            .clicked()
                        {
                            operation = Some((index, "remove"));
                        }
                    });
                    match effect.kind {
                        BuiltinEffectKind::Eq3 => {
                            ui.horizontal_wrapped(|ui| {
                                for (band, label) in ["LOW", "MID", "HIGH"].iter().enumerate() {
                                    ui.label(
                                        egui::RichText::new(*label).size(10.0).color(STUDIO_DIM),
                                    );
                                    changed |= ui
                                        .add(
                                            egui::Slider::new(
                                                &mut effect.gains_db[band],
                                                -24.0..=24.0,
                                            )
                                            .step_by(0.5)
                                            .max_decimals(1)
                                            .suffix(" dB"),
                                        )
                                        .on_hover_text("Gain for this EQ band; 0 dB = neutral")
                                        .changed();
                                }
                            });
                        }
                        BuiltinEffectKind::Eq8 => {
                            changed |=
                                draw_eq8_curve(ui, &mut effect.gains_db, sample_rate, id, index);
                            ui.small("Band gain (dB)    ·    Fixed Q 1.1");
                            ui.horizontal_wrapped(|ui| {
                                for (i, hz) in daw_engine::effects::EQ8_HZ.iter().enumerate() {
                                    ui.vertical(|ui| {
                                        ui.label(
                                            egui::RichText::new(format!("{hz} Hz"))
                                                .size(9.0)
                                                .color(STUDIO_DIM),
                                        );
                                        changed |= ui
                                            .add(
                                                egui::DragValue::new(&mut effect.gains_db[i])
                                                    .range(-24.0..=24.0)
                                                    .speed(0.15)
                                                    .fixed_decimals(1)
                                                    .suffix(" dB"),
                                            )
                                            .on_hover_text(
                                                "Parametric EQ bell gain for this center frequency",
                                            )
                                            .changed();
                                    });
                                }
                            });
                        }
                        BuiltinEffectKind::VocalReverb | BuiltinEffectKind::GuitarReverb => {
                            ui.horizontal_wrapped(|ui| {
                                ui.small("Dry / wet");
                                changed |= ui
                                    .add(
                                        egui::Slider::new(&mut effect.wet, 0.0..=1.0)
                                            .max_decimals(2),
                                    )
                                    .on_hover_text("Dry/wet blend")
                                    .changed();
                                ui.small("Decay");
                                changed |= ui
                                    .add(
                                        egui::DragValue::new(&mut effect.decay_s)
                                            .range(0.2..=8.0)
                                            .speed(0.02)
                                            .suffix(" s"),
                                    )
                                    .on_hover_text("Approximate decay time")
                                    .changed();
                                ui.small("Damping");
                                changed |= ui
                                    .add(egui::Slider::new(&mut effect.tone, 0.0..=1.0))
                                    .on_hover_text("Higher values make the reverb tail darker")
                                    .changed();
                            });
                        }
                    }
                });
            }
            if let Some((index, action)) = operation {
                match action {
                    "remove" => {
                        track.effects.remove(index);
                    }
                    "up" if index > 0 => track.effects.swap(index, index - 1),
                    "down" if index + 1 < track.effects.len() => {
                        track.effects.swap(index, index + 1)
                    }
                    _ => {}
                }
                changed = true;
            }
        }
        if changed {
            self.fx_rebuild_pending = true;
            self.dirty = true;
        }
        ui.add_space(3.0);
        ui.label(egui::RichText::new(
            "Device settings are saved with the project. Playback rebuilds after you release a control.")
            .color(STUDIO_DIM).size(10.0));
    }
}

// EQ8 has eight fixed-frequency bells. Its response comes from the DSP coefficients.
fn draw_eq8_curve(
    ui: &mut egui::Ui,
    gains: &mut [f32; 8],
    rate: u32,
    track_id: u64,
    effect_index: usize,
) -> bool {
    use daw_engine::effects::{EQ8_HZ, eq8_response_db};
    let width = ui.available_width().min(750.0);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 148.0), Sense::hover());
    let plot = Rect::from_min_max(
        rect.min + Vec2::new(33.0, 12.0),
        rect.max - Vec2::new(10.0, 22.0),
    );
    if !plot.is_positive() {
        return false;
    }
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 5.0, INK_0);
    let log_min = 30.0_f32.ln();
    let log_range = 16_000.0_f32.ln() - log_min;
    let x_of = |hz: f32| plot.left() + (hz.ln() - log_min) / log_range * plot.width();
    let y_of = |db: f32| plot.center().y - db.clamp(-24.0, 24.0) * plot.height() / 48.0;
    for db in [-24, -12, 0, 12, 24] {
        let y = y_of(db as f32);
        painter.hline(
            plot.x_range(),
            y,
            Stroke::new(0.8_f32, if db == 0 { STUDIO_DIM } else { STUDIO_BORDER }),
        );
        painter.text(
            Pos2::new(plot.left() - 5.0, y),
            egui::Align2::RIGHT_CENTER,
            format!("{db:+}"),
            FontId::proportional(10.0),
            STUDIO_DIM,
        );
    }
    let mut changed = false;
    for (band, &hz) in EQ8_HZ.iter().enumerate() {
        let x = x_of(hz);
        painter.vline(x, plot.y_range(), Stroke::new(0.7_f32, STUDIO_BORDER));
        let label = if hz >= 1000.0 {
            format!("{:.1}k", hz / 1000.0)
        } else {
            format!("{hz:.0}")
        };
        painter.text(
            Pos2::new(x, plot.bottom() + 6.0),
            egui::Align2::CENTER_TOP,
            label,
            FontId::proportional(10.0),
            STUDIO_DIM,
        );
        let handle = Pos2::new(x, y_of(gains[band]));
        let response = ui
            .interact(
                Rect::from_center_size(handle, Vec2::splat(22.0)),
                ui.id().with(("eq8_band", track_id, effect_index, band)),
                Sense::click_and_drag(),
            )
            .on_hover_cursor(egui::CursorIcon::ResizeVertical);
        let mut gain = gains[band];
        if response.double_clicked() {
            gain = 0.0;
        } else if response.dragged()
            && let Some(pos) = response.interact_pointer_pos()
        {
            gain = ((plot.center().y - pos.y) * 48.0 / plot.height()).clamp(-24.0, 24.0);
        }
        if gain != gains[band] {
            gains[band] = gain;
            changed = true;
        }
        response.on_hover_text(format!(
            "{hz:.0} Hz   {gain:+.1} dB   Q 1.1 (fixed)\nDrag for gain. Double-click to reset."
        ));
    }
    let steps = (plot.width() / 2.0).ceil().max(1.0) as usize;
    let points = (0..=steps)
        .map(|i| {
            let fraction = i as f32 / steps as f32;
            let hz = (log_min + fraction * log_range).exp();
            Pos2::new(
                plot.left() + fraction * plot.width(),
                y_of(eq8_response_db(gains, hz, rate)),
            )
        })
        .collect::<Vec<_>>();
    painter.add(egui::Shape::line(
        points,
        Stroke::new(1.6_f32, ACCENT_BRIGHT),
    ));
    for (&hz, &db) in EQ8_HZ.iter().zip(gains.iter()) {
        painter.circle(
            Pos2::new(x_of(hz), y_of(db)),
            4.8,
            ACCENT_BRIGHT,
            Stroke::new(1.1_f32, INK_0),
        );
    }
    changed
}

#[cfg(test)]
mod eq8_interaction_tests {
    use super::*;

    #[test]
    fn dragging_a_handle_changes_only_that_gain_and_empty_plot_clicks_are_inert() {
        let ctx = egui::Context::default();
        let mut gains = [0.0; 8];
        let mut render = |events: Vec<egui::Event>| {
            let mut bounds = Rect::NOTHING;
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(640.0, 240.0))),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        bounds = Rect::from_min_size(
                            ui.cursor().min,
                            Vec2::new(ui.available_width(), 148.0),
                        );
                        draw_eq8_curve(ui, &mut gains, 48_000, 1, 0);
                    });
                },
            );
            (bounds, gains)
        };
        let (bounds, _) = render(vec![]);
        let plot = Rect::from_min_max(
            bounds.min + Vec2::new(33.0, 12.0),
            bounds.max - Vec2::new(10.0, 22.0),
        );
        let x = plot.left() + (350.0_f32 / 30.0).ln() / (16_000.0_f32 / 30.0).ln() * plot.width();
        let handle = Pos2::new(x, plot.center().y);
        let event = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        let empty = handle - Vec2::new(0.0, 27.0);
        render(vec![egui::Event::PointerMoved(empty), event(empty, true)]);
        assert_eq!(render(vec![event(empty, false)]).1, [0.0; 8]);
        render(vec![egui::Event::PointerMoved(handle), event(handle, true)]);
        // Cross another band's x coordinate while held. The originally grabbed handle keeps ownership.
        let end = empty + Vec2::new(100.0, 0.0);
        render(vec![egui::Event::PointerMoved(end)]);
        let edited = render(vec![event(end, false)]).1;
        assert!(edited[3] > 10.0);
        assert_eq!(edited.iter().filter(|g| **g != 0.0).count(), 1);
    }
}

#[cfg(test)]
#[test]
#[ignore = "requires a private PulseAudio server with a synthetic input and output"]
fn hardware_panel_and_playback_survive_repeated_input_discovery() {
    use std::sync::atomic::Ordering;

    let expected_source = "qa_synthetic_input";
    for _ in 0..75 {
        let sources = Studio::available_input_devices().unwrap();
        assert!(
            sources
                .iter()
                .any(|source| source.id.id() == expected_source)
        );
        assert!(
            sources
                .iter()
                .all(|source| !source.id.id().ends_with(".monitor"))
        );
    }

    let mut studio = Studio::with_sample_rate(44_100);
    studio.add_audio_track();
    let ctx = egui::Context::default();
    let _ = ctx.run(egui::RawInput::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| studio.draw_hardware_controls(ui));
    });
    let sources = studio.capture_devices.as_ref().unwrap().1.as_ref().unwrap();
    assert!(
        sources
            .iter()
            .any(|source| source.id.id() == expected_source)
    );

    studio.start_playback();
    let driver = studio.driver.as_ref().expect("demo opened its output");
    assert_eq!(driver.info().device, "Null Output");
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let telemetry = driver.telemetry();
        let peak = f32::from_bits(telemetry.output_peak_bits.load(Ordering::Relaxed));
        let position = telemetry.project_frame.load(Ordering::Relaxed);
        if peak > 0.0 && position >= 4_410 {
            assert!(position < 176_400, "demo advanced too far: {position}");
            break;
        }
        assert!(
            Instant::now() < deadline,
            "demo did not render audible samples"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}
