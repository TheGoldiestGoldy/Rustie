// Non-destructive clip editing operations. Audio split and left trim persist a
// source offset, so the resulting clips play the correct part of the original
// WAV. The source file itself is never rewritten.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ClipAction {
    Duplicate { track: u64, clip: u64 },
    SplitAtPlayhead { track: u64, clip: u64 },
    Delete { track: u64, clip: u64 },
    Copy { track: u64, clip: u64 },
}

/// Which edge of an arrangement clip a pointer drag captured.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ClipTrimEdge {
    Left,
    Right,
}

#[derive(Clone, Copy)]
struct ClipMenuTarget {
    track: u64,
    clip: u64,
    kind: TrackKind,
    parent: Option<u64>,
    can_split: bool,
}

fn clip_context_menu(ui: &mut egui::Ui, target: ClipMenuTarget,
    groups: &[(u64,String)], track_action: &mut Option<TrackAction>,
    clip_action: &mut Option<ClipAction>,
) {
    let ClipMenuTarget { track, clip, kind, parent, can_split } = target;
    track_context_menu(ui, track, kind, parent, groups, track_action);
    ui.separator();
    ui.label(egui::RichText::new("CLIP ACTIONS").size(10.0).color(STUDIO_DIM));
    if ui.button("Copy clip").on_hover_text(
        "Copy this clip to the clipboard. Paste at the playhead with Ctrl+V").clicked() {
        *clip_action = Some(ClipAction::Copy { track, clip });
        ui.close();
    }
    if ui.button("Duplicate clip").on_hover_text(
        "Add a copy after the original clip; source media stays untouched").clicked() {
        *clip_action = Some(ClipAction::Duplicate { track, clip });
        ui.close();
    }
    if ui.add_enabled(can_split, egui::Button::new("Split clip at playhead"))
        .on_hover_text("Cut into two clips here without modifying the audio source")
        .clicked() {
        *clip_action = Some(ClipAction::SplitAtPlayhead { track, clip });
        ui.close();
    }
    if ui.button("Delete clip…").on_hover_text(
        "Remove this clip after confirmation. Its original audio file is not deleted")
        .clicked() {
        *clip_action = Some(ClipAction::Delete { track, clip });
        ui.close();
    }
}

/// Convert a pointer/tick delta into a snapped tick delta. Snap stays at the
/// arrangement's 1/16 grid; disabling snap keeps the sub-tick drag resolution.
fn snapped_tick_delta(raw_ticks: i64, snap: bool, quantum: i64) -> i64 {
    if !snap { return raw_ticks; }
    let q = quantum.max(1);
    let half = q / 2;
    if raw_ticks >= 0 {
        raw_ticks.saturating_add(half) / q * q
    } else {
        raw_ticks.saturating_sub(half) / q * q
    }
}

/// Quantize an absolute musical position (paste, loop points) to the grid.
fn snapped_absolute_tick(tick: i64, snap: bool, quantum: i64) -> i64 {
    snapped_tick_delta(tick.max(0), snap, quantum)
}

#[cfg(test)]
fn frames_for_ticks(ticks: i64, bpm: f64, sample_rate: u32) -> u64 {
    if ticks <= 0 || sample_rate == 0 || !bpm.is_finite() || bpm <= 0.0 {
        return 0;
    }
    (ticks as f64 / PPQ as f64 * 60.0 / bpm * sample_rate as f64)
        .round()
        .max(0.0) as u64
}

#[cfg(test)]
fn ticks_for_frames(frames: u64, bpm: f64, sample_rate: u32) -> i64 {
    if frames == 0 || sample_rate == 0 || !bpm.is_finite() || bpm <= 0.0 {
        return 0;
    }
    (frames as f64 / sample_rate as f64 * bpm / 60.0 * PPQ as f64)
        .round()
        .max(0.0) as i64
}

/// Clamp a left-edge drag so the clip start stays on/after zero and at least
/// one tick of duration remains. Returns the actually applied tick delta.
fn clamped_left_trim(requested: i64, start_ticks: i64, duration_ticks: i64) -> i64 {
    let end = start_ticks.saturating_add(duration_ticks.max(1));
    let new_start = start_ticks
        .saturating_add(requested)
        .clamp(0, end.saturating_sub(1));
    new_start - start_ticks
}

/// Clamp a right-edge drag to one tick and to the available source length.
fn clamped_right_duration(
    requested: i64,
    duration_ticks: i64,
    max_ticks: i64,
) -> i64 {
    duration_ticks
        .saturating_add(requested)
        .clamp(1, max_ticks.max(1))
}

/// The audio source advance follows the exact project frame of the split tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SplitPlan {
    left_ticks: i64,
    source_advance_frames: u64,
}

fn plan_split(
    playhead_frame: u64,
    playhead_tick: i64,
    begin: i64,
    end: i64,
    frame_at_tick: impl Fn(i64) -> u64,
) -> Option<SplitPlan> {
    let start_frame = frame_at_tick(begin);
    if end.saturating_sub(begin) <= 1
        || playhead_frame <= start_frame
        || playhead_frame >= frame_at_tick(end)
    {
        return None;
    }
    let split_tick = playhead_tick.clamp(begin + 1, end - 1);
    Some(SplitPlan {
        left_ticks: split_tick - begin,
        source_advance_frames: frame_at_tick(split_tick).saturating_sub(start_frame),
    })
}

/// Rebases MIDI notes after the left edge moves right by `delta_ticks` (trim
/// in) or left by a negative delta (extend). Notes fully outside the retained
/// region are dropped; crossing notes are shortened, never pushed outside.
fn trim_midi_left(notes: &[MidiNote], delta_ticks: i64) -> Vec<MidiNote> {
    let mut kept = Vec::with_capacity(notes.len());
    for note in notes {
        let start = note.start.ticks();
        let end = start.saturating_add(note.duration.ticks());
        let relative_start = start.saturating_sub(delta_ticks);
        let relative_end = end.saturating_sub(delta_ticks);
        if relative_end <= 0 { continue; }
        let mut trimmed = *note;
        trimmed.start = MusicalTime::from_ticks(relative_start.max(0));
        trimmed.duration = MusicalTime::from_ticks(
            relative_end.saturating_sub(relative_start.max(0)).max(1),
        );
        kept.push(trimmed);
    }
    kept
}

