// Non-destructive audio clip editing and waveform preview.
impl Studio {
    fn audio_detail_editor(&mut self, ui: &mut egui::Ui, track_id: u64,
                           track_name: &str, clip: &Clip) {
        let ClipKind::Audio(audio) = &clip.kind else { return };
        ui.horizontal(|ui| {
            ui.heading(format!("≈  {track_name}"));
            ui.separator();
            ui.label(egui::RichText::new(
                audio.source.file_name().unwrap_or_default().to_string_lossy()
            ).size(11.0).color(STUDIO_DIM)).on_hover_text(audio.source.display().to_string());
        });
        let bpm = self.project.tempo_map.tempo_at(MusicalTime::ZERO)
            .map_or(120.0, |t| t.bpm());
        ui.horizontal(|ui| {
            let mut start_beats = clip.start.ticks() as f64 / PPQ as f64;
            let mut duration_beats = clip.duration.ticks() as f64 / PPQ as f64;
            ui.label("Start").on_hover_text("Position of this clip in the arrangement, measured in quarter-note beats");
            let moved = ui.add_enabled(self.view == View::Arrangement, egui::DragValue::new(&mut start_beats)
                .speed(0.25).range(0.0..=10000.0).suffix(" beats"))
                .on_hover_text("Move the clip without changing its audio data").changed();
            ui.separator();
            ui.label("Length").on_hover_text("Non-destructively shorten the audible length of the clip");
            let trimmed = ui.add(egui::DragValue::new(&mut duration_beats)
                .speed(0.25).range(0.01..=10000.0).suffix(" beats"))
                .on_hover_text("Trim end of audio without modifying the source WAV").changed();
            if moved || trimmed {
                let start_ticks = (start_beats * PPQ as f64).round().max(0.0) as i64;
                let length_ticks = (duration_beats * PPQ as f64).round().max(1.0) as i64;
                if let Some(target) = self.project.track_mut(track_id)
                    .and_then(|t| t.all_clips_mut().find(|c| c.id == clip.id)) {
                    if moved { target.start = MusicalTime::from_ticks(start_ticks); }
                    if trimmed { target.duration = MusicalTime::from_ticks(length_ticks); }
                    self.dirty = true;
                    self.restart_for_edit();
                }
            }
        });
        ui.horizontal(|ui| {
            // Left trim is expressed in source frames so split/trim never
            // changes the original WAV or silently shifts audio content.
            let mut trim_in_beats = audio.source_offset_frames as f64
                / self.sample_rate.max(1) as f64 * bpm / 60.0;
            let before = trim_in_beats;
            ui.label("Trim in").on_hover_text(
                "Hide audio from the start of the source without rewriting the WAV. The right edge stays fixed.");
            let changed = ui.add(egui::DragValue::new(&mut trim_in_beats)
                .speed(0.05).range(0.0..=10_000.0).suffix(" beats"))
                .on_hover_text("Non-destructive source-in point; uses an exact frame offset")
                .changed();
            if changed {
                let old_ticks = (before * PPQ as f64).round() as i64;
                let new_ticks = (trim_in_beats * PPQ as f64).round().max(0.0) as i64;
                self.apply_clip_trim(clip.id, ClipTrimEdge::Left, new_ticks - old_ticks);
            }
            ui.separator();
            ui.label(egui::RichText::new(format!(
                "source in {} frames · {:.3}s",
                audio.source_offset_frames,
                audio.source_offset_frames as f64 / self.sample_rate.max(1) as f64
            )).size(10.0).color(STUDIO_DIM))
                .on_hover_text("Exact decoded-source frame used by the first audible sample of this clip");
        });
        ui.horizontal_wrapped(|ui| {
            let mut gain_percent = (audio.gain * 100.0).round().clamp(0.0, 200.0) as i32;
            let mut fade_in_ms = audio.fade_in_ms;
            let mut fade_out_ms = audio.fade_out_ms;
            ui.label("Clip gain").on_hover_text("Non-destructive gain applied only to this audio clip");
            let gain_changed = ui.add(egui::DragValue::new(&mut gain_percent)
                .range(0..=200).speed(0.5).suffix("%"))
                .on_hover_text("Change clip gain; 100% is original volume").changed();
            ui.separator();
            ui.label("Fade in").on_hover_text("Smoothly ramp up this clip at its beginning");
            let fade_in_changed = ui.add(egui::DragValue::new(&mut fade_in_ms)
                .range(0..=20_000).speed(10.0).suffix(" ms"))
                .on_hover_text("Linear fade-in duration in milliseconds").changed();
            ui.separator();
            ui.label("Fade out").on_hover_text("Smoothly ramp down before the clip ends");
            let fade_out_changed = ui.add(egui::DragValue::new(&mut fade_out_ms)
                .range(0..=20_000).speed(10.0).suffix(" ms"))
                .on_hover_text("Linear fade-out duration in milliseconds").changed();
            if (gain_changed || fade_in_changed || fade_out_changed)
                && let Some(target) = self.project.track_mut(track_id)
                    .and_then(|t| t.all_clips_mut().find(|c|c.id == clip.id))
                && let ClipKind::Audio(ref mut settings) = target.kind {
                if gain_changed { settings.gain = gain_percent as f32 / 100.0; }
                if fade_in_changed { settings.fade_in_ms = fade_in_ms; }
                if fade_out_changed { settings.fade_out_ms = fade_out_ms; }
                self.dirty = true;
                self.restart_for_edit();
            }
        });
        ui.separator();
        match self.media.get(&audio.source) {
            Some(prepared) if !prepared.peaks.is_empty() => {
                let width = ui.available_width().max(120.0);
                let height = ui.available_height().clamp(100.0, 310.0);
                let (rect, response) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
                response.on_hover_text("Real decoded waveform amplitude. Source file is not modified by clip editing.");
                let painter = ui.painter_at(rect);
                painter.rect_filled(rect, 6.0, INK_0);
                painter.line_segment([
                    Pos2::new(rect.left(), rect.center().y),
                    Pos2::new(rect.right(), rect.center().y)
                ], Stroke::new(0.6_f32, STUDIO_BORDER));
                let peaks = &prepared.peaks;
                let count = (rect.width() as usize).clamp(64, 1500);
                let bpm = self.project.tempo_map.tempo_at(MusicalTime::ZERO)
                    .map_or(120.0, |t| t.bpm());
                let clip_frames = (clip.duration.ticks() as f64 / PPQ as f64
                    * 60.0 / bpm * self.sample_rate as f64).round().max(1.0) as u64;
                let start_frame = audio.source_offset_frames.min(prepared.samples.len() as u64);
                let shown_frames = clip_frames.min(prepared.samples.len() as u64 - start_frame);
                for x in 0..count {
                    let source_frame = start_frame.saturating_add(
                        shown_frames.saturating_mul(x as u64) / count as u64);
                    let index = ((source_frame as u128 * peaks.len() as u128)
                        / prepared.samples.len().max(1) as u128) as usize;
                    let peak = peaks[index.min(peaks.len()-1)].clamp(0.0, 1.0);
                    let local_frame = shown_frames.saturating_mul(x as u64) / count as u64;
                    let fade_in_frames = audio.fade_in_ms as u64 * self.sample_rate as u64 / 1000;
                    let fade_out_frames = audio.fade_out_ms as u64 * self.sample_rate as u64 / 1000;
                    let fade_in = if fade_in_frames == 0 {1.0} else {
                        (local_frame as f32 / fade_in_frames as f32).min(1.0)
                    };
                    let fade_out = if fade_out_frames == 0 {1.0} else {
                        (clip_frames.saturating_sub(local_frame) as f32 / fade_out_frames as f32).min(1.0)
                    };
                    let h = (peak * audio.gain * fade_in.min(fade_out) * rect.height() * 0.44)
                        .min(rect.height() * 0.44);
                    let px = rect.left() + x as f32 * rect.width() / count as f32;
                    painter.line_segment([
                        Pos2::new(px, rect.center().y - h),
                        Pos2::new(px, rect.center().y + h)
                    ], Stroke::new(1.15_f32, ACCENT_BRIGHT));
                }
                ui.label(format!("Decoded audio: {:.2}s · {} Hz · {} frames",
                    prepared.samples.len() as f64 / self.sample_rate as f64,
                    self.sample_rate, prepared.samples.len()));
            }
            _ => {
                ui.colored_label(SUNSET, "Waveform unavailable. Source may be missing or could not be decoded.");
                if ui.button("Retry decode").clicked()
                    && let Err(err) = self.decode_cached(&audio.source) { self.error = Some(err); }
            }
        }
    }
}
