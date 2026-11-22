// Original Slate Studio asset browser. No third-party plugin code is loaded here.
// Search vendor subdirectories but treat a bundle as a single entry. Never descend
// into symlinked directories. An explicitly linked bundle can still be selected.
fn discover_plugin_bundles(root: &Path, extension: &str, found: &mut Vec<PathBuf>) {
    let mut pending = vec![(root.to_owned(), 0)];
    let mut directories = 0;
    while let Some((folder, depth)) = pending.pop() {
        directories += 1;
        if directories > 128 { break; }
        let Ok(entries) = fs::read_dir(folder) else { continue };
        for entry in entries.flatten().take(800) {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == extension) {
                found.push(path);
            } else if depth < 3 && entry.file_type().is_ok_and(|kind| kind.is_dir())
                && !entry.file_name().to_string_lossy().starts_with('.') {
                pending.push((path, depth + 1));
            }
        }
    }
}

impl Studio {
    fn refresh_library(&mut self) {
        self.sample_paths.clear();
        self.plugin_paths.clear();

        // Bounded discovery: a giant Downloads folder must not freeze the GUI.
        let mut pending = self.sample_roots.iter().cloned()
            .map(|path| (path, 0_usize)).collect::<Vec<_>>();
        let mut visited = 0_usize;
        while let Some((dir, depth)) = pending.pop() {
            if visited >= 180 || self.sample_paths.len() >= 2400 {
                break;
            }
            visited += 1;
            let Ok(entries) = fs::read_dir(&dir) else { continue };
            for entry in entries.flatten().take(800) {
                let path = entry.path();
                let Ok(kind) = entry.file_type() else { continue };
                if kind.is_symlink() { continue; }
                if kind.is_dir() && depth < 3 {
                    if !entry.file_name().to_string_lossy().starts_with('.') {
                        pending.push((path, depth + 1));
                    }
                } else if kind.is_file() && path.extension().and_then(|e| e.to_str())
                    .is_some_and(|e| matches!(e.to_ascii_lowercase().as_str(),
                        "wav" | "aif" | "aiff" | "flac" | "mp3" | "ogg" | "m4a"))
                {
                    self.sample_paths.push(path);
                    if self.sample_paths.len() >= 2400 { break; }
                }
            }
        }
        self.sample_paths.sort();
        self.sample_paths.dedup();
        let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
        for (format_dir, extension) in [("clap", "clap"), ("vst3", "vst3"), ("lv2", "lv2"), ("vst", "so")] {
            let roots = [home.join(format!(".{format_dir}")), Path::new("/usr/lib").join(format_dir),
                Path::new("/usr/local/lib").join(format_dir)];
            for root in roots { discover_plugin_bundles(&root, extension, &mut self.plugin_paths); }
        }
        if let Some(paths) = std::env::var_os("CLAP_PATH") {
            for root in std::env::split_paths(&paths).filter(|p| !p.as_os_str().is_empty()) {
                discover_plugin_bundles(&root, "clap", &mut self.plugin_paths);
            }
        }
        self.plugin_paths.sort();
        self.plugin_paths.dedup();
        self.browser_ready = true;
    }