/// Split note data at a relative clip tick. Crossing notes are divided; no
/// note extends outside its resulting clip's timeline.
fn split_midi_notes(notes: &[MidiNote], at_tick: i64) -> (Vec<MidiNote>, Vec<MidiNote>) {
    let mut left = Vec::new();
    let mut right = Vec::new();
    for note in notes {
        let start = note.start.ticks();
        let end = start.saturating_add(note.duration.ticks());
        if start < at_tick {
            let mut first = *note;
            first.duration = MusicalTime::from_ticks(end.min(at_tick).saturating_sub(start).max(1));
            left.push(first);
        }
        if end > at_tick {
            let mut later = *note;
            later.start = MusicalTime::from_ticks(start.max(at_tick).saturating_sub(at_tick));
            later.duration = MusicalTime::from_ticks(end.saturating_sub(start.max(at_tick)));
            right.push(later);
        }
    }
    (left,right)
}

/// Arrangement preview rectangle for a clip. Left trims move the left edge
/// while keeping the right edge fixed; right trims move only the right edge.
fn clip_preview_bounds(
    start_x: f32,
    full_width: f32,
    move_delta: f32,
    trim: Option<(ClipTrimEdge, f32)>,
    min_width: f32,
) -> (f32, f32) {
    let min_width = min_width.max(1.0);
    match trim {
        Some((ClipTrimEdge::Left, delta)) => {
            let delta = delta.clamp(-(full_width - min_width), full_width - min_width);
            (start_x + delta, (full_width - delta).max(min_width))
        }
        Some((ClipTrimEdge::Right, delta)) => {
            (start_x, (full_width + delta).max(min_width))
        }
        None => (start_x + move_delta, full_width.max(min_width)),
    }
}

fn track_kind_accepts_clip(kind: TrackKind, clip: &ClipKind) -> bool {
    matches!(
        (kind, clip),
        (TrackKind::Audio, ClipKind::Audio(_)) | (TrackKind::Midi, ClipKind::Midi(_))
    )
}

/// Track-compatible paste destination: the selected track first, then the
/// selected clip's track, then the first matching track. Groups and the wrong
/// media kind are never silently chosen.
fn choose_paste_track(
    project: &Project,
    selected_track: Option<u64>,
    selected_clip: Option<u64>,
    kind: &ClipKind,
) -> Option<u64> {
    let compatible = |track: &Track| track_kind_accepts_clip(track.kind, kind);
    let selected = selected_track
        .and_then(|id| project.tracks.iter().find(|t| t.id == id))
        .filter(|t| compatible(t))
        .map(|t| t.id);
    let clip_track = selected_clip
        .and_then(|id| project.tracks.iter()
            .find(|t| t.clips.iter().any(|c| c.id == id)))
        .filter(|t| compatible(t))
        .map(|t| t.id);
    let first = project.tracks.iter().find(|t| compatible(t)).map(|t| t.id);
    selected.or(clip_track).or(first)
}

struct ArrangementSplit {
    track: u64,
    clip: u64,
    begin: i64,
    end: i64,
    plan: SplitPlan,
}

/// Apply an already-planned split. Kept as a pure project transform so tests
/// can assert exact tick/frame boundaries without an audio device.
fn split_clip_in_project(
    project: &mut Project,
    track_id: u64,
    clip_id: u64,
    new_clip_id: u64,
    begin: i64,
    end: i64,
    plan: SplitPlan,
) -> bool {
    let Some(track) = project.track_mut(track_id) else { return false };
    let Some(index) = track.clips.iter().position(|c| c.id == clip_id) else {
        return false;
    };
    let original = &track.clips[index];
    let mut right = original.clone();
    right.id = new_clip_id;
    right.start = MusicalTime::from_ticks(begin + plan.left_ticks);
    right.duration = MusicalTime::from_ticks(end - (begin + plan.left_ticks));
    match (&mut track.clips[index].kind, &mut right.kind) {
        (ClipKind::Audio(_), ClipKind::Audio(audio)) => {
            audio.source_offset_frames = audio.source_offset_frames
                .saturating_add(plan.source_advance_frames);
        }
        (ClipKind::Midi(left), ClipKind::Midi(second)) => {
            let (a, b) = split_midi_notes(&left.notes, plan.left_ticks);
            left.notes = a;
            second.notes = b;
        }
        _ => return false,
    }
    track.clips[index].duration = MusicalTime::from_ticks(plan.left_ticks);
    track.clips.insert(index + 1, right);
    true
}

impl Studio {
    fn tick_at_frame(&self, frame: u64) -> i64 {
        let frame_at = |tick| {
            self.project.tempo_map.sample_frame_at(MusicalTime::from_ticks(tick), self.sample_rate)
                .unwrap_or(u64::MAX)
        };
        // Find the first tick at or after the requested frame, then compare
        // its adjacent tick. Use the full tempo map for a shared split point.
        let mut low = 0;
        let mut high = i64::MAX;
        while low < high {
            let mid = low + (high - low) / 2;
            if frame_at(mid) < frame {
                low = mid + 1;
            } else {
                high = mid;
            }
        }
        let previous = low.saturating_sub(1).max(0);
        if frame.abs_diff(frame_at(previous)) < frame.abs_diff(frame_at(low)) {
            previous
        } else {
            low
        }
    }

