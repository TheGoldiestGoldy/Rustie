// Non-realtime full arrangement mixdown. Writing WAV is done on its own thread,
// without using an audio hardware output or changing user audio routing.
use std::sync::mpsc as export_mpsc;

fn render_mix_to_wav(
    mut graph: daw_engine::AudioGraph,
    events: &[(u64, EngineCommand)],
    frames: u64,
    path: &Path,
    sample_rate: u32,
) -> Result<f32, String> {
    if frames == 0 || frames > sample_rate as u64 * 60 * 30 {
        return Err("Mix export length must be between one frame and 30 minutes".into());
    }
    let spec = hound::WavSpec { channels: 2, sample_rate, bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float };
    let mut wav = hound::WavWriter::create(path, spec)
        .map_err(|e| format!("Open mix export: {e}"))?;
    let mut next = 0;
    let mut peak = 0.0_f32;
    for frame in 0..frames {
        while next < events.len() && events[next].0 <= frame {
            match events[next].1 {
                EngineCommand::NoteOn { node, note, velocity } => graph.note_on(node, note, velocity),
                EngineCommand::NoteOff { node, note } => graph.note_off(node, note),
                EngineCommand::AllNotesOff { node } => graph.all_notes_off(node),
                _ => {}
            }
            next += 1;
        }
        let s = graph.render_frame(frame);
        peak = peak.max(s.left.abs()).max(s.right.abs());
        wav.write_sample(s.left).map_err(|e| format!("Write left mix: {e}"))?;
        wav.write_sample(s.right).map_err(|e| format!("Write right mix: {e}"))?;
    }
    wav.finalize().map_err(|e| format!("Finish mix WAV: {e}"))?;
    Ok(peak)
}

impl Studio {
    fn start_mix_export(&mut self) {
        if self.export_job.is_some() || self.recorder.is_some() {
            self.error = Some("Finish recording or the current export first".into());
            return;
        }
        if self.project.tracks.iter().any(|track| {
            track.live_clap.as_ref().is_some_and(|insert| insert.enabled)
        }) {
            self.error = Some("Arrangement export requires bypassing live plug-ins. Import printed audio first if the mix needs those parts.".into());
            return;
        }
        if let Some(instrument) = self.project.tracks.iter()
            .filter_map(|track| track.instrument.as_ref())
            .find(|instrument| !self.soundfonts.contains_key(&instrument.bank))
        {
            self.error = Some(format!("Arrangement export needs the assigned SoundFont bank: {}", instrument.bank));
            return;
        }
        let before = self.view;
        self.view = View::Arrangement;
        // Live subprocesses require realtime pacing; offline bounce must
        // never consume mis-timed plugin blocks.
        let compiled = self.compiled_graph_with_live(false);
        let events = if compiled.is_ok() { self.compiled_midi_events() } else { Vec::new() };
        self.view = before;
        let Some(graph) = self.apply_error(compiled) else { return };
        let length = ((self.max_project_seconds().max(0.2) + 0.5)
            * self.sample_rate as f64).ceil() as u64;
        if length > self.sample_rate as u64 * 60 * 30 {
            self.error = Some("Export limited to 30 minutes".into());
            return;
        }
        let directory = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default()
            .join("Music/Rusty Exports");
        if let Err(e) = fs::create_dir_all(&directory) {
            self.error = Some(format!("Export folder: {e}")); return;
        }
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH).map_or(0, |t| t.as_millis());
        let path = directory.join(format!("Mix-{timestamp}.wav"));
        let (tx, rx) = export_mpsc::channel();
        self.export_job = Some(rx);
        self.status = "Exporting full arrangement to stereo 32-bit float WAV…".into();
        let sample_rate = self.sample_rate;
        std::thread::spawn(move || {
            let result = render_mix_to_wav(graph, &events, length, &path, sample_rate)
                .map(|peak| (path.clone(), peak));
            if result.is_err() { let _ = fs::remove_file(path); }
            let _ = tx.send(result);
        });
    }

    fn poll_mix_export(&mut self) {
        if let Some(result) = self.export_job.as_ref().and_then(|rx| rx.try_recv().ok()) {
            self.export_job = None;
            match result {
                Ok((path, peak)) => {
                    self.status = format!("Mix exported: {} (peak {:.3})", path.display(), peak);
                    self.error = None;
                }
                Err(error) => {
                    self.error = Some(error);
                    self.status = "Mix export failed".into();
                }
            }
        }
    }
}

