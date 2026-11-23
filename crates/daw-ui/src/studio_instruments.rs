// Built-in SoundFont instrument rack: bank discovery/loading, track preset
// assignment, live tone controls, on-screen mini-keyboard, computer-keyboard
// triggering, and the Devices "Instrument" tab. All file parsing happens on a
// background thread; the resulting synthesizer is handed to the audio thread
// through the lock-free `rtrb` ring inside `SoundFontInstrument`.

fn discover_soundfont_files(root: &Path, out: &mut Vec<PathBuf>) {
    let mut pending = vec![(root.to_owned(), 0_usize)];
    let mut directories = 0_usize;
    while let Some((folder, depth)) = pending.pop() {
        directories += 1;
        if directories > 96 || out.len() >= 256 {
            break;
        }
        let Ok(entries) = std::fs::read_dir(&folder) else { continue };
        for entry in entries.flatten().take(400) {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "sf2") {
                out.push(path);
            } else if depth < 3 && entry.file_type().is_ok_and(|kind| kind.is_dir())
                && !entry.file_name().to_string_lossy().starts_with('.')
            {
                pending.push((path, depth + 1));
            }
        }
    }
}

impl Studio {
    pub(crate) fn soundfont_bank_name(path: &Path) -> String {
        path.file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string())
    }

    fn soundfont_search_roots() -> Vec<PathBuf> {
        let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
        let mut roots = vec![PathBuf::from("assets/soundfonts"), home.join("Music"), home.join("Downloads")];
        if let Ok(exe) = std::env::current_exe()
            && let Some(dir) = exe.parent()
        {
            roots.push(dir.join("assets/soundfonts"));
        }
        roots
    }

    fn discover_soundfonts(&mut self) {
        if self.soundfont_ready {
            return;
        }
        self.soundfont_ready = true;
        for root in Self::soundfont_search_roots() {
            discover_soundfont_files(&root, &mut self.soundfont_banks);
        }
        self.soundfont_banks.sort();
        self.soundfont_banks.dedup();
        if self.soundfont_banks.is_empty() {
            self.soundfont_error = Some(
                "No .sf2 SoundFont found. Drop an open-license bank into assets/soundfonts/ \
                 (see its README), then restart or press Rescan.".into(),
            );
        } else {
            self.soundfont_error = None;
        }
    }

    fn soundfont_path(&self, bank: &str) -> Option<PathBuf> {
        self.soundfont_banks
            .iter()
            .find(|path| Self::soundfont_bank_name(path) == bank)
            .cloned()
    }

    fn default_bank_name(&self) -> Option<String> {
        self.soundfont_banks
            .first()
            .map(|path| Self::soundfont_bank_name(path))
    }

    /// Parse a `.sf2` on a background thread. The finished bank is stored in
    /// `self.soundfonts` by `poll_soundfont_loads`, never read on the audio path.
    fn ensure_soundfont_loaded(&mut self, bank: &str) {
        if self.soundfonts.contains_key(bank) || self.soundfont_loads.contains_key(bank) {
            return;
        }
        let Some(path) = self.soundfont_path(bank) else { return };
        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = sender.send(load_sound_font(&path));
        });
        self.soundfont_loads.insert(bank.to_string(), receiver);
    }

    fn poll_soundfont_loads(&mut self) {
        let mut finished = Vec::new();
        let mut loaded_any = false;
        for (bank, receiver) in &self.soundfont_loads {
            match receiver.try_recv() {
                Ok(Ok(sound_font)) => {
                    finished.push(bank.clone());
                    self.soundfonts.insert(bank.clone(), sound_font);
                    loaded_any = true;
                    self.soundfont_error = None;
                }
                Ok(Err(error)) => {
                    finished.push(bank.clone());
                    self.soundfont_error = Some(error);
                }
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => {
                    finished.push(bank.clone());
                    self.soundfont_error = Some(format!("SoundFont loader stopped: {bank}"));
                }
            }
        }
        for bank in finished {
            self.soundfont_loads.remove(&bank);
        }
        if loaded_any {
            self.status = "SoundFont bank loaded".into();
            self.soundfont_rebuild_pending = true;
        }
    }

    fn load_project_soundfonts(&mut self) {
        let banks = self.project.tracks.iter()
            .filter_map(|track| track.instrument.as_ref().map(|instrument| instrument.bank.clone()))
            .collect::<std::collections::BTreeSet<_>>();
        for bank in banks {
            if self.soundfont_path(&bank).is_some() {
                self.ensure_soundfont_loaded(&bank);
            } else {
                self.soundfont_error = Some(format!("Missing SoundFont bank: {bank}"));
            }
        }
    }

    fn track_instrument(&self, track: &Track) -> Option<(SoundFontBank, InstrumentPreset)> {
        let instrument = track.instrument.as_ref()?;
        let bank = self.soundfonts.get(&instrument.bank)?.clone();
        Some((bank, instrument.resolve()))
    }

    fn instrument_tone(&self, track_id: u64) -> SoundFontTone {
        self.instrument_tones.get(&track_id).copied().unwrap_or_default()
    }

    fn midi_node_ids_for_track(&self, track: &Track) -> Vec<u32> {
        let mut ids = Vec::new();
        if let Ok(id) = Self::track_midi_node(track.id) {
            ids.push(id);
        }
        if (self.playing && self.playback_view == View::Session)
            || (!self.playing && self.view == View::Session)
        {
            for clip in track.session_slots.values() {
                if let Ok(id) = Self::track_midi_node(clip.id) {
                    ids.push(id);
                }
            }
        }
        ids
    }

    /// Assign a curated preset to a MIDI track. The bank is parsed off-thread if
    /// needed; once loaded, a running track swaps its synthesizer through the
    /// lock-free handle without rebuilding the graph.
    fn assign_instrument(&mut self, track_id: u64, bank: &str, preset: InstrumentPreset) {
        {
            let Some(track) = self.project.track_mut(track_id) else { return };
            track.instrument = Some(BuiltinInstrument::new(bank, preset.name));
        }
        self.dirty = true;
        self.ensure_soundfont_loaded(bank);
        let Some(sound_font) = self.soundfonts.get(bank).cloned() else {
            // Bank is still loading; poll_soundfont_loads triggers a rebuild.
            self.soundfont_rebuild_pending = true;
            self.status = format!("Loading {bank}…");
            return;
        };
        if self.driver.is_none() {
            // The next Play / monitor build embeds the new preset.
            self.status = format!("{} instrument: {}", bank, preset.name);
            return;
        }
        let node_ids = self
            .project
            .tracks
            .iter()
            .find(|track| track.id == track_id)
            .map(|track| self.midi_node_ids_for_track(track))
            .unwrap_or_default();
        let tone = self.instrument_tone(track_id);
        let mut rebuild = node_ids.is_empty();
        for node in &node_ids {
            if let Some(handle) = self.soundfont_handles.get_mut(node) {
                match build_synth_with_tone(
                    &sound_font,
                    self.sample_rate,
                    preset.channel,
                    preset.program,
                    tone,
                ) {
                    Ok(synth) => {
                        if let Err(error) = handle.replace(synth, preset.channel) {
                            self.error = Some(error);
                            rebuild = true;
                        }
                    }
                    Err(error) => {
                        self.error = Some(error);
                        rebuild = true;
                    }
                }
            } else {
                rebuild = true;
            }
        }
        if rebuild {
            // Nodes were still sine (bank finished loading after they were built).
            self.soundfont_rebuild_pending = true;
        }
        self.status = format!("{} instrument: {}", bank, preset.name);
    }

    /// Start the audio callback in stopped-transport monitoring mode so the
    /// on-screen/computer keyboard and hardware MIDI can audition instruments.
    fn start_monitor_driver(&mut self) {
        if self.playing || self.driver.is_some() {
            return;
        }
        self.midi_reset_routing();
        if self.backend == Backend::PulseAudio {
            let rate = Self::negotiate_sample_rate();
            if rate != self.sample_rate {
                self.sample_rate = rate;
                self.media.clear();
            }
        }
        let compiled = self.compiled_graph();
        let Some(graph) = self.apply_error(compiled) else { return };
        let config = DriverConfig {
            sample_rate: self.sample_rate,
            backend_preference: self.backend,
            ..DriverConfig::default()
        };
        let Some(mut driver) =
            self.apply_error(AudioDriver::start(graph, &config).map_err(|e| e.to_string()))
        else {
            return;
        };
        if driver
            .controller()
            .immediate(EngineCommand::SetMonitoring(true))
            .is_err()
        {
            self.error = Some("Monitor command queue unavailable".into());
            return;
        }
        self.driver = Some(driver);
    }

    fn start_audition_note(&mut self, note: u8) -> Option<(u32, u8)> {
        let node = self.midi_selected_node()?;
        self.start_monitor_driver();
        self.midi_send(node, 0, LiveMidiMessage::NoteOn(note, 100))
            .then_some((node, note))
    }

    fn stop_audition_note(&mut self, (node, note): (u32, u8)) {
        self.midi_send(node, 0, LiveMidiMessage::NoteOff(note));
    }

    fn send_instrument_cc(&mut self, track_id: u64, controller: u8, value: u8) {
        let (node_ids, channel) = self
            .project
            .tracks
            .iter()
            .find(|track| track.id == track_id)
            .map(|track| {
                (
                    self.midi_node_ids_for_track(track),
                    track
                        .instrument
                        .as_ref()
                        .map(|instrument| instrument.resolve().channel)
                        .unwrap_or(0),
                )
            })
            .unwrap_or_default();
        if let Some(driver) = self.driver.as_mut() {
            for node in node_ids {
                let _ = driver.controller().immediate(EngineCommand::ControlChange {
                    node,
                    channel,
                    controller,
                    value,
                });
            }
        }
    }

    fn instrument_device_editor(&mut self, ui: &mut egui::Ui) {
        let Some(track_id) = self.selected_track.filter(|id| {
            self.project
                .tracks
                .iter()
                .any(|track| track.id == *id && track.kind == TrackKind::Midi)
        }) else {
            ui.weak("Select a MIDI track to assign and play its built-in instrument.");
            return;
        };

        let current_bank = self
            .project
            .tracks
            .iter()
            .find(|track| track.id == track_id)
            .and_then(|track| track.instrument.as_ref().map(|instrument| instrument.bank.clone()))
            .or_else(|| self.default_bank_name());
        let current_preset = self
            .project
            .tracks
            .iter()
            .find(|track| track.id == track_id)
            .and_then(|track| track.instrument.as_ref().map(|instrument| instrument.preset.clone()))
            .unwrap_or_else(|| DEFAULT_PRESET.name.to_string());

        let mut bank_change: Option<String> = None;
        let mut preset_change: Option<InstrumentPreset> = None;

        ui.label(egui::RichText::new("SOUNDFONT BANK").size(10.0).color(STUDIO_DIM));
        if self.soundfont_banks.is_empty() {
            ui.colored_label(SUNSET, "No .sf2 banks found. Drop one into assets/soundfonts/ and press Rescan.");
            if ui.button("Rescan").clicked() {
                self.rescan_soundfonts();
            }
        } else {
            let selected = current_bank.clone().unwrap_or_default();
            egui::ComboBox::from_id_salt("rustie_sf_bank")
                .width(330.0)
                .selected_text(selected.as_str())
                .show_ui(ui, |ui| {
                    for bank in &self.soundfont_banks {
                        let name = Self::soundfont_bank_name(bank);
                        if ui.selectable_label(current_bank.as_deref() == Some(&name), &name).clicked() {
                            bank_change = Some(name);
                        }
                    }
                });
        }
        if let Some(error) = &self.soundfont_error {
            ui.colored_label(SUNSET, error);
        }

        ui.label(egui::RichText::new("PRESET").size(10.0).color(STUDIO_DIM));
        let selected_label = format!(
            "{}  ·  {}",
            current_preset,
            resolve_preset(&current_preset)
                .map(|preset| preset.category.label())
                .unwrap_or(DEFAULT_PRESET.category.label())
        );
        egui::ComboBox::from_id_salt("rustie_sf_preset")
            .width(330.0)
            .selected_text(selected_label.as_str())
            .show_ui(ui, |ui| {
                for category in InstrumentCategory::ALL {
                    ui.label(
                        egui::RichText::new(category.label()).size(9.5).color(STUDIO_DIM),
                    );
                    for preset in PRESETS.iter().filter(|preset| preset.category == category) {
                        if ui
                            .selectable_label(current_preset == preset.name, preset.name)
                            .clicked()
                        {
                            preset_change = Some(*preset);
                        }
                    }
                }
            });

        if let Some(bank) = bank_change {
            let preset = resolve_preset(&current_preset).unwrap_or(DEFAULT_PRESET);
            self.assign_instrument(track_id, &bank, preset);
        }
        if let (Some(preset), Some(bank)) = (preset_change, current_bank) {
            self.assign_instrument(track_id, &bank, preset);
        }

        ui.separator();
        self.draw_instrument_tone(ui, track_id);
        ui.separator();
        self.draw_mini_keyboard(ui);
    }

    fn draw_instrument_tone(&mut self, ui: &mut egui::Ui, track_id: u64) {
        ui.label(egui::RichText::new("TONE").size(10.0).color(STUDIO_DIM));
        ui.weak("These map to GM control changes the SoundFont synthesizer honors.");
        let mut tone = self.instrument_tone(track_id);
        let mut changed = false;

        changed |= ui
            .add(egui::Slider::new(&mut tone.volume, 0..=127).text("Volume"))
            .changed();
        changed |= ui
            .add(egui::Slider::new(&mut tone.pan, 0..=127).text("Pan"))
            .changed();
        changed |= ui
            .add(egui::Slider::new(&mut tone.modulation, 0..=127).text("Modulation"))
            .changed();
        let mut sustain = tone.sustain >= 64;
        changed |= ui.checkbox(&mut sustain, "Sustain (hold pedal)").changed();
        tone.sustain = if sustain { 127 } else { 0 };
        changed |= ui
            .add(egui::Slider::new(&mut tone.reverb, 0..=127).text("Reverb"))
            .changed();
        changed |= ui
            .add(egui::Slider::new(&mut tone.chorus, 0..=127).text("Chorus"))
            .changed();

        if changed {
            self.instrument_tones.insert(track_id, tone);
            self.send_instrument_cc(track_id, 7, tone.volume);
            self.send_instrument_cc(track_id, 10, tone.pan);
            self.send_instrument_cc(track_id, 1, tone.modulation);
            self.send_instrument_cc(track_id, 64, tone.sustain);
            self.send_instrument_cc(track_id, 91, tone.reverb);
            self.send_instrument_cc(track_id, 93, tone.chorus);
            self.dirty = true;
        }
    }

    fn draw_mini_keyboard(&mut self, ui: &mut egui::Ui) {
        ui.label(egui::RichText::new("MINI KEYBOARD").size(10.0).color(STUDIO_DIM));
        ui.horizontal(|ui| {
            if ui.small_button("Octave -").on_hover_text("Shift the keyboard down one octave").clicked() {
                self.keyboard_octave = (self.keyboard_octave - 1).clamp(-2, 3);
            }
            ui.label(format!("C{}", 4 + self.keyboard_octave));
            if ui.small_button("Octave +").on_hover_text("Shift the keyboard up one octave").clicked() {
                self.keyboard_octave = (self.keyboard_octave + 1).clamp(-2, 3);
            }
            ui.weak("Hold a key to play. Computer keys: Z X C V B N M , . / (white) · A D G H J L ; (black) · [ ] octave.");
        });

        let base: i32 = 60 + i32::from(self.keyboard_octave) * 12;
        let white: [u8; 14] = [0, 2, 4, 5, 7, 9, 11, 12, 14, 16, 17, 19, 21, 23];
        let black: [(u8, u8); 10] = [
            (1, 0), (3, 1), (6, 3), (8, 4), (10, 5),
            (13, 7), (15, 8), (18, 10), (20, 11), (22, 12),
        ];
        let width = ui.available_width().min(760.0);
        let key_h = 120.0;
        let white_w = width / white.len() as f32;
        let (area, _) = ui.allocate_exact_size(egui::Vec2::new(width, key_h + 10.0), egui::Sense::hover());
        let painter = ui.painter_at(area);

        let mut white_rects = Vec::with_capacity(white.len());
        for (index, _) in white.iter().enumerate() {
            white_rects.push(egui::Rect::from_min_size(
                egui::Pos2::new(area.left() + index as f32 * white_w, area.top()),
                egui::Vec2::new(white_w, key_h),
            ));
        }
        for (index, rect) in white_rects.iter().enumerate() {
            let note = (base + white[index] as i32) as u8;
            let active = self.keyboard_mouse_note.is_some_and(|(_, held)| held == note);
            painter.rect_filled(*rect, 3.0, if active { ACCENT_TINT } else { Color32::from_gray(230) });
            painter.rect_stroke(*rect, 3.0, egui::Stroke::new(1.0_f32, STUDIO_BORDER), egui::StrokeKind::Inside);
            if white[index].is_multiple_of(12) {
                painter.text(
                    rect.center_bottom() - egui::Vec2::new(0.0, 18.0),
                    egui::Align2::CENTER_CENTER,
                    format!("C{}", 4 + i32::from(self.keyboard_octave) + i32::from(white[index]) / 12),
                    egui::FontId::proportional(10.0),
                    STUDIO_DIM,
                );
            }
        }
        let black_w = white_w * 0.62;
        let black_h = key_h * 0.62;
        let mut black_rects = Vec::with_capacity(black.len());
        for (_semitone, before) in black.iter() {
            let x = area.left() + *before as f32 * white_w + white_w - black_w / 2.0;
            black_rects.push(egui::Rect::from_min_size(
                egui::Pos2::new(x, area.top()),
                egui::Vec2::new(black_w, black_h),
            ));
        }
        for (index, rect) in black_rects.iter().enumerate() {
            let note = (base + black[index].0 as i32) as u8;
            let active = self.keyboard_mouse_note.is_some_and(|(_, held)| held == note);
            painter.rect_filled(*rect, 2.0, if active { ACCENT_BRIGHT } else { Color32::from_gray(18) });
            painter.rect_stroke(*rect, 2.0, egui::Stroke::new(1.0_f32, Color32::BLACK), egui::StrokeKind::Inside);
        }

        // Hit-test black keys last so they sit on top of the white keys.
        let mut mouse_note: Option<u8> = None;
        for (index, rect) in white_rects.iter().enumerate() {
            let note = (base + white[index] as i32) as u8;
            let response = ui.interact(*rect, ui.id().with(("sf_white", index)), egui::Sense::click_and_drag());
            if response.is_pointer_button_down_on() {
                mouse_note = Some(note);
            }
        }
        for (index, rect) in black_rects.iter().enumerate() {
            let note = (base + black[index].0 as i32) as u8;
            let response = ui.interact(*rect, ui.id().with(("sf_black", index)), egui::Sense::click_and_drag());
            if response.is_pointer_button_down_on() {
                mouse_note = Some(note);
            }
        }

        if self.keyboard_mouse_note.map(|(_, note)| note) != mouse_note {
            if let Some(previous) = self.keyboard_mouse_note.take() {
                self.stop_audition_note(previous);
            }
            self.keyboard_mouse_note = mouse_note.and_then(|note| self.start_audition_note(note));
        }
    }

    fn computer_keyboard(&mut self, ctx: &egui::Context) {
        let rack_visible = self.show_devices && self.device_tab == DeviceTab::Instrument;
        let active = rack_visible && !ctx.wants_keyboard_input() && ctx.input(|input| {
            input.focused && !input.modifiers.any()
        });
        let released = self.keyboard_held.keys().copied()
            .filter(|key| !active || !ctx.input(|input| input.key_down(*key)))
            .collect::<Vec<_>>();
        for key in released {
            if let Some(note) = self.keyboard_held.remove(&key) {
                self.stop_audition_note(note);
            }
        }
        if (!rack_visible || !ctx.input(|input| input.focused))
            && let Some(note) = self.keyboard_mouse_note.take()
        {
            self.stop_audition_note(note);
        }
        if !active {
            return;
        }
        if ctx.input(|i| i.key_pressed(egui::Key::OpenBracket)) {
            self.keyboard_octave = (self.keyboard_octave - 1).clamp(-2, 3);
        }
        if ctx.input(|i| i.key_pressed(egui::Key::CloseBracket)) {
            self.keyboard_octave = (self.keyboard_octave + 1).clamp(-2, 3);
        }
        const KEYS: [egui::Key; 17] = [
            egui::Key::Z,
            egui::Key::X,
            egui::Key::C,
            egui::Key::V,
            egui::Key::B,
            egui::Key::N,
            egui::Key::M,
            egui::Key::Comma,
            egui::Key::Period,
            egui::Key::Slash,
            egui::Key::A,
            egui::Key::D,
            egui::Key::G,
            egui::Key::H,
            egui::Key::J,
            egui::Key::L,
            egui::Key::Semicolon,
        ];
        let base: i32 = 60 + i32::from(self.keyboard_octave) * 12;
        for key in KEYS {
            let Some(semitone) = computer_key_semitone(key) else {
                continue;
            };
            let note = (base + i32::from(semitone)) as u8;
            let pressed = ctx.input(|input| input.events.iter().any(|event| {
                matches!(event, egui::Event::Key { key: pressed, pressed: true, repeat: false, .. }
                    if *pressed == key)
            }));
            if pressed && !self.keyboard_held.contains_key(&key)
                && let Some(held) = self.start_audition_note(note)
            {
                self.keyboard_held.insert(key, held);
            }
        }
    }

    fn rescan_soundfonts(&mut self) {
        self.soundfont_ready = false;
        self.soundfont_banks.clear();
        self.discover_soundfonts();
        self.load_project_soundfonts();
    }

    fn assign_preset_to_selected_or_new(&mut self, preset: InstrumentPreset) {
        let Some(bank) = self.default_bank_name() else {
            self.error = Some("Install a SoundFont bank before assigning a preset.".into());
            return;
        };
        let track_id = match self.selected_track.filter(|id| {
            self.project
                .tracks
                .iter()
                .any(|track| track.id == *id && track.kind == TrackKind::Midi)
        }) {
            Some(id) => id,
            None => {
                self.add_midi_track();
                let Some(id) = self.selected_track else { return };
                id
            }
        };
        self.assign_instrument(track_id, &bank, preset);
        self.device_tab = DeviceTab::Instrument;
        self.show_devices = true;
    }

    fn instrument_rack_browser(
        &self,
        ui: &mut egui::Ui,
        query: &str,
        add: &mut Option<InstrumentPreset>,
        rescan: &mut bool,
    ) {
        if self.soundfont_banks.is_empty() {
            ui.colored_label(SUNSET, "No SoundFont bank installed.");
            ui.weak("Drop a .sf2 into assets/soundfonts/ (see its README) and rescan.");
            if ui.button("Rescan SoundFonts").clicked() {
                *rescan = true;
            }
            return;
        }
        let query = query.to_lowercase();
        for category in InstrumentCategory::ALL {
            let presets: Vec<InstrumentPreset> = PRESETS
                .iter()
                .copied()
                .filter(|preset| preset.category == category)
                .filter(|preset| {
                    query.is_empty()
                        || preset.name.to_lowercase().contains(&query)
                        || category.label().to_lowercase().contains(&query)
                })
                .collect();
            if presets.is_empty() {
                continue;
            }
            egui::CollapsingHeader::new(category.label())
                .id_salt(("sf_category", category as usize))
                .default_open(category == InstrumentCategory::PianosKeys)
                .show(ui, |ui| {
                    for preset in presets {
                        let label = if preset.is_drum() {
                            format!("{}   ·   drum kit", preset.name)
                        } else {
                            preset.name.to_string()
                        };
                        if ui
                            .button(egui::RichText::new(label).size(11.5))
                            .on_hover_text(format!(
                                "Assign {} (GM program {}) to a MIDI track",
                                preset.name, preset.program
                            ))
                            .clicked()
                        {
                            *add = Some(preset);
                        }
                    }
                });
        }
    }
}