    /// Apply a right-edge trim or left-edge trim to one clip. The caller has
    /// already converted pixels into a tick delta and applied snap.
    fn apply_clip_trim(&mut self, clip_id: u64, edge: ClipTrimEdge, tick_delta: i64) {
        if tick_delta == 0 { return; }
        let sample_rate = self.sample_rate;
        let tempo_map = &self.project.tempo_map;
        // All audio offsets are in decoded PCM frames; use the real tempo map
        // at the clip's absolute position, not relative-beat approximations.
        let frame_at_tick = |tick: i64| tempo_map.sample_frame_at(
            MusicalTime::from_ticks(tick.max(0)), sample_rate,
        ).unwrap_or(0);
        for track in &mut self.project.tracks {
            let Some(clip) = track.all_clips_mut().find(|c| c.id == clip_id) else { continue };
            let start = clip.start.ticks();
            let duration = clip.duration.ticks().max(1);
            match edge {
                ClipTrimEdge::Right => {
                    let max_ticks = match &clip.kind {
                        ClipKind::Audio(a) => self.media.get(&a.source).map_or(i64::MAX, |cache| {
                            let available = (cache.samples.len() as u64)
                                .saturating_sub(a.source_offset_frames);
                            let old_frame = frame_at_tick(start);
                            let desired = duration.saturating_add(tick_delta).max(1);
                            // Find the last musical tick which fits in the
                            // decoded file, including tempo changes.
                            let (mut low, mut high) = (0_i64, desired);
                            while low < high {
                                let mid = low + (high - low) / 2 + (high - low) % 2;
                                if frame_at_tick(start.saturating_add(mid))
                                    .saturating_sub(old_frame) <= available
                                {
                                    low = mid;
                                } else {
                                    high = mid - 1;
                                }
                            }
                            low.max(1)
                        }),
                        ClipKind::Midi(_) => i64::MAX,
                    };
                    clip.duration = MusicalTime::from_ticks(clamped_right_duration(
                        tick_delta, duration, max_ticks,
                    ));
                }
                ClipTrimEdge::Left => {
                    let mut applied = clamped_left_trim(tick_delta, start, duration);
                    if applied == 0 { return; }
                    match &mut clip.kind {
                        ClipKind::Audio(audio) => {
                            let old_frame = frame_at_tick(start);
                            if applied > 0 {
                                let available = self.media.get(&audio.source).map_or(
                                    u64::MAX, |cache| (cache.samples.len() as u64)
                                        .saturating_sub(audio.source_offset_frames)
                                        .saturating_sub(1),
                                );
                                let (mut low, mut high) = (0_i64, applied);
                                while low < high {
                                    let mid = low + (high - low) / 2 + (high - low) % 2;
                                    let advance = frame_at_tick(start.saturating_add(mid))
                                        .saturating_sub(old_frame);
                                    if advance <= available {
                                        low = mid;
                                    } else {
                                        high = mid - 1;
                                    }
                                }
                                applied = low;
                                if applied == 0 { return; }
                                let advance = frame_at_tick(start.saturating_add(applied))
                                    .saturating_sub(old_frame);
                                audio.source_offset_frames = audio.source_offset_frames
                                    .saturating_add(advance);
                            } else {
                                // Revealing earlier material must not underflow
                                // the source, even at fractional frames per tick.
                                let (mut low, mut high) = (applied, 0_i64);
                                while low < high {
                                    let mid = low + (high - low) / 2;
                                    let reveal = old_frame.saturating_sub(
                                        frame_at_tick(start.saturating_add(mid)),
                                    );
                                    if reveal <= audio.source_offset_frames {
                                        high = mid;
                                    } else {
                                        low = mid + 1;
                                    }
                                }
                                applied = low;
                                if applied == 0 { return; }
                                let reveal = old_frame.saturating_sub(
                                    frame_at_tick(start.saturating_add(applied)),
                                );
                                audio.source_offset_frames = audio.source_offset_frames
                                    .saturating_sub(reveal);
                            }
                        }
                        ClipKind::Midi(midi) => {
                            if applied < 0 {
                                let max_back = midi
                                    .notes
                                    .iter()
                                    .map(|note| note.start.ticks())
                                    .min()
                                    .unwrap_or(0);
                                applied = applied.max(-max_back);
                                if applied == 0 { return; }
                            }
                            midi.notes = trim_midi_left(&midi.notes, applied);
                        }
                    }
                    clip.start = MusicalTime::from_ticks(start + applied);
                    clip.duration = MusicalTime::from_ticks(duration - applied);
                }
            }
            self.selected_clip = Some(clip_id);
            self.dirty = true;
            self.status = match edge {
                ClipTrimEdge::Left => "Trimmed clip start without rewriting source audio".into(),
                ClipTrimEdge::Right => "Trimmed clip end without rewriting source audio".into(),
            };
            self.restart_for_edit();
            return;
        }
    }

    fn arrangement_clip_selected(&self, id: u64) -> bool {
        self.selected_clip == Some(id)
            || (self.view == View::Arrangement
                && self.arrangement_selection.last().copied() == self.selected_clip
                && self.arrangement_selection.contains(&id))
    }

    fn select_arrangement_clip(&mut self, track: u64, clip: u64, toggle: bool, keep_group: bool) {
        if self.arrangement_selection.last().copied() != self.selected_clip {
            self.arrangement_selection = self.selected_clip.into_iter().collect();
        }
        if toggle && let Some(index) = self.arrangement_selection.iter().position(|id| *id == clip) {
            self.arrangement_selection.remove(index);
            self.selected_clip = self.arrangement_selection.last().copied();
            if let Some((owner, _)) = self.selected_clip_pair() {
                self.selected_track = Some(owner.id);
            }
        } else {
            if !toggle && !keep_group {
                self.arrangement_selection.clear();
            }
            self.arrangement_selection.retain(|id| *id != clip);
            self.arrangement_selection.push(clip);
            self.selected_clip = Some(clip);
            self.selected_track = Some(track);
        }
        self.piano_selected = None;
    }

    fn selected_arrangement_clips(&self) -> impl Iterator<Item = (&Track, &Clip)> {
        self.project.tracks.iter().flat_map(move |track| {
            track.clips.iter().filter_map(move |clip| {
                self.arrangement_clip_selected(clip.id).then_some((track, clip))
            })
        })
    }