#[cfg(test)]
mod export_tests {
    use super::*;

    fn write_test_wav(path: &Path, channels: u16, frame: &[f32]) {
        let mut wav = hound::WavWriter::create(
            path,
            hound::WavSpec {
                channels,
                sample_rate: 48_000,
                bits_per_sample: 32,
                sample_format: hound::SampleFormat::Float,
            },
        )
        .unwrap();
        for _ in 0..512 {
            for &sample in frame {
                wav.write_sample(sample).unwrap();
            }
        }
        wav.finalize().unwrap();
    }

    #[test]
    fn reopened_take_keeps_routing_and_gain_independent_through_track_and_master() {
        let dir = std::env::temp_dir().join(format!("rusty-take-routing-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let stereo_take = dir.join("Stereo take.wav");
        let mono_take = dir.join("Mono take.wav");
        let stereo_import = dir.join("Stereo import.wav");
        let saved = dir.join("Routing.slate.json");
        let export = dir.join("Mix.wav");

        let sample = 0.03125;
        write_test_wav(&stereo_take, 2, &[sample, -0.001]);
        write_test_wav(&mono_take, 1, &[sample]);
        write_test_wav(&stereo_import, 2, &[0.25, -0.5]);

        let mut studio = Studio::with_sample_rate(48_000);
        studio.view = View::Arrangement;
        studio.project = Project::new("Track routing", Tempo::default());
        studio.selected_track = None;
        studio.import_audio(stereo_import.clone());
        let imported_id = studio.selected_track.unwrap();
        studio.project.track_mut(imported_id).unwrap().record_input_channels =
            daw_core::RecordInputChannels::Mono2;

        let take_id = studio.next_id;
        let take_clip_id = take_id + 1;
        let group_id = take_id + 2;
        let mut take = Track::new(take_id, "Record", TrackKind::Audio);
        take.solo = true;
        take.group_id = Some(group_id);
        take.clips.push(Clip {
            id: take_clip_id,
            start: MusicalTime::from_ticks(PPQ),
            duration: MusicalTime::from_ticks(PPQ),
            kind: ClipKind::Audio(AudioClip {
                source: stereo_take,
                warped: false,
                source_offset_frames: 0,
                gain: 1.0,
                fade_in_ms: 0,
                fade_out_ms: 0,
            }),
            scale_override: None,
        });
        studio.project.tracks.push(take);
        studio.project.tracks.push(Track::new(group_id, "Vocals bus", TrackKind::Group));

        let content = serde_json::to_vec(&studio.project).unwrap();
        atomic_save_project(&saved, &content).unwrap();
        studio.load_project_path(saved.clone());
        assert!(studio.error.is_none(), "{:?}", studio.error);

        let start = studio.tick_to_frame(PPQ);
        let mut stereo = studio.compiled_graph().unwrap();
        assert_eq!(stereo.render_frame(0), StereoFrame::ZERO);
        assert_eq!(stereo.render_frame(start - 1), StereoFrame::ZERO);
        let original = stereo.render_frame(start);
        assert_eq!((original.left, original.right), (sample, -0.001));

        let take = studio.project.track_mut(take_id).unwrap();
        take.record_input_channels = daw_core::RecordInputChannels::Mono1;
        let ClipKind::Audio(audio) = &mut take.clips[0].kind else {
            panic!("recorded clip is audio");
        };
        audio.source = mono_take.clone();
        atomic_save_project(&saved, &serde_json::to_vec(&studio.project).unwrap()).unwrap();
        studio.load_project_path(saved);
        assert!(studio.error.is_none(), "{:?}", studio.error);

        let mut mono = studio.compiled_graph().unwrap();
        let centered = mono.render_frame(start);
        assert_eq!(
            (centered.left, centered.right),
            (original.left, original.left)
        );

        studio.project.track_mut(take_id).unwrap().pan = 1.0;
        let mut panned_graph = studio.compiled_graph().unwrap();
        let panned = panned_graph.render_frame(start);
        assert_eq!((panned.left, panned.right), (0.0, sample));
        panned_graph.set_pan(Studio::clip_audio_node(take_clip_id).unwrap(), -1.0);
        let panned = panned_graph.render_frame(start + 1);
        assert_eq!((panned.left, panned.right), (sample, 0.0));
        studio.project.track_mut(take_id).unwrap().pan = 0.0;

        studio.project.track_mut(group_id).unwrap().volume = 0.5;
        let mut group_mix = studio.compiled_graph().unwrap();
        let grouped = group_mix.render_frame(start);
        assert_eq!((grouped.left, grouped.right), (sample * 0.5, sample * 0.5));
        studio.project.track_mut(group_id).unwrap().volume = 1.0;

        // Change level only after confirming channel routing at unity.
        let gain = 2.0;
        let take = studio.project.track_mut(take_id).unwrap();
        let ClipKind::Audio(audio) = &mut take.clips[0].kind else {
            panic!("recorded clip is audio");
        };
        audio.gain = gain;
        let mut louder = studio.compiled_graph().unwrap();
        let boosted = louder.render_frame(start);
        assert_eq!((boosted.left, boosted.right), (sample * gain, sample * gain));

        let peak = render_mix_to_wav(
            studio.compiled_graph_with_live(false).unwrap(),
            &[],
            start + 512,
            &export,
            48_000,
        )
        .unwrap();
        assert_eq!(peak, sample * gain);
        let mut rendered = hound::WavReader::open(&export).unwrap();
        let mut samples = rendered.samples::<f32>().skip((start * 2) as usize);
        assert_eq!(samples.next().unwrap().unwrap(), boosted.left);
        assert_eq!(samples.next().unwrap().unwrap(), boosted.right);
        let mut raw_take = hound::WavReader::open(mono_take).unwrap();
        assert_eq!(raw_take.samples::<f32>().next().unwrap().unwrap(), sample);

        studio.project.track_mut(take_id).unwrap().solo = false;
        studio.project.track_mut(imported_id).unwrap().solo = true;
        let mut imported = studio.compiled_graph().unwrap();
        let original_stereo = imported.render_frame(0);
        assert_eq!((original_stereo.left, original_stereo.right), (0.25, -0.5));
        assert_eq!(imported.render_frame(start), StereoFrame::ZERO);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn offline_mix_wav_contains_real_midi_notes_and_silence_outside_notes() {
        let mut builder = GraphBuilder::new(48_000);
        builder.add_node(NodeSpec::midi(500_001)).add_node(NodeSpec::master(MASTER_ID));
        builder.route(500_001, MASTER_ID, 1.0);
        let graph = builder.build().unwrap();
        let events = [(480_u64, EngineCommand::NoteOn { node: 500_001, note: 69, velocity: 100 }),
            (2500, EngineCommand::NoteOff { node: 500_001, note: 69 })];
        let file = std::env::temp_dir().join(format!("rustie-export-test-{}.wav",std::process::id()));
        let peak = render_mix_to_wav(graph, &events, 3000, &file, 48_000).unwrap();
        assert!(peak > 0.01);
        let mut wav = hound::WavReader::open(&file).unwrap();
        let samples = wav.samples::<f32>().collect::<Result<Vec<_>, _>>().unwrap();
        assert_eq!(samples.len(), 6000);
        assert!(samples[..960].iter().all(|v| *v == 0.0));
        assert!(samples[960..5000].iter().any(|v| v.abs() > 0.01));
        assert!(samples[5000..].iter().all(|v| *v == 0.0));
        let _ = fs::remove_file(file);
    }
}