/// Headless SoundFont verification: load a `.sf2`, play a short note on the
/// Acoustic Piano preset, and report the rendered peak. Never touches the audio
/// device; used by `daw-ui --probe-soundfont PATH`.
pub(crate) fn probe_soundfont(path: &Path, sample_rate: u32) -> Result<f32, String> {
    let bank = load_sound_font(path)?;
    let mut synth = build_synth_with_tone(&bank, sample_rate, 0, 0, SoundFontTone::default())?;
    synth.note_on(0, 60, 100);
    let block = 256_usize;
    let mut left = vec![0.0_f32; block];
    let mut right = vec![0.0_f32; block];
    let mut peak = 0.0_f32;
    for _ in 0..(sample_rate as usize * 2 / block) {
        synth.render(&mut left, &mut right);
        for (&l, &r) in left.iter().zip(right.iter()) {
            peak = peak.max(l.abs()).max(r.abs());
        }
    }
    synth.note_off(0, 60);
    if !peak.is_finite() || peak <= 0.001 {
        return Err(format!("rendered peak {peak} too low; is the bank a valid GM SoundFont?"));
    }
    Ok(peak)
}

/// Computer-keyboard note map: key -> semitone offset from the current octave
/// base (C4 = 60). Keeps `S` and `Q` free for the arrangement shortcuts.
pub(crate) fn computer_key_semitone(key: egui::Key) -> Option<u8> {
    match key {
        egui::Key::Z => Some(0),
        egui::Key::X => Some(2),
        egui::Key::C => Some(4),
        egui::Key::V => Some(5),
        egui::Key::B => Some(7),
        egui::Key::N => Some(9),
        egui::Key::M => Some(11),
        egui::Key::Comma => Some(12),
        egui::Key::Period => Some(14),
        egui::Key::Slash => Some(16),
        egui::Key::A => Some(1),
        egui::Key::D => Some(3),
        egui::Key::G => Some(6),
        egui::Key::H => Some(8),
        egui::Key::J => Some(10),
        egui::Key::L => Some(13),
        egui::Key::Semicolon => Some(15),
        _ => None,
    }
}