    fn clear_arrangement_selection(&mut self) {
        self.selected_clip = None;
        self.arrangement_selection.clear();
        self.piano_selected = None;
    }

    fn copy_selected_clip(&mut self) -> bool {
        if self.view == View::Session {
            let Some((track, clip)) = self.selected_clip_pair() else { return false };
            self.clip_clipboard = vec![(track.id, clip.clone())];
            self.status = "Session clip copied".into();
            return true;
        }
        let clips: Vec<_> = self.selected_arrangement_clips()
            .map(|(track, clip)| (track.id, clip.clone()))
            .collect();
        if clips.is_empty() {
            self.error = Some("Select clips to copy".into());
            return false;
        }
        self.clip_clipboard = clips;
        self.status = "Copied selection. Ctrl+V pastes at the playhead.".into();
        true
    }

    fn paste_clip(&mut self) {
        let mut clips = self.clip_clipboard.clone();
        if clips.is_empty() {
            self.error = Some("Clipboard is empty. Copy a clip first.".into());
            return;
        }
        if let [(track_id, clip)] = clips.as_mut_slice() {
            let Some(target) = choose_paste_track(
                &self.project, self.selected_track, self.selected_clip, &clip.kind,
            ) else {
                self.error = Some("Add a compatible audio or MIDI track before pasting".into());
                return;
            };
            *track_id = target;
        }
        if self.view == View::Session {
            if let [(track_id, clip)] = clips.as_slice() {
                self.copy_into_session(*track_id, clip.clone());
            } else {
                self.error = Some("Paste multi-clip selections in Arrangement.".into());
            }
            return;
        }
        let raw_tick = self.tick_at_frame(self.playhead_frame);
        let paste_tick = snapped_absolute_tick(raw_tick, self.snap, PPQ / 4).max(0);
        let first_tick = clips.iter().map(|(_, clip)| clip.start.ticks()).min().unwrap_or(0);
        self.insert_clip_copies(clips, paste_tick.saturating_sub(first_tick), false);
    }

    fn can_add_arrangement_clips(&mut self, count: usize) -> bool {
        if self.clip_count().saturating_add(count) > MAX_CLIPS {
            self.error = Some(format!("Maximum {MAX_CLIPS} clips"));
            return false;
        }
        if self.next_id.checked_add(count as u64).is_none() {
            self.error = Some("No clip IDs available".into());
            return false;
        }
        true
    }

    fn insert_clip_copies(&mut self, mut clips: Vec<(u64, Clip)>, delta: i64, duplicate: bool) {
        if !self.can_add_arrangement_clips(clips.len()) {
            return;
        }
        // Check every destination before touching project data.
        for (track_id, clip) in &mut clips {
            if !self.project.tracks.iter().any(|track| {
                track.id == *track_id && track_kind_accepts_clip(track.kind, &clip.kind)
            }) {
                self.error = Some("The selection needs all of its original tracks to paste".into());
                return;
            }
            let Some(start) = clip.start.ticks().checked_add(delta).filter(|start| {
                *start >= 0 && start.checked_add(clip.duration.ticks()).is_some()
            }) else {
                self.error = Some("The selection extends beyond the timeline".into());
                return;
            };
            clip.start = MusicalTime::from_ticks(start);
        }
        self.clear_arrangement_selection();
        let count = clips.len();
        for (track_id, mut clip) in clips {
            let original_id = clip.id;
            clip.id = self.next_id;
            self.next_id += 1;
            self.arrangement_selection.push(clip.id);
            self.selected_track = Some(track_id);
            self.selected_clip = Some(clip.id);
            if let Some(track) = self.project.track_mut(track_id) {
                let index = if duplicate {
                    track.clips.iter().position(|item| item.id == original_id)
                        .map_or(track.clips.len(), |index| index + 1)
                } else {
                    track.clips.len()
                };
                track.clips.insert(index, clip);
            }
        }
        self.dirty = true;
        let action = if duplicate { "Duplicated" } else { "Pasted" };
        self.status = format!("{action} {count} clip(s)");
        self.restart_for_edit();
    }

    fn delete_selected_clip(&mut self) {
        if self.view == View::Session {
            if let Some((track, clip)) = self.selected_clip_pair() {
                self.delete_session_clip(track.id, clip.id);
            }
            return;
        }
        let targets: Vec<_> = self.selected_arrangement_clips()
            .map(|(track, clip)| (track.id, clip.id))
            .collect();
        if targets.is_empty() {
            self.error = Some("Select clips to delete".into());
            return;
        }
        self.delete_arrangement_clips(&targets);
    }

    fn delete_arrangement_clips(&mut self, targets: &[(u64, u64)]) {
        if self.arrangement_selection.last().copied() != self.selected_clip {
            self.arrangement_selection.clear();
        }
        for &(track_id, clip_id) in targets {
            if let Some(track) = self.project.track_mut(track_id) {
                track.clips.retain(|clip| clip.id != clip_id);
            }
            self.arrangement_selection.retain(|id| *id != clip_id);
            if self.selected_clip == Some(clip_id) {
                self.selected_clip = self.arrangement_selection.last().copied();
                self.piano_selected = None;
            }
        }
        if let Some((track, _)) = self.selected_clip_pair() {
            self.selected_track = Some(track.id);
        }
        self.dirty = true;
        self.status = format!("Clips deleted: {}. Ctrl+Z to undo.", targets.len());
        self.restart_for_edit();
    }

    fn duplicate_selected_clip(&mut self) {
        if self.view == View::Session {
            if let Some((track, clip)) = self.selected_clip_pair() {
                self.copy_into_session(track.id, clip.clone());
            }
            return;
        }
        let clips = self.selected_arrangement_clips()
            .map(|(track, clip)| (track.id, clip.clone()))
            .collect();
        self.duplicate_arrangement_clips(clips);
    }

