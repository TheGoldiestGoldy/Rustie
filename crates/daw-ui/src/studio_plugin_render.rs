// Bundle selection for isolated live inserts and offline clip renders.
use std::sync::mpsc;

/// A selected AudioClip may represent only a portion of the original source,
/// with gain/fades. Send precisely that audio to the isolated plugin worker.
struct AudioRenderInput {
    samples: Arc<[StereoFrame]>,
    rate: u32,
    frames: u64,
    offset: u64,
    gain: f32,
    fade_in_ms: u32,
    fade_out_ms: u32,
}

fn write_audio_clip_input(path: &Path, clip: &AudioRenderInput) -> Result<(), String> {
    if clip.frames == 0 || clip.frames > clip.rate as u64 * 60 * 10 {
        return Err("Offline plugin processing is limited to 10 minutes per clip".into());
    }
    let mut builder = GraphBuilder::new(clip.rate);
    builder.add_node(NodeSpec::audio(
        1,
        Source::TrimmedSamples {
            frames: Arc::clone(&clip.samples),
            start_frame: 0,
            end_frame: clip.frames,
            source_offset_frames: clip.offset,
            gain: clip.gain,
            fade_in_frames: clip.fade_in_ms as u64 * clip.rate as u64 / 1000,
            fade_out_frames: clip.fade_out_ms as u64 * clip.rate as u64 / 1000,
        },
    ));
    builder.add_node(NodeSpec::master(MASTER_ID));
    builder.route(1, MASTER_ID, 1.0);
    let mut graph = builder.build().map_err(|e| format!("Build plugin input graph: {e:?}"))?;
    let spec = hound::WavSpec { channels: 2, sample_rate: clip.rate,
        bits_per_sample: 32, sample_format: hound::SampleFormat::Float };
    let mut wav = hound::WavWriter::create(path, spec)
        .map_err(|e|format!("Write plugin source clip: {e}"))?;
    for frame in 0..clip.frames {
        let sample = graph.render_frame(frame);
        wav.write_sample(sample.left).map_err(|e|e.to_string())?;
        wav.write_sample(sample.right).map_err(|e|e.to_string())?;
    }
    wav.finalize().map_err(|e|e.to_string())
}

