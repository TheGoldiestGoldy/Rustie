// Metadata is supplied by the existing isolated host, never by loading a library in egui.
#[derive(Debug, PartialEq)]
struct PluginClass {
    id: String,
    name: String,
    vendor: String,
    category: String,
}

impl PluginClass {
    // Only exclude an explicitly incompatible role. Unknown/custom categories remain selectable.
    fn supports_track(&self, midi: bool) -> bool {
        let tags = self.category.split([',', '|']).map(str::trim);
        let synth = tags
            .clone()
            .any(|tag| tag.eq_ignore_ascii_case("instrument"));
        let effect = tags
            .clone()
            .any(|tag| tag.eq_ignore_ascii_case("audio-effect") || tag.eq_ignore_ascii_case("fx"));
        if midi {
            synth || !effect
        } else {
            effect || !synth
        }
    }

    fn parse(line: &str) -> Option<Self> {
        let fields: HashMap<_, _> = line
            .split('\t')
            .filter_map(|field| field.split_once('='))
            .collect();
        let (id_key, category_key) = match *fields.get("format")? {
            "clap" => ("id", "features"),
            "vst3" => ("uid", "category"),
            _ => return None,
        };
        let id = *fields.get(id_key)?;
        if id.is_empty() {
            return None;
        }
        Some(Self {
            id: id.into(),
            name: fields
                .get("name")
                .copied()
                .filter(|s| !s.is_empty())
                .unwrap_or(id)
                .into(),
            vendor: fields.get("vendor").copied().unwrap_or_default().into(),
            category: fields.get(category_key).copied().unwrap_or_default().into(),
        })
    }
}

impl Studio {
    fn select_plugin_bundle(&mut self, path: PathBuf) {
        if self.plugin_selected.as_ref() != Some(&path) {
            self.plugin_selected = Some(path);
            self.inspect_plugin_bundle();
        }
    }

    fn inspect_plugin_bundle(&mut self) {
        self.plugin_classes.clear();
        self.selected_class_id = None;
        self.plugin_scan_error = None;
        // Keep at most one host process in flight. Polling starts the newest
        // selection after an older inspection finishes.
        if self.plugin_scan_job.is_some() {
            return;
        }
        let Some(path) = self.plugin_selected.clone() else {
            return;
        };
        let command = match path.extension().and_then(|ext| ext.to_str()) {
            Some("clap") => "inspect-clap",
            Some("vst3") => "inspect-vst3",
            _ => return,
        };
        let worker = match crate::plugin_worker_path() {
            Ok(worker) if worker.is_file() => worker,
            Ok(_) => {
                self.plugin_scan_error = Some("The plug-in inspection helper is missing.".into());
                return;
            }
            Err(error) => {
                self.plugin_scan_error = Some(error);
                return;
            }
        };
        let (tx, rx) = mpsc::channel();
        let scan_path = path.clone();
        let start = std::thread::Builder::new().spawn(move || {
            let result = Command::new("timeout")
                .args(["-k", "2s", "15s"])
                .arg(worker)
                .arg(command)
                .arg(path)
                .output()
                .map_err(|error| format!("Could not start plug-in inspection ({error})"))
                .and_then(|output| {
                    if !output.status.success() {
                        let reason = String::from_utf8_lossy(&output.stderr);
                        return Err(format!(
                            "Inspection failed ({}). {}",
                            output.status,
                            reason.trim()
                        ));
                    }
                    let classes = String::from_utf8_lossy(&output.stdout)
                        .lines()
                        .filter_map(PluginClass::parse)
                        .collect::<Vec<_>>();
                    if classes.is_empty() {
                        Err("The host reported no usable classes.".into())
                    } else {
                        Ok(classes)
                    }
                });
            let _ = tx.send(result);
        });
        match start {
            Ok(_) => self.plugin_scan_job = Some((scan_path, rx)),
            Err(error) => self.plugin_scan_error = Some(format!("Could not start inspection: {error}")),
        }
    }