    fn duplicate_arrangement_clips(&mut self, clips: Vec<(u64, Clip)>) {
        let Some(first) = clips.iter().map(|(_, clip)| clip.start.ticks()).min() else {
            self.error = Some("Select clips to duplicate".into());
            return;
        };
        let end = clips.iter().map(|(_, clip)| {
            clip.start.ticks().saturating_add(clip.duration.ticks())
        }).max().unwrap_or(first);
        self.insert_clip_copies(clips, end.saturating_sub(first), true);
    }

    fn split_selected_clip(&mut self) {
        let tick = self.tick_at_frame(self.playhead_frame);
        let splits = self.selected_arrangement_clips()
            .filter_map(|(track, clip)| self.plan_clip_split(track.id, clip, tick))
            .collect();
        self.apply_arrangement_splits(splits);
    }

    fn plan_clip_split(&self, track: u64, clip: &Clip, tick: i64) -> Option<ArrangementSplit> {
        let begin = clip.start.ticks();
        let end = begin.saturating_add(clip.duration.ticks());
        let plan = plan_split(self.playhead_frame, tick, begin, end, |tick| {
            self.tick_to_frame(tick)
        })?;
        Some(ArrangementSplit { track, clip: clip.id, begin, end, plan })
    }

    fn apply_arrangement_splits(&mut self, splits: Vec<ArrangementSplit>) {
        if splits.is_empty() {
            self.error = Some("Place the playhead inside a selected clip to split it".into());
            return;
        }
        if !self.can_add_arrangement_clips(splits.len()) {
            return;
        }
        self.clear_arrangement_selection();
        for ArrangementSplit { track, clip, begin, end, plan } in splits {
            let new_id = self.next_id;
            if split_clip_in_project(&mut self.project, track, clip, new_id, begin, end, plan) {
                self.next_id += 1;
                self.arrangement_selection.push(new_id);
                self.selected_track = Some(track);
                self.selected_clip = Some(new_id);
            }
        }
        self.dirty = true;
        self.status = "Split clips at the playhead".into();
        self.restart_for_edit();
    }

    fn move_arrangement_selection(&mut self, delta: i64) {
        if delta == 0 {
            return;
        }
        let ids: Vec<_> = self.selected_arrangement_clips().map(|(_, clip)| clip.id).collect();
        for clip in self.project.tracks.iter_mut().flat_map(|track| &mut track.clips) {
            if ids.contains(&clip.id) {
                clip.start = MusicalTime::from_ticks(clip.start.ticks().saturating_add(delta));
            }
        }
        self.dirty = true;
        self.restart_for_edit();
    }

    fn arrangement_move_delta(&self, pixels: f32) -> i64 {
        let raw_ticks = (pixels / self.zoom * PPQ as f32).round() as i64;
        let ticks = snapped_tick_delta(raw_ticks, self.snap, PPQ / 4);
        let mut earliest = i64::MAX;
        let mut latest = 0;
        for (_, clip) in self.selected_arrangement_clips() {
            earliest = earliest.min(clip.start.ticks());
            latest = latest.max(clip.start.ticks().saturating_add(clip.duration.ticks()));
        }
        if earliest == i64::MAX { 0 } else { ticks.clamp(-earliest, i64::MAX - latest) }
    }

    fn copy_into_session(&mut self, track: u64, clip: Clip) {
        if let Some(scene) = self.first_empty_scene(track) {
            self.insert_session_clip(track, scene, clip);
        } else {
            self.error = Some("Add an empty scene for the copied clip.".into());
        }
    }

    fn delete_session_clip(&mut self, track: u64, clip: u64) {
        if let Some(track) = self.project.track_mut(track) {
            track.session_slots.retain(|_, item| item.id != clip);
        }
        self.session_active.retain(|_, id| *id != clip);
        if self.selected_clip == Some(clip) {
            self.selected_clip = None;
            self.piano_selected = None;
        }
        self.dirty = true;
        self.status = "Session clip deleted. Ctrl+Z to undo.".into();
        self.restart_for_edit();
    }

    fn apply_clip_action(&mut self, action: ClipAction) {
        match action {
            ClipAction::Copy { track, clip } => {
                if let Some(source) = self.project.track_mut(track)
                    .and_then(|track| track.all_clips().find(|item| item.id == clip))
                {
                    self.clip_clipboard = vec![(track, source.clone())];
                    self.status = "Clip copied. Ctrl+V pastes at the playhead.".into();
                }
            }
            ClipAction::Delete { track, clip } => self.clip_delete = Some((track, clip)),
            ClipAction::Duplicate { track, clip } => {
                if let Some(source) = self.project.track_mut(track)
                    .and_then(|item| item.playback_clips(self.view == View::Session)
                        .find(|item| item.id == clip))
                    .cloned()
                {
                    if self.view == View::Session {
                        self.copy_into_session(track, source);
                    } else {
                        self.duplicate_arrangement_clips(vec![(track, source)]);
                    }
                }
            }
            ClipAction::SplitAtPlayhead { track, clip } => {
                if self.view == View::Session { return; }
                let tick = self.tick_at_frame(self.playhead_frame);
                let splits = self.project.tracks.iter()
                    .find(|item| item.id == track)
                    .and_then(|track| track.clips.iter().find(|item| item.id == clip))
                    .and_then(|source| self.plan_clip_split(track, source, tick))
                    .into_iter().collect();
                self.apply_arrangement_splits(splits);
            }
        }
    }