impl Studio {
    fn plugin_render_start(&mut self) {
        if self.plugin_job.is_some() {
            self.status = "A plugin render is already running".into();
            return;
        }
        let Some(plugin) = self.plugin_selected.clone() else {
            self.error = Some("Select a native Linux CLAP, VST2 or VST3 plugin first".into());
            return;
        };
        let is_vst3 = plugin.extension().is_some_and(|e| e == "vst3");
        let is_clap = plugin.extension().is_some_and(|e| e == "clap");
        if is_clap && self.selected_class_id.is_none() {
            self.error=Some("Choose a CLAP plugin class from the scan list first".into());
            return;
        }
        let is_vst2 = plugin.extension().is_some_and(|e| e == "so");
        if !plugin.exists() || !(is_vst3 || is_vst2 || is_clap) {
            self.error = Some("Selected native CLAP/VST2/VST3 plugin no longer exists".into());
            return;
        }
        let Some((_, clip)) = self.selected_clip_pair().map(|(t, c)| (t.id, c.clone())) else {
            self.error = Some("Select an audio or MIDI clip to process with the plug-in".into());
            return;
        };
        let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
        let output_dir = home.join("Music/Rusty Plugin Renders");
        if let Err(e) = fs::create_dir_all(&output_dir) {
            self.error = Some(format!("Cannot create plugin render directory: {e}"));
            return;
        }
        let cache_dir = home.join(".cache/rustie");
        if let Err(e) = fs::create_dir_all(&cache_dir) {
            self.error = Some(format!("Cannot create plugin cache: {e}"));
            return;
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH).map_or(0, |v| v.as_millis());
        let out = output_dir.join(format!("Plugin-{now}-{}.wav", clip.id));
        let request = cache_dir.join(format!("render-{now}-{}.json", clip.id));
        let (kind, input, prepared_audio) = match &clip.kind {
            ClipKind::Audio(audio) => {
                let Some(cached) = self.media.get(&audio.source) else {
                    self.error = Some(format!("Audio source has no decoded waveform: {}",audio.source.display()));
                    return;
                };
                let length = self.tick_to_frame(clip.duration.ticks()).max(1);
                let prepared = AudioRenderInput {
                    samples: Arc::clone(&cached.samples),
                    rate: self.sample_rate,
                    frames: length,
                    offset: audio.source_offset_frames,
                    gain: audio.gain,
                    fade_in_ms: audio.fade_in_ms,
                    fade_out_ms: audio.fade_out_ms,
                };
                (if is_clap {"render-clap-effect"} else if is_vst3 { "render-effect" }
                    else { "render-vst2-effect" },
                    cache_dir.join(format!("clip-input-{now}-{}.wav",clip.id)),
                    Some(prepared))
            }
            ClipKind::Midi(midi) => {
                let bpm = self.project.tempo_map.tempo_at(MusicalTime::ZERO)
                    .map_or(120.0, |t| t.bpm());
                let to_frame = |ticks: i64| {
                    (ticks as f64 / PPQ as f64 * 60.0 / bpm
                        * 48_000.0).round().max(0.0) as u64
                };
                let mut events = Vec::new();
                for note in &midi.notes {
                    if note.muted || note.start.ticks() >= clip.duration.ticks() {
                        continue;
                    }
                    events.push(serde_json::json!({
                        "frame": to_frame(note.start.ticks()),
                        "note": note.pitch, "velocity": note.velocity, "on": true
                    }));
                    events.push(serde_json::json!({
                        "frame": to_frame(note.start.ticks().saturating_add(note.duration.ticks())
                            .min(clip.duration.ticks())),
                        "note": note.pitch, "velocity": 0, "on": false
                    }));
                }
                let content = serde_json::json!({
                    "length_frames": to_frame(clip.duration.ticks()).max(1),
                    "bpm": bpm,
                    "events": events
                });
                if let Err(e) = fs::write(&request, content.to_string()) {
                    self.error = Some(format!("Write render request: {e}"));
                    return;
                }
                (if is_clap { "render-clap-midi" } else if is_vst3 { "render-midi" } else { "render-vst2-midi" },
                    request.clone(), None)
            }
        };
        let worker = match crate::plugin_worker_path() {
            Ok(worker) => worker,
            Err(error) => {
                self.error = Some(error);
                return;
            }
        };
        if !worker.is_file() {
            self.error = Some(format!("Plug-in worker missing: {}. Build with cargo build --release -p daw-plugin-host",worker.display()));
            return;
        }
        let (tx, rx) = mpsc::channel();
        self.plugin_job = Some(rx);
        self.status = format!("Rendering selected clip with {} (separate plug-in worker)…",
            plugin.file_name().unwrap_or_default().to_string_lossy());
        self.error = None;
        let selected_clap_class=self.selected_class_id.clone();
        std::thread::spawn(move || {
            let prepared = prepared_audio.as_ref().map_or(Ok(()),
                |clip| write_audio_clip_input(&input, clip));
            let result = match prepared {
                Err(error) => Err(error),
                Ok(()) => {
                    let mut command=std::process::Command::new("timeout");
                    command.arg("-k").arg("3s").arg("180s").arg(&worker).arg(kind).arg(&plugin)
                        .arg(&input).arg(&out);
                    if is_clap
                        && let Some(id)=selected_clap_class.as_deref() {command.arg(id);}
                    let result=command.output();
                    match result {
                        Ok(output) if output.status.success() && out.is_file() => Ok(out.clone()),
                        Ok(output) => Err(format!("Plug-in worker failed (code {:?}): {}",
                            output.status.code(), String::from_utf8_lossy(&output.stderr).trim())),
                        Err(e) => Err(format!("Launch plug-in worker: {e}")),
                    }
                }
            };
            let _ = fs::remove_file(&request);
            if prepared_audio.is_some() { let _ = fs::remove_file(&input); }
            if result.is_err() { let _ = fs::remove_file(out); }
            let _ = tx.send(result);
        });
    }