#[cfg(test)]
mod instrument_rack_tests {
    use super::*;

    #[test]
    fn computer_key_map_is_conflict_free_and_octave_anchored() {
        // S and Q stay free for split/snap shortcuts.
        assert_eq!(computer_key_semitone(egui::Key::S), None);
        assert_eq!(computer_key_semitone(egui::Key::Q), None);
        assert_eq!(computer_key_semitone(egui::Key::Z), Some(0));
        assert_eq!(computer_key_semitone(egui::Key::A), Some(1)); // C#4
        assert_eq!(computer_key_semitone(egui::Key::Slash), Some(16));
    }

    #[test]
    fn installed_bank_preserves_unassigned_track_and_resolves_saved_assignment() {
        let mut studio = Studio::with_sample_rate(48_000);
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../daw-engine/tests/fixtures/qa-two-programs.sf2");
        let bank = Studio::soundfont_bank_name(&fixture);
        studio.soundfont_banks.push(fixture.clone());
        studio.soundfonts.insert(bank.clone(), load_sound_font(&fixture).unwrap());
        assert!(studio.track_instrument(&studio.project.tracks[0]).is_none());
        studio.assign_instrument(1, &bank, PRESETS[1]);
        assert_eq!(studio.track_instrument(&studio.project.tracks[0]).unwrap().1, PRESETS[1]);
    }

    #[test]
    fn soundfont_bank_name_uses_file_name() {
        assert_eq!(
            Studio::soundfont_bank_name(Path::new("/a/b/GeneralUser GS v1.471.sf2")),
            "GeneralUser GS v1.471.sf2"
        );
    }
}