    fn draw_clip_delete_window(&mut self, ctx: &egui::Context) {
        let Some((track,clip))=self.clip_delete else {return};
        let mut confirm=false;
        let mut cancel=false;
        egui::Window::new("Delete clip?")
            .id(egui::Id::new("delete_clip_dialog"))
            .collapsible(false).resizable(false)
            .show(ctx,|ui| {
                ui.label("Remove this clip from the project?");
                ui.weak("The original WAV is kept. Delete is reversible with Ctrl+Z.");
                ui.horizontal(|ui| {
                    if ui.button("Delete clip").clicked(){confirm=true;}
                    if ui.button("Cancel").clicked(){cancel=true;}
                });
                if ui.input(|i| i.key_pressed(egui::Key::Escape)){cancel=true;}
            });
        if cancel || confirm {self.clip_delete=None;}
        if confirm {
            if self.project.tracks.iter().any(|owner| owner.id == track
                && owner.session_slots.values().any(|item| item.id == clip))
            {
                self.delete_session_clip(track, clip);
            } else {
                self.delete_arrangement_clips(&[(track, clip)]);
            }
        }
    }
}

#[cfg(test)]
mod clip_edit_tests {
    use super::*;

    fn note(start: i64, duration: i64) -> MidiNote {
        MidiNote {
            pitch: 60, velocity: 100, muted: false,
            start: MusicalTime::from_ticks(start),
            duration: MusicalTime::from_ticks(duration),
        }
    }

    #[test]
    fn batch_edits_preserve_mixed_tracks_offsets_and_survive_save_reopen() {
        let temp = std::env::temp_dir().join(format!(
            "rustie-batch-{}-{}", std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos(),
        ));
        fs::create_dir(&temp).unwrap();
        let wav = temp.join("source.wav");
        let mut writer = hound::WavWriter::create(&wav, hound::WavSpec {
            channels: 1,
            sample_rate: 48_000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        }).unwrap();
        for sample in (0..48_000 * 6).map(|i| (i % 4096) as i16) {
            writer.write_sample(sample).unwrap();
        }
        writer.finalize().unwrap();
        let source_bytes = fs::read(&wav).unwrap();
        let path = temp.join("editing.slate.json");
        let mut studio = Studio::with_sample_rate(48_000);
        assert_eq!(studio.bar_ticks(), 4 * PPQ);
        studio.project.time_signature = TimeSignature::new(6, 8).unwrap();
        assert_eq!(studio.bar_ticks(), 3 * PPQ);
        let group_id = studio.next_id;
        let audio_id = group_id + 1;
        let audio_clip_id = audio_id + 1;
        studio.project.tracks[0].group_id = Some(group_id);
        studio.project.tracks.push(Track::new(group_id, "Group", TrackKind::Group));
        let mut audio = Track::new(audio_id, "Audio", TrackKind::Audio);
        audio.group_id = Some(group_id);
        audio.clips.push(Clip {
            id: audio_clip_id,
            start: MusicalTime::from_ticks(PPQ),
            duration: MusicalTime::from_ticks(8 * PPQ),
            kind: ClipKind::Audio(AudioClip {
                source: wav.clone(),
                source_offset_frames: 32,
                gain: 1.0,
                fade_in_ms: 0,
                fade_out_ms: 0,
                warped: false,
            }),
            scale_override: None,
        });
        studio.project.tracks.push(audio);
        studio.next_id = studio.project.next_object_id().unwrap();
        studio.history = ProjectHistory::new(&studio.project);

        studio.select_arrangement_clip(1, 2, false, false);
        studio.select_arrangement_clip(audio_id, audio_clip_id, true, false);
        let selection = studio.editor_selection();
        studio.history.observe(&studio.project, selection.clone(), false);
        assert_eq!(studio.arrangement_move_delta(-studio.zoom), 0);
        studio.duplicate_selected_clip();
        let duplicated = studio.editor_selection();
        studio.history.observe(&studio.project, duplicated, false);
        let duplicate_starts = studio.selected_arrangement_clips()
            .map(|(_, clip)| clip.start.ticks()).collect::<Vec<_>>();
        assert_eq!(duplicate_starts, vec![9 * PPQ, 10 * PPQ]);
        studio.history_step(false);
        assert_eq!(studio.editor_selection(), selection);
        assert!(studio.copy_selected_clip());
        studio.playhead_frame = studio.tick_to_frame(12 * PPQ + 7);
        let pasted_id = studio.next_id;
        studio.paste_clip();
        let pasted = studio.editor_selection();
        studio.history.observe(&studio.project, pasted.clone(), false);
        assert_eq!(pasted.clips, vec![pasted_id, pasted_id + 1]);
        let starts = studio.selected_arrangement_clips().map(|(track, clip)| {
            (track.id, clip.start.ticks())
        }).collect::<Vec<_>>();
        assert_eq!(starts, vec![(1, 12 * PPQ), (audio_id, 13 * PPQ)]);
        studio.history_step(false);
        assert_eq!(studio.editor_selection(), selection);
        studio.history_step(true);
        assert_eq!(studio.editor_selection(), pasted);

        studio.project.tempo_map.insert(
            MusicalTime::from_ticks(13 * PPQ), Tempo::from_bpm(96.0).unwrap(),
        ).unwrap();
        let after_tempo = studio.editor_selection();
        studio.history.observe(&studio.project, after_tempo, false);
        studio.playhead_frame = studio.tick_to_frame(14 * PPQ);
        studio.split_selected_clip();
        let right = studio.selected_arrangement_clips().map(|(_, clip)| clip).collect::<Vec<_>>();
        assert_eq!(right.len(), 2);
        assert!(right.iter().all(|clip| clip.start.ticks() == 14 * PPQ));
        let ClipKind::Audio(region) = &right[1].kind else { panic!("audio region") };
        assert_eq!(region.source_offset_frames, 30_032);
        let after_split = studio.editor_selection();
        studio.history.observe(&studio.project, after_split.clone(), false);
        studio.delete_selected_clip();
        let after_delete = studio.editor_selection();
        studio.history.observe(&studio.project, after_delete, false);
        studio.history_step(false);
        assert_eq!(studio.editor_selection(), after_split);

        studio.project_path = Some(path.clone());
        studio.save_project(false);
        assert!(studio.error.is_none(), "{:?}", studio.error);
        let saved = studio.project.clone();
        let mut reopened = Studio::with_sample_rate(48_000);
        reopened.load_project_path(path);
        assert!(reopened.error.is_none(), "{:?}", reopened.error);
        assert_eq!(reopened.project, saved);
        assert_eq!(reopened.media.len(), 1);
        assert_eq!(fs::read(wav).unwrap(), source_bytes);
        fs::remove_dir_all(temp).unwrap();
    }

