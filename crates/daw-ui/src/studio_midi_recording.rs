// Non-destructive realtime MIDI take capture. Arrival times are measured on
// the engine project clock when the UI forwards MIDI to the audio callback.
// This initial version does NOT promise sample-accurate hardware timestamps.
struct MidiTake {
    track_id: u64,
    start_frame: u64,
    sample_rate: u32,
    bpm: f64,
    held: HashMap<(u8, u8), (u64, u8)>,
    notes: Vec<MidiNote>,
}

impl MidiTake {
    fn new(track_id: u64, start_frame: u64, sample_rate: u32, bpm: f64) -> Self {
        Self {
            track_id, start_frame, sample_rate, bpm,
            held: HashMap::new(), notes: Vec::new(),
        }
    }

    fn ticks_at(&self, frame: u64) -> i64 {
        ((frame.saturating_sub(self.start_frame) as f64 / self.sample_rate as f64)
            * self.bpm * PPQ as f64 / 60.0)
            .round().max(0.0).min(i64::MAX as f64) as i64
    }

    fn note_on(&mut self, channel: u8, pitch: u8, velocity: u8, frame: u64) {
        self.note_off(channel, pitch, frame);
        self.held.insert((channel, pitch), (frame, velocity));
    }

    fn note_off(&mut self, channel: u8, pitch: u8, frame: u64) {
        let Some((start, velocity)) = self.held.remove(&(channel, pitch)) else {
            return;
        };
        let start_tick = self.ticks_at(start);
        let end_tick = self.ticks_at(frame);
        self.notes.push(MidiNote {
            pitch, velocity, muted: false,
            start: MusicalTime::from_ticks(start_tick),
            duration: MusicalTime::from_ticks(end_tick.saturating_sub(start_tick).max(1)),
        });
    }

    fn finish(mut self, frame: u64) -> (u64, Vec<MidiNote>, i64) {
        let keys = self.held.keys().copied().collect::<Vec<_>>();
        for (channel, note) in keys {
            self.note_off(channel, note, frame);
        }
        self.notes.sort_by_key(|note| note.start.ticks());
        let duration = self.ticks_at(frame).max(
            self.notes.iter().map(|note|
                note.start.ticks().saturating_add(note.duration.ticks()))
                .max().unwrap_or(0)
        ).max(1);
        (self.track_id, self.notes, duration)
    }
}

impl Studio {
    fn midi_transport_frame(&self) -> u64 {
        self.driver.as_ref().map_or(self.playhead_frame, |driver| {
            driver.telemetry().project_frame.load(Ordering::Relaxed)
        })
    }

    fn start_midi_take(&mut self) {
        if self.midi_take.is_some() || self.recorder.is_some() {
            return;
        }
        let Some(track_id) = self.selected_track.filter(|id| self.project.tracks.iter()
            .any(|track| track.id == *id && track.kind == TrackKind::Midi)) else {
            self.error = Some("Select a MIDI track before recording MIDI notes".into());
            return;
        };
        if self.midi_connection.is_none() {
            self.error = Some("Connect a hardware MIDI keyboard in Devices before recording MIDI".into());
            return;
        }
        if self.clip_count() >= MAX_CLIPS {
            self.error = Some("Maximum 48 clips; cannot record another MIDI clip".into());
            return;
        }
        if !self.playing {
            self.start_playback();
        }
        if !self.playing {
            self.error = Some("Audio transport failed to start for MIDI recording".into());
            return;
        }
        let bpm = self.project.tempo_map.tempo_at(MusicalTime::ZERO)
            .map_or(120.0, |tempo| tempo.bpm());
        self.midi_take = Some(MidiTake::new(
            track_id, self.playhead_frame, self.sample_rate, bpm));
        self.error = None;
        self.status = "Recording MIDI notes • REC again or Stop to finish".into();
    }

    fn finish_midi_take(&mut self) {
        let Some(take) = self.midi_take.take() else { return; };
        let end_frame = self.midi_transport_frame();
        let start_frame = take.start_frame;
        let (track_id, notes, duration) = take.finish(end_frame);
        if notes.is_empty() {
            self.status = "MIDI take finished without notes; project unchanged".into();
            return;
        }
        if self.clip_count() >= MAX_CLIPS {
            self.error = Some("Maximum 48 clips; recorded MIDI take was not added".into());
            return;
        }
        // The project currently exposes one constant tempo; start is stored
        // as musical time and all note starts are relative to the new clip.
        let bpm = self.project.tempo_map.tempo_at(MusicalTime::ZERO)
            .map_or(120.0, |tempo| tempo.bpm());
        let start_ticks = ((start_frame as f64 / self.sample_rate as f64)
            * bpm * PPQ as f64 / 60.0).round().max(0.0) as i64;
        let new_id = self.next_id;
        if let Some(track) = self.project.track_mut(track_id) {
            track.clips.push(Clip {
                id: new_id,
                start: MusicalTime::from_ticks(start_ticks),
                duration: MusicalTime::from_ticks(duration),
                kind: ClipKind::Midi(MidiClip { notes }),
                scale_override: None,
            });
            self.next_id = self.next_id.saturating_add(1);
            self.selected_clip = Some(new_id);
            self.selected_track = Some(track_id);
            self.detail = Detail::Clip;
            self.dirty = true;
            self.status = "MIDI performance captured in a new editable clip".into();
        }
    }
}

#[cfg(test)]
mod midi_take_tests {
    use super::*;

    #[test]
    fn midi_take_preserves_note_lengths_velocity_and_retrigger() {
        let mut take = MidiTake::new(9, 48_000, 48_000, 120.0);
        take.note_on(0, 60, 96, 48_000);
        take.note_off(0, 60, 72_000);
        take.note_on(0, 64, 80, 72_000);
        take.note_on(0, 64, 99, 84_000);
        let (track_id, notes, duration) = take.finish(96_000);
        assert_eq!(track_id, 9);
        assert_eq!(notes.len(), 3);
        assert_eq!(notes[0].pitch, 60);
        assert_eq!(notes[0].velocity, 96);
        assert_eq!(notes[0].start.ticks(), 0);
        assert_eq!(notes[0].duration.ticks(), 960);
        assert_eq!(notes[1].pitch, 64);
        assert_eq!(notes[1].duration.ticks(), 480);
        assert_eq!(notes[2].velocity, 99);
        assert_eq!(notes[2].start.ticks(), 1440);
        assert_eq!(duration, 1920);
    }

    #[test]
    fn zero_length_and_held_notes_never_create_invalid_notes() {
        let mut take = MidiTake::new(1, 0, 44_100, 120.0);
        take.note_on(3, 73, 127, 0);
        let (_, notes, duration) = take.finish(0);
        assert_eq!(duration, 1);
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].duration.ticks(), 1);
        let (_, empty, duration) = MidiTake::new(1, 0, 44_100, 120.0).finish(0);
        assert!(empty.is_empty());
        assert_eq!(duration, 1);
    }
}