    fn poll_plugin_job(&mut self) {
        let done = self.plugin_job.as_ref().and_then(|rx| rx.try_recv().ok());
        if let Some(result) = done {
            self.plugin_job = None;
            match result {
                Ok(path) => {
                    self.status = format!("Plug-in audio render complete: {}", path.display());
                    self.add_audio(path);
                }
                Err(err) => {
                    self.error = Some(err);
                    self.status = "Plug-in render failed; original clips untouched".into();
                }
            }
        }
    }

    fn plugin_device_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("NATIVE PLUG-INS")
                .strong().color(ACCENT_BRIGHT).size(11.0));
            ui.separator();
            ui.small("CLAP  /  VST3  /  VST2");
            if ui.small_button("↻ Rescan").on_hover_text(
                "Re-index installed Linux plugins and samples").clicked() {
                self.refresh_library();
            }
        });
        ui.label(egui::RichText::new(
            "Choose a bundle to inspect its classes and assign a track insert, or render a selected clip to WAV."
        ).size(10.0).color(STUDIO_DIM));
        let plugins = self.plugin_paths.iter().filter(|p|
            p.extension().is_some_and(|e| e == "vst3" || e == "so" || e == "clap"));
        if plugins.clone().next().is_none() {
            ui.colored_label(SUNSET, "No loadable bundles indexed. Use Rescan.");
        }
        let mut choice = None;
        ui.small("PLUG-IN BUNDLE");
        egui::ComboBox::from_id_salt("plugin_render_select")
            .width(330.0)
            .selected_text(self.plugin_selected.as_ref()
                .and_then(|p| p.file_name()).map_or_else(|| "Choose native CLAP / VST…".into(),
                    |p| p.to_string_lossy().to_string()))
            .show_ui(ui, |ui| {
                for plugin in plugins {
                    let label = plugin.file_name().unwrap_or_default().to_string_lossy().to_string();
                    if ui.selectable_label(self.plugin_selected.as_ref() == Some(plugin), label).clicked() {
                        choice = Some(plugin.clone());
                    }
                }
            }).response.on_hover_text("Choose an installed Linux CLAP .clap, VST2 .so or VST3 .vst3 bundle");
        if let Some(path) = choice { self.select_plugin_bundle(path); }
        let format = self.plugin_selected.as_ref().and_then(|p| p.extension()?.to_str());
        let is_clap = format == Some("clap");
        let is_vst3 = format == Some("vst3");
        let previous_class = self.selected_class_id.clone();
        if is_clap || is_vst3 {
            ui.horizontal_wrapped(|ui| {
                if ui.add_enabled(self.plugin_scan_job.is_none(), egui::Button::new("Inspect classes"))
                    .on_hover_text("Read vendor and category in the isolated plug-in host").clicked() {
                    self.inspect_plugin_bundle();
                }
                if self.plugin_scan_job.is_some() {
                    ui.spinner();
                    ui.label("Inspecting plug-in…");
                }
                if !self.plugin_classes.is_empty() {
                    let selected = self.plugin_classes.iter().find(|c| Some(&c.id) == self.selected_class_id.as_ref());
                    if is_clap {
                        egui::ComboBox::from_id_salt("rustie_clap_class")
                            .selected_text(selected.map_or("Select class", |c| &c.name))
                            .show_ui(ui, |ui| {
                                for class in &self.plugin_classes {
                                    ui.selectable_value(&mut self.selected_class_id, Some(class.id.clone()), &class.name);
                                }
                            });
                    } else if let Some(class) = selected {
                        ui.label(&class.name);
                    }
                }
            });
            if previous_class != self.selected_class_id {
                self.select_compatible_track_for_plugin();
            }
            if let Some(class) = self.plugin_classes.iter().find(|c| Some(&c.id) == self.selected_class_id.as_ref()) {
                let details = [class.vendor.as_str(), class.category.as_str()].into_iter()
                    .filter(|field| !field.is_empty()).collect::<Vec<_>>().join("  ·  ");
                ui.label(details).on_hover_text(&class.id);
            }
            if let Some(error) = &self.plugin_scan_error { ui.colored_label(SUNSET, error); }
        }
        let bundle_exists = self.plugin_selected.as_ref().is_some_and(|p| p.exists());
        let can_render = bundle_exists && (!is_clap || self.selected_class_id.is_some())
            && self.plugin_job.is_none() && self.selected_clip_pair().is_some();
        ui.horizontal(|ui| {
            if ui.add_enabled(can_render,
                egui::Button::new("Render selected clip to WAV").small())
                .on_hover_text("Render instrument MIDI or process audio in an isolated worker. Creates a separate WAV track.")
                .clicked() {
                self.plugin_render_start();
            }
            if self.plugin_job.is_some() { ui.spinner(); ui.label("Processing…"); }
        });

        ui.separator();
        ui.label(egui::RichText::new("LIVE CLAP/VST3 INSTRUMENT / EFFECT · EXPERIMENTAL")
            .size(11.0).strong().color(ACCENT_BRIGHT));
        ui.label(egui::RichText::new(
            "MIDI tracks take instruments. Audio tracks and groups take effects. Project Save captures native state from running workers that support it."
        ).color(STUDIO_DIM).size(10.0));
        let track_id = self.selected_track.filter(|id| self.project.tracks.iter()
            .any(|track| track.id == *id &&
                matches!(track.kind, TrackKind::Audio | TrackKind::Group | TrackKind::Midi)));
        let midi_track = track_id.is_some_and(|id| self.project.tracks.iter()
            .any(|track| track.id == id && track.kind == TrackKind::Midi));
        let saved = track_id.and_then(|id| self.project.tracks.iter()
            .find(|track| track.id == id)).and_then(|track| track.live_clap.as_ref());
        let selected_class = self.plugin_classes.iter()
            .find(|class| Some(&class.id) == self.selected_class_id.as_ref());
        let class_fits = selected_class.is_some_and(|class| class.supports_track(midi_track));
        if selected_class.is_some() && !class_fits && track_id.is_some() {
            ui.weak(if midi_track { "Select an Audio or Group track for this effect." }
                else { "Select a MIDI track for this instrument." });
        }
        let same_insert = saved.is_some_and(|insert| Some(&insert.bundle) == self.plugin_selected.as_ref()
            && (is_vst3 || Some(&insert.class_id) == self.selected_class_id.as_ref()));
        let can_change = self.recorder.is_none() && self.midi_take.is_none();
        let mut live_action: Option<&'static str> = None;
        ui.horizontal_wrapped(|ui| {
            if ui.add_enabled(can_change && track_id.is_some() &&
                bundle_exists && class_fits && !same_insert && (is_vst3 || is_clap),
                egui::Button::new(if same_insert { "Assigned to track" } else if saved.is_some() {
                    "Replace track insert"
                } else if midi_track { "Assign instrument" } else { "Assign audio effect" }))
                .on_hover_text("Assign this class to the track. The isolated worker starts with the playback graph.")
                .on_disabled_hover_text("Inspect the bundle, choose a compatible class and select a track. Finish any active take before changing inserts.")
                .clicked() {
                live_action = Some("attach");
            }
            if ui.add_enabled(can_change && saved.is_some(),
                egui::Button::new("Remove insert"))
                .on_hover_text("Remove the live effect; built-in FX remain intact")
                .clicked() {
                live_action = Some("remove");
            }
        });
        ui.horizontal_wrapped(|ui| {
            if let (Some(saved), Some(id)) = (saved, track_id) {
                let bundle_name = saved.bundle.file_name().unwrap_or_default().to_string_lossy();
                let insert_name = selected_class.filter(|_| same_insert)
                    .map_or(bundle_name.as_ref(), |class| class.name.as_str());
                ui.small(format!("Track insert: {insert_name}"))
                    .on_hover_text(if saved.class_id.is_empty() { bundle_name.as_ref() } else { &saved.class_id });
                if ui.add_enabled(can_change, egui::Button::new(if saved.enabled { "Bypass" } else { "Enable" }))
                    .on_hover_text("Toggle this track insert and retain its assignment and saved state").clicked() {
                    live_action = Some("toggle");
                }
                if !saved.enabled {
                    ui.weak("Bypassed");
                } else if !saved.bundle.exists() {
                    ui.colored_label(SUNSET, "Assigned bundle missing");
                } else if let Some(error) = self.live_plugin_errors.get(&id) {
                    ui.colored_label(SUNSET, "Plug-in activation failed").on_hover_text(error);
                } else if self.driver.is_none() {
                    ui.weak("Assigned · engine idle");
                } else if let Some(runtimes) = self.live_plugin_runtimes.get(&id) {
                    let stopped = runtimes.iter().filter(|runtime| runtime.bridge.is_finished()).count();
                    let missed: u64 = runtimes.iter()
                        .map(|runtime| runtime.missed_blocks.load(Ordering::Relaxed)).sum();
                    if stopped == 0 {
                        ui.small(format!("Worker active · {missed} fallback blocks"));
                    } else {
                        ui.colored_label(SUNSET, format!("{stopped} plug-in worker(s) stopped · {missed} fallback blocks"));
                    }
                    ui.weak(if midi_track { "Fallback: silence" } else { "Fallback: dry audio" });
                    ui.ctx().request_repaint_after(Duration::from_millis(250));
                } else {
                    ui.weak("Assigned · worker is idle");
                }
                ui.small(if saved.state.is_some() { "Project contains native state" } else { "No native state saved" });
            } else {
                ui.weak(if midi_track { "No live CLAP/VST3 instrument on selected MIDI track" }
                    else { "No live effect on selected track" });
            }
        });
        match (live_action, track_id) {
            (Some("attach"), Some(id)) => {
                let choice = self.plugin_selected.clone().and_then(|bundle| {
                    let class_id = if bundle.extension().is_some_and(|ext| ext == "vst3") {
                        Some(String::new())
                    } else {
                        self.selected_class_id.clone()
                    }?;
                    Some((bundle, class_id))
                });
                if let Some((bundle, class_id)) = choice
                    && let Some(track) = self.project.track_mut(id) {
                        track.live_clap = Some(ClapLiveInsert {
                            bundle, class_id, enabled: true, state: None,
                        });
                        self.live_plugin_errors.remove(&id);
                        self.dirty = true;
                        self.status = if midi_track {
                            "Experimental live CLAP/VST3 instrument assigned to MIDI track".into()
                        } else {
                            "Experimental isolated live audio insert assigned".into()
                        };
                        self.restart_for_edit();
                    }
            }
            (Some("toggle"), Some(id)) => {
                if saved.is_some_and(|insert| insert.enabled) {
                    self.capture_live_plugin_states(Some(id));
                }
                if let Some(insert) = self.project.track_mut(id).and_then(|t| t.live_clap.as_mut()) {
                    insert.enabled = !insert.enabled;
                    self.dirty = true;
                    self.restart_for_edit();
                }
            }
            (Some("remove"), Some(id)) => {
                if let Some(track) = self.project.track_mut(id) {
                    track.live_clap = None;
                    self.live_plugin_errors.remove(&id);
                    self.dirty = true;
                    self.status = "Live plugin effect removed".into();
                    self.restart_for_edit();
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod plugin_input_tests {
    use super::*;
    #[test]
    fn isolated_plugin_input_is_exactly_the_selected_gain_adjusted_slice() {
        let samples: Arc<[StereoFrame]> = (0..100)
            .map(|i| StereoFrame { left: i as f32 / 100.0, right: -(i as f32 / 100.0) })
            .collect::<Vec<_>>().into();
        let clip = AudioRenderInput { samples, rate:48_000,frames:20,offset:30,
            gain:0.5,fade_in_ms:0,fade_out_ms:0 };
        let file=std::env::temp_dir().join(format!("rustie-plugin-input-{}.wav",std::process::id()));
        write_audio_clip_input(&file,&clip).unwrap();
        let mut reader=hound::WavReader::open(&file).unwrap();
        assert_eq!(reader.spec().sample_rate,48_000);
        let stereo=reader.samples::<f32>().collect::<Result<Vec<_>,_>>().unwrap();
        assert_eq!(stereo.len(),40);
        assert!((stereo[0]-0.15).abs()<1e-6);
        assert!((stereo[39]+0.245).abs()<1e-6);
        let _=fs::remove_file(file);
    }
}