    #[test]
    fn session_copy_duplicate_and_delete_leave_arrangement_untouched() {
        let mut studio = Studio::with_sample_rate(48_000);
        studio.change_view(View::Session);
        let arrangement = studio.project.tracks[0].clips.clone();
        let original = studio.selected_clip.unwrap();
        let scene = studio.first_empty_scene(1).unwrap();
        assert!(studio.copy_selected_clip());
        assert_eq!(studio.clip_clipboard[0].1.id, original);
        studio.duplicate_selected_clip();
        let duplicated = studio.project.tracks[0].session_slots[&scene].id;
        assert_ne!(duplicated, original);
        studio.delete_selected_clip();
        assert!(!studio.project.tracks[0].session_slots.contains_key(&scene));
        studio.paste_clip();
        assert!(studio.project.tracks[0].session_slots.contains_key(&scene));
        assert_eq!(studio.project.tracks[0].clips, arrangement);
        assert_eq!(studio.project.next_object_id(), Some(studio.next_id));
        assert!(studio.project.prepare_session().is_ok());
    }

    #[test]
    fn batch_capacity_failure_preserves_project_selection_and_next_id() {
        let mut studio = Studio::with_sample_rate(44_100);
        studio.select_arrangement_clip(1, 2, false, false);
        studio.select_arrangement_clip(3, 4, true, false);
        let template = studio.project.tracks[0].clips[0].clone();
        while studio.clip_count() < MAX_CLIPS - 1 {
            let mut clip = template.clone();
            clip.id = studio.next_id;
            studio.next_id += 1;
            studio.project.tracks[0].clips.push(clip);
        }
        let project = studio.project.clone();
        let selection = studio.editor_selection();
        let next_id = studio.next_id;
        studio.duplicate_selected_clip();
        assert!(studio.error.is_some());
        assert_eq!(studio.project, project);
        assert_eq!(studio.editor_selection(), selection);
        assert_eq!(studio.next_id, next_id);
    }

    #[test]
    fn split_midi_preserves_notes_across_split_and_rebases_right() {
        let (left,right)=split_midi_notes(&[note(0,480),note(720,240),note(1200,480)],960);
        assert_eq!(left.len(),2);
        assert_eq!(right.len(),1);
        assert_eq!(right[0].start.ticks(),240);
        let (left,right)=split_midi_notes(&[note(800,500)],960);
        assert_eq!(left[0].start.ticks(),800);
        assert_eq!(left[0].duration.ticks(),160);
        assert_eq!(right[0].start.ticks(),0);
        assert_eq!(right[0].duration.ticks(),340);
    }

    #[test]
    fn split_plan_uses_exact_source_frames_at_44k_and_48k() {
        // A 4-second clip starting at bar 2 (8 s). Split at 9.5 s.
        for rate in [44_100_u32, 48_000] {
            let begin = 16 * PPQ;
            let end = begin + 8 * PPQ;
            let tick = begin + 3 * PPQ;
            let frame_at = |tick| frames_for_ticks(tick, 120.0, rate);
            let plan = plan_split(frame_at(tick), tick, begin, end, frame_at).unwrap();
            assert_eq!(plan.source_advance_frames, rate as u64 * 3 / 2);
            assert_eq!(plan.left_ticks, 3 * PPQ);
        }
    }

    #[test]
    fn audio_split_preserves_sample_continuity_and_survives_project_roundtrip() {
        let bpm = 120.0;
        for rate in [44_100_u32, 48_000] {
            let mut project = Project::new("Audio seam", Tempo::from_bpm(bpm).unwrap());
            let mut track = Track::new(1, "Voice", TrackKind::Audio);
            let clip_start = 7 * PPQ + 13; // intentionally not at a bar boundary
            let duration = 8 * PPQ;
            let original_offset = 17_u64;
            track.clips.push(Clip {
                id: 2,
                start: MusicalTime::from_ticks(clip_start),
                duration: MusicalTime::from_ticks(duration),
                kind: ClipKind::Audio(AudioClip {
                    source: "untouched.wav".into(), warped: false,
                    source_offset_frames: original_offset,
                    gain: 1.0, fade_in_ms: 0, fade_out_ms: 0,
                }),
                scale_override: None,
            });
            project.tracks.push(track);
            let before = project.clone();
            let tick = clip_start + 3 * PPQ + 17;
            let frame_at = |tick| frames_for_ticks(tick, bpm, rate);
            let start_frame = frame_at(clip_start);
            let split_frame = frame_at(tick);
            let plan = plan_split(split_frame + 3, tick, clip_start, clip_start + duration, frame_at)
                .unwrap();
            assert!(split_clip_in_project(
                &mut project, 1, 2, 3, clip_start, clip_start + duration, plan,
            ));
            let clips = &project.tracks[0].clips;
            assert_eq!(clips.len(), 2);
            assert_eq!(clips[0].start.ticks() + clips[0].duration.ticks(),
                clips[1].start.ticks());
            let ClipKind::Audio(left_audio) = &clips[0].kind else { panic!("audio left") };
            let ClipKind::Audio(right_audio) = &clips[1].kind else { panic!("audio right") };
            assert_eq!(left_audio.source_offset_frames, original_offset);
            assert_eq!(right_audio.source_offset_frames,
                original_offset + split_frame - start_frame);
            // Adjacent clips address adjacent source frames, not the cursor's
            // off-grid frame. This also detects one-frame rounding errors.
            let first_right_source_frame = right_audio.source_offset_frames;
            let last_left_source_frame = left_audio.source_offset_frames
                + frames_for_ticks(clips[0].start.ticks()
                    + clips[0].duration.ticks(), bpm, rate)
                - frames_for_ticks(clips[0].start.ticks(), bpm, rate) - 1;
            assert_eq!(first_right_source_frame, last_left_source_frame + 1);
            assert_eq!(left_audio.source, right_audio.source);
            let encoded = serde_json::to_vec(&project).unwrap();
            let reopened: Project = serde_json::from_slice(&encoded).unwrap();
            assert_eq!(reopened, project);
            let mut history = ProjectHistory::new(&before);
            let selection = EditorSelection {
                track: Some(1), clip: Some(2), piano_note: None, clips: vec![2],
            };
            history.observe(&before, selection.clone(), false);
            history.observe(&project, selection.clone(), false);
            assert!(history.undo(&mut project, selection.clone()).is_some());
            assert_eq!(project, before);
            assert!(history.redo(&mut project, selection).is_some());
            assert_eq!(project, reopened);
        }
    }