    fn studio_browser(&mut self, ctx: &egui::Context) {
        if !self.browser_ready { self.refresh_library(); }
        let mut import: Option<PathBuf> = None;
        let mut add_folder = false;
        let mut add_instrument: Option<InstrumentPreset> = None;
        let mut rescan_soundfonts = false;
        let mut add_fx = None;
        let mut refresh = false;
        let mut select_plugin = None;
        let query = self.search.trim().to_lowercase();

        egui::SidePanel::left("studio_browser")
            .frame(egui::Frame::default()
                .fill(INK_1)
                .stroke(Stroke::new(0.8_f32, STUDIO_BORDER))
                .inner_margin(egui::Margin::symmetric(13, 12)))
            // Video-confirmed drift: egui used the content-measured panel width
            // as its NEXT frame's persisted width. Clamp it explicitly so text
            // and nested scrolling content cannot slowly push the DAW right.
            // Width changes only when the user presses the header +/- buttons.
            .exact_width(self.browser_width).resizable(false)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("LIBRARY")
                        .strong().size(11.0).color(STUDIO_DIM));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("↻").on_hover_text("Rescan library").clicked() {
                            refresh = true;
                        }
                        if ui.small_button("›").on_hover_text("Widen library").clicked() {
                            self.browser_width = (self.browser_width + 24.0).min(350.0);
                        }
                        if ui.small_button("‹").on_hover_text("Narrow library").clicked() {
                            self.browser_width = (self.browser_width - 24.0).max(220.0);
                        }
                    });
                });
                ui.add_space(8.0);
                ui.add(egui::TextEdit::singleline(&mut self.search)
                    .hint_text("Search this category  ·  Ctrl+F")
                    .id(egui::Id::new("source_search"))
                    .desired_width(f32::INFINITY)).on_hover_text("Filter the selected library category. Choose All content to search across categories.");
                ui.add_space(12.0);

                let sections = [
                    (BrowserSection::Instruments, "Instruments"),
                    (BrowserSection::Effects, "Effects"),
                    (BrowserSection::Plugins, "Plugins"),
                    (BrowserSection::Samples, "Samples"),
                    (BrowserSection::Places, "Places"),
                    (BrowserSection::All, "All content"),
                ];
                for (value, title) in sections {
                    let selected = self.browser_section == value;
                    let label = egui::RichText::new(title)
                        .size(12.0)
                        .color(if selected { ACCENT_BRIGHT } else { STUDIO_TEXT });
                    if ui.add_sized([ui.available_width(), 25.0],
                        egui::Button::new(label)
                            .fill(if selected { ACCENT_TINT } else { INK_1 })
                            .stroke(Stroke::new(0.8_f32,
                                if selected { ACCENT_DARK } else { INK_1 })))
                        .on_hover_text(format!("Browse {title}"))
                        .clicked() {
                        self.browser_section = value;
                    }
                }
                ui.add_space(10.0);
                ui.separator();
                ui.add_space(6.0);

                let section = self.browser_section;
                let show_instruments = matches!(section,
                    BrowserSection::All | BrowserSection::Instruments);
                let show_fx = matches!(section, BrowserSection::All | BrowserSection::Effects);
                let show_plugins = matches!(section, BrowserSection::All | BrowserSection::Plugins);
                let show_samples = matches!(section, BrowserSection::All | BrowserSection::Samples);
                let show_places = matches!(section, BrowserSection::All | BrowserSection::Places);

                let content_height = ui.available_height();
                egui::ScrollArea::vertical()
                    .id_salt("slate_browser_scroll")
                    .max_height(content_height)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        // The nested scroll content must never advertise a
                        // wider minimum than the sidebar's allocated viewport.
                        ui.set_max_width(ui.available_width());
                        if show_instruments {
                            ui.label(egui::RichText::new("INSTRUMENTS")
                                .size(10.0).strong().color(STUDIO_DIM));
                            self.instrument_rack_browser(
                                ui,
                                &query,
                                &mut add_instrument,
                                &mut rescan_soundfonts,
                            );
                            ui.add_space(10.0);
                        }
                        if show_fx {
                            ui.label(egui::RichText::new("EFFECTS")
                                .size(10.0).strong().color(STUDIO_DIM));
                            for kind in BUILTIN_EFFECTS {
                                if format!("{} {}", kind.label(), effect_description(kind)).to_lowercase().contains(&query)
                                    && ui.add_enabled(self.can_add_effect(), egui::Button::new(format!("+  {}", kind.label())))
                                        .on_hover_text(format!("{} · Add to selected track", effect_description(kind)))
                                        .on_disabled_hover_text("Select a track with fewer than eight effects and finish any active take.")
                                        .clicked() {
                                    add_fx = Some(kind);
                                }
                            }
                            ui.add_space(10.0);
                        }
                        if show_plugins {
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new("INSTALLED PLUGINS")
                                    .size(10.0).strong().color(STUDIO_DIM));
                                ui.weak(self.plugin_paths.len().to_string());
                            });
                            let mut matches = self.plugin_paths.iter().filter(|p| p.file_name()
                                .unwrap_or_default().to_string_lossy()
                                .to_lowercase().contains(&query));
                            for path in matches.by_ref().take(200) {
                                let label = path.file_stem().unwrap_or_default().to_string_lossy();
                                let (format, hint, usable) = match path.extension().and_then(|e| e.to_str()) {
                                    Some("clap") => ("CLAP", "Inspect classes for a live insert or offline render", true),
                                    Some("vst3") => ("VST3", "Inspect the VST3 bundle for a live insert or offline render", true),
                                    Some("so") => ("VST2", "Render a selected audio or MIDI clip to WAV", true),
                                    _ => ("LV2", "Bundle discovered. This format cannot be loaded by this host.", false),
                                };
                                if ui.add_enabled(usable, egui::Button::new(format!("{format}  ·  {label}")).truncate())
                                    .on_hover_text(hint).on_disabled_hover_text(hint).clicked() {
                                    select_plugin = Some(path.clone());
                                }
                            }
                            if matches.next().is_some() {
                                ui.small("Showing the first 200 matches. Refine the search for more.");
                            }
                            if self.plugin_paths.is_empty() {
                                ui.label(egui::RichText::new("No CLAP / VST3 / LV2 plugins found")
                                    .size(11.0).color(STUDIO_DIM));
                            }
                            ui.add_space(10.0);
                        }
                        if show_samples {
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new("AUDIO SAMPLES")
                                    .size(10.0).strong().color(STUDIO_DIM));
                                ui.weak(self.sample_paths.len().to_string());
                            });
                            for path in self.sample_paths.iter().filter(|p| p.display()
                                .to_string().to_lowercase().contains(&query)).take(500) {
                                let name = path.file_name().unwrap_or_default().to_string_lossy();
                                if ui.add(egui::Button::new(
                                    egui::RichText::new(name.as_ref()).size(11.0)).truncate()
                                    .frame(false))
                                    .on_hover_text(format!("Import: {}", path.display()))
                                    .clicked() { import = Some(path.clone()); }
                            }
                            if self.sample_paths.is_empty() {
                                ui.label(egui::RichText::new("Add a folder to discover audio")
                                    .size(11.0).color(STUDIO_DIM));
                            }
                            ui.add_space(10.0);
                        }
                        if show_places {
                            ui.label(egui::RichText::new("PLACES")
                                .size(10.0).strong().color(STUDIO_DIM));
                            for path in &self.sample_roots {
                                if query.is_empty() || path.display().to_string()
                                    .to_lowercase().contains(&query) {
                                    ui.label(egui::RichText::new(format!("▱  {}",
                                        path.file_name().unwrap_or_default().to_string_lossy()))
                                        .size(11.0).color(STUDIO_TEXT))
                                        .on_hover_text(path.display().to_string());
                                }
                            }
                            ui.add_space(5.0);
                            ui.horizontal_wrapped(|ui| {
                                if ui.small_button("+ Folder").on_hover_text("Index a folder of samples").clicked() { add_folder = true; }
                                if ui.small_button("+ Audio file").on_hover_text("Import a file onto an audio track").clicked() {
                                    import = Self::pick_path(false, true);
                                }
                            });
                        }
                    });
            });

        if add_folder
            && let Ok(output) = Command::new("zenity")
                .args(["--file-selection", "--directory", "--title=Choose sample folder"])
                .output()
                && output.status.success()
            {
                let path = PathBuf::from(String::from_utf8_lossy(&output.stdout).trim());
                if path.is_dir() && !self.sample_roots.contains(&path) {
                    self.sample_roots.push(path);
                    refresh = true;
                }
        }
        if refresh { self.refresh_library(); }
        if rescan_soundfonts { self.rescan_soundfonts(); }
        if let Some(preset) = add_instrument {
            self.assign_preset_to_selected_or_new(preset);
        }
        if let Some(kind) = add_fx { self.add_builtin_effect(kind); }
        if let Some(path) = select_plugin {
            self.select_plugin_bundle(path);
            self.device_tab = DeviceTab::Plugins;
            self.show_devices = true;

        }
        if let Some(path) = import { self.import_audio(path); }
    }
}