    /// Keep a freshly inspected plug-in actionable instead of leaving its
    /// assign button disabled only because the previously selected track has
    /// the opposite role (instrument versus effect).
    fn select_compatible_track_for_plugin(&mut self) {
        let Some(class) = self
            .plugin_classes
            .iter()
            .find(|class| Some(&class.id) == self.selected_class_id.as_ref())
        else {
            return;
        };
        let supported_track = |track: &&daw_core::Track| {
            matches!(track.kind, TrackKind::Audio | TrackKind::Group | TrackKind::Midi)
                && class.supports_track(track.kind == TrackKind::Midi)
        };
        if self
            .selected_track
            .and_then(|id| self.project.tracks.iter().find(|track| track.id == id))
            .as_ref()
            .is_some_and(supported_track)
        {
            return;
        }
        let compatible = self
            .project
            .tracks
            .iter()
            .find(supported_track)
            .map(|track| (track.id, track.name.clone()));
        if let Some((id, name)) = compatible {
            self.selected_track = Some(id);
            self.selected_clip = None;
            self.status = format!("Plug-in target: {name}");
        }
    }

    fn poll_plugin_scan(&mut self) {
        let Some((path, job)) = &self.plugin_scan_job else {
            return;
        };
        let result = match job.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => return,
            Err(mpsc::TryRecvError::Disconnected) => Err("Plug-in inspection stopped.".into()),
        };
        let is_current = self.plugin_selected.as_ref() == Some(path);
        self.plugin_scan_job = None;
        if !is_current {
            self.inspect_plugin_bundle();
            return;
        }
        match result {
            Ok(classes) => {
                self.selected_class_id = classes.first().map(|class| class.id.clone());
                self.plugin_classes = classes;
                self.select_compatible_track_for_plugin();
            }
            Err(error) => self.plugin_scan_error = Some(error),
        }
    }
}

#[cfg(test)]
mod plugin_discovery_tests {
    use super::*;
    #[test]
    fn finished_inspection_cannot_replace_a_newer_bundle_selection() {
        let creation = eframe::CreationContext::_new_kittest(egui::Context::default());
        let mut studio = Studio::new(&creation, None);
        let previous = PathBuf::from("previous.clap");
        let next = PathBuf::from("next.so");
        let (sender, receiver) = mpsc::channel();
        studio.plugin_selected = Some(previous.clone());
        studio.plugin_scan_job = Some((previous, receiver));

        studio.select_plugin_bundle(next.clone());
        assert!(studio.plugin_scan_job.is_some());
        sender.send(Err("Previous plug-in failed".into())).unwrap();
        studio.poll_plugin_scan();

        assert_eq!(studio.plugin_selected, Some(next));
        assert!(studio.plugin_classes.is_empty());
        assert!(studio.plugin_scan_error.is_none());
        assert!(studio.plugin_scan_job.is_none());
    }

    #[test]
    fn uses_host_identity_and_metadata_for_each_format() {
        let clap = PluginClass::parse(
            "format=clap\tid=org.test\tname=Tone\tvendor=Test\tfeatures=instrument,synthesizer",
        )
        .unwrap();
        assert_eq!(
            (
                clap.id.as_str(),
                clap.vendor.as_str(),
                clap.category.as_str()
            ),
            ("org.test", "Test", "instrument,synthesizer")
        );
        let vst = PluginClass::parse("format=vst3\tuid=123\tname=Echo\tcategory=Fx|Delay").unwrap();
        assert_eq!(
            (vst.id.as_str(), vst.name.as_str(), vst.category.as_str()),
            ("123", "Echo", "Fx|Delay")
        );
        assert!(clap.supports_track(true));
        assert!(!clap.supports_track(false));
        assert!(vst.supports_track(false));
        assert!(!vst.supports_track(true));
        assert!(PluginClass::parse("format=clap\tid=\tname=bad").is_none());
    }

    #[test]
    fn inspected_plugin_selects_an_existing_compatible_track() {
        let creation = eframe::CreationContext::_new_kittest(egui::Context::default());
        let mut studio = Studio::new(&creation, None);
        studio.add_audio_track();
        let audio_id = studio.selected_track.unwrap();

        studio.plugin_classes = vec![PluginClass {
            id: "instrument".into(),
            name: "Instrument".into(),
            vendor: String::new(),
            category: "instrument".into(),
        }];
        studio.selected_class_id = Some("instrument".into());
        studio.select_compatible_track_for_plugin();
        assert!(studio.selected_track.is_some_and(|id| {
            studio
                .project
                .tracks
                .iter()
                .find(|track| track.id == id)
                .is_some_and(|track| track.kind == TrackKind::Midi)
        }));

        studio.plugin_classes = vec![PluginClass {
            id: "effect".into(),
            name: "Effect".into(),
            vendor: String::new(),
            category: "audio-effect".into(),
        }];
        studio.selected_class_id = Some("effect".into());
        studio.select_compatible_track_for_plugin();
        assert_eq!(studio.selected_track, Some(audio_id));
    }
}