    #[test]
    fn split_plan_rejects_boundaries_and_zero_length_clips() {
        let frame_at = |tick| frames_for_ticks(tick, 120.0, 44_100);
        assert!(plan_split(0, 0, 0, PPQ, frame_at).is_none());
        assert!(plan_split(frame_at(PPQ), PPQ, 0, PPQ, frame_at).is_none());
        assert!(plan_split(frame_at(PPQ) / 2, PPQ / 2, 0, 0, frame_at).is_none());
    }

    #[test]
    fn left_trim_moves_start_and_clamps_to_available_room() {
        assert_eq!(clamped_left_trim(480, 0, 1920), 480);
        assert_eq!(clamped_left_trim(-480, 0, 1920), 0, "cannot start before zero");
        assert_eq!(clamped_left_trim(9999, 960, 1920), 1919, "one tick must remain");
        assert_eq!(clamped_left_trim(-9999, 960, 1920), -960);
    }

    #[test]
    fn right_trim_never_crosses_source_end() {
        assert_eq!(clamped_right_duration(480, 1920, 5000), 2400);
        assert_eq!(clamped_right_duration(9999, 1920, 2000), 2000);
        assert_eq!(clamped_right_duration(-9999, 1920, 2000), 1);
    }

    #[test]
    fn snapping_rounds_to_sixteenth_grid_and_handles_negative_drag() {
        assert_eq!(snapped_tick_delta(0, true, PPQ / 4), 0);
        assert_eq!(snapped_tick_delta(100, true, PPQ / 4), 0);
        assert_eq!(snapped_tick_delta(130, true, PPQ / 4), PPQ / 4);
        assert_eq!(snapped_tick_delta(-130, true, PPQ / 4), -(PPQ / 4));
        assert_eq!(snapped_tick_delta(137, false, PPQ / 4), 137);
        assert_eq!(snapped_absolute_tick(1800, true, PPQ / 4), PPQ * 2);
    }

    #[test]
    fn midi_left_trim_rebases_and_drops_hidden_notes() {
        let notes = [note(0, 480), note(480, 480), note(960, 480)];
        let trimmed = trim_midi_left(&notes, 480);
        assert_eq!(trimmed.len(), 2);
        assert_eq!(trimmed[0].start.ticks(), 0);
        assert_eq!(trimmed[0].pitch, 60);
        assert_eq!(trimmed[1].start.ticks(), 480);
        // A note crossing the new left edge is shortened, not shifted out.
        let crossing = trim_midi_left(&[note(240, 480)], 300);
        assert_eq!(crossing[0].start.ticks(), 0);
        assert_eq!(crossing[0].duration.ticks(), 420);
    }

    #[test]
    fn left_trim_extension_shifts_midi_notes_right() {
        let shifted = trim_midi_left(&[note(0, 480)], -480);
        assert_eq!(shifted[0].start.ticks(), 480);
        assert_eq!(shifted[0].duration.ticks(), 480);
    }

    #[test]
    fn clip_preview_bounds_keep_right_edge_fixed_for_left_trim() {
        let (x, w) = clip_preview_bounds(100.0, 200.0, 0.0,
            Some((ClipTrimEdge::Left, 40.0)), 14.0);
        assert_eq!((x, w), (140.0, 160.0));
        assert_eq!(x + w, 300.0);
        let (x, w) = clip_preview_bounds(100.0, 200.0, 0.0,
            Some((ClipTrimEdge::Right, 30.0)), 14.0);
        assert_eq!((x, w), (100.0, 230.0));
        let (x, w) = clip_preview_bounds(100.0, 200.0, -25.0, None, 14.0);
        assert_eq!((x, w), (75.0, 200.0));
    }

    #[test]
    fn frame_tick_conversion_round_trips_within_one_tick() {
        for rate in [44_100_u32, 48_000, 96_000] {
            for bpm in [90.0_f64, 120.0, 137.5] {
                let ticks = 7 * PPQ + 123;
                let frames = frames_for_ticks(ticks, bpm, rate);
                let back = ticks_for_frames(frames, bpm, rate);
                assert!((back - ticks).abs() <= 1,
                    "rate {rate} bpm {bpm}: {ticks} -> {frames} -> {back}");
            }
        }
    }

    #[test]
    fn track_compatibility_separates_audio_and_midi() {
        let audio = ClipKind::Audio(AudioClip {
            source: "a.wav".into(), warped: false, source_offset_frames: 0,
            gain: 1.0, fade_in_ms: 0, fade_out_ms: 0,
        });
        let midi = ClipKind::Midi(MidiClip::default());
        assert!(track_kind_accepts_clip(TrackKind::Audio, &audio));
        assert!(!track_kind_accepts_clip(TrackKind::Midi, &audio));
        assert!(track_kind_accepts_clip(TrackKind::Midi, &midi));
        assert!(!track_kind_accepts_clip(TrackKind::Audio, &midi));
        assert!(!track_kind_accepts_clip(TrackKind::Group, &midi));
    }
}