#[cfg(test)]
mod browser_geometry_tests {
    use super::*;

    #[test]
    fn discovery_finds_vendor_bundles_and_stops_at_bundle_boundaries() {
        let folder = std::env::temp_dir().join(format!("rustie-test-discovery-{}-{}", std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let bundle = folder.join("vendor/Delay.vst3");
        fs::create_dir_all(bundle.join("Contents/Inner.vst3")).unwrap();
        std::os::unix::fs::symlink(&folder, folder.join("vendor/loop")).unwrap();
        let mut paths = Vec::new();
        discover_plugin_bundles(&folder, "vst3", &mut paths);
        fs::remove_dir_all(folder).unwrap();
        assert_eq!(paths, vec![bundle]);
    }

    /// Regression from the 2026-11-18 screencast: the LIBRARY boundary moved
    /// right despite no click/resize. Never let text/layout persist a larger
    /// panel width between frames when the user has not requested resizing.
    #[test]
    fn browser_does_not_grow_over_idle_frames() {
        let ctx = egui::Context::default();
        let creation = eframe::CreationContext::_new_kittest(ctx.clone());
        let mut studio = Studio::new(&creation, None);
        studio.browser_ready = true;
        studio.plugin_paths = vec![PathBuf::from(format!("{}.clap", "LongBundleName".repeat(16)))];
        studio.sample_paths = vec![PathBuf::from(format!("{}.wav", "Loop".repeat(35)))];
        studio.browser_section = BrowserSection::All;
        for (width, height) in [(980.0, 640.0), (1540.0, 940.0)] {
            let mut first = None;
            for _ in 0..120 {
                let _ = ctx.run(egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(width, height))),
                    ..Default::default()
                }, |ctx| {
                    studio.studio_browser(ctx);
                    let boundary = ctx.available_rect().left();
                    assert!((studio.browser_width - boundary).abs() <= 2.0, "sidebar edge {boundary}");
                    assert_eq!(*first.get_or_insert(boundary), boundary);
                });
            }
        }
    }
}
