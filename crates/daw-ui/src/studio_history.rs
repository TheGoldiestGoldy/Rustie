// Bounded, document-level undo/redo for metadata, clip and note edits.
// A continuous slider/drag or text-entry gesture creates ONE history entry
// when pointer and text focus are released, not one entry per repaint.
// Every entry also stores the editor selection at that moment, so undoing a
// delete restores the deleted clip instead of leaving a dangling selection.
const HISTORY_LIMIT: usize = 64;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct EditorSelection {
    track: Option<u64>,
    clip: Option<u64>,
    piano_note: Option<usize>,
    clips: Vec<u64>,
}

#[derive(Clone)]
struct HistorySnapshot {
    project: Project,
    selection: EditorSelection,
}

struct ProjectHistory {
    last: Project,
    last_selection: EditorSelection,
    pending: Option<HistorySnapshot>,
    undo: Vec<HistorySnapshot>,
    redo: Vec<HistorySnapshot>,
}

impl ProjectHistory {
    fn new(project: &Project) -> Self {
        Self { last: project.clone(), last_selection: EditorSelection::default(),
            pending: None, undo: Vec::new(), redo: Vec::new() }
    }

    /// Observe once per frame. Selection-only changes never create history;
    /// they only update the selection that the next edit will restore.
    fn observe(&mut self, project: &Project, selection: EditorSelection, gesture_active: bool) {
        if self.last != *project {
            if self.pending.is_none() {
                self.pending = Some(HistorySnapshot {
                    project: self.last.clone(),
                    selection: self.last_selection.clone(),
                });
            }
            self.last = project.clone();
        }
        self.last_selection = selection;
        if !gesture_active { self.commit(project); }
    }

    fn commit(&mut self, project: &Project) {
        if let Some(before) = self.pending.take()
            && before.project != *project {
            self.undo.push(before);
            self.redo.clear();
            if self.undo.len() > HISTORY_LIMIT { self.undo.remove(0); }
        }
    }

    fn undo(&mut self, project: &mut Project, selection: EditorSelection)
        -> Option<EditorSelection> {
        self.observe(project, selection.clone(), false);
        let previous = self.undo.pop()?;
        self.redo.push(HistorySnapshot { project: project.clone(), selection });
        *project = previous.project;
        self.last = project.clone();
        self.last_selection = previous.selection.clone();
        Some(previous.selection)
    }

    fn redo(&mut self, project: &mut Project, selection: EditorSelection)
        -> Option<EditorSelection> {
        self.observe(project, selection.clone(), false);
        let next = self.redo.pop()?;
        self.undo.push(HistorySnapshot { project: project.clone(), selection });
        if self.undo.len() > HISTORY_LIMIT { self.undo.remove(0); }
        *project = next.project;
        self.last = project.clone();
        self.last_selection = next.selection.clone();
        Some(next.selection)
    }

    fn can_undo(&self) -> bool {
        !self.undo.is_empty() || self.pending.is_some()
    }

    fn can_redo(&self) -> bool { !self.redo.is_empty() }

    #[cfg(test)]
    fn undo_len(&self) -> usize { self.undo.len() }
}

impl Studio {
    fn editor_selection(&self) -> EditorSelection {
        EditorSelection {
            track: self.selected_track,
            clip: self.selected_clip,
            piano_note: self.piano_selected,
            clips: self.arrangement_selection.clone(),
        }
    }

    /// Restore a historical selection only when the referenced objects still
    /// exist. Never leave a stale clip ID or piano-note index selected.
    fn restore_editor_selection(&mut self, selection: EditorSelection) {
        self.selected_track = selection.track
            .filter(|id| self.project.tracks.iter().any(|t| t.id == *id))
            .or_else(|| self.project.tracks.first().map(|t| t.id));
        self.selected_clip = selection.clip
            .filter(|id| self.project.tracks.iter()
                .any(|t| t.all_clips().any(|c| c.id == *id)));
        self.arrangement_selection = selection.clips;
        self.arrangement_selection.retain(|id| self.project.tracks.iter()
            .any(|t| t.clips.iter().any(|clip| clip.id == *id)));
        self.piano_selected = selection.piano_note.filter(|index| {
            self.selected_clip_pair().is_some_and(|(_, clip)| {
                matches!(&clip.kind, ClipKind::Midi(midi) if *index < midi.notes.len())
            })
        });
    }

    fn history_step(&mut self, redo: bool) {
        if self.recorder.is_some() || self.midi_take.is_some() {
            self.error = Some("Finish recording before Undo or Redo".into());
            return;
        }
        let selection = self.editor_selection();
        let restored = if redo { self.history.redo(&mut self.project, selection) }
            else { self.history.undo(&mut self.project, selection) };
        let Some(restored) = restored else { return };
        self.restore_editor_selection(restored);
        self.session_active.retain(|track,clip|
            self.project.tracks.iter().any(|t| t.id == *track && t.session_slots.values().any(|c|c.id == *clip)));
        self.dirty = true;
        self.status = if redo {"Redid project edit".into()} else {"Undid project edit".into()};
        self.restart_for_edit();
    }
}

#[cfg(test)]
mod history_tests {
    use super::*;

    fn selection(track: u64, clip: u64, note: Option<usize>) -> EditorSelection {
        EditorSelection { track: Some(track), clip: Some(clip), piano_note: note, clips: vec![clip] }
    }

    #[test]
    fn undo_redo_and_drag_coalescing() {
        let mut project = Project::new("Track History",Tempo::from_bpm(120.0).unwrap());
        project.tracks.push(Track::new(1,"Vocals",TrackKind::Audio));
        let mut history=ProjectHistory::new(&project);
        let unchanged = EditorSelection::default();
        for value in [0.5,0.6,0.7,0.8,0.9] {
            project.tracks[0].volume=value;
            history.observe(&project, unchanged.clone(), true);
        }
        assert!(history.can_undo());
        assert_eq!(history.undo_len(),0,"drag should not create many undo entries");
        history.observe(&project, unchanged.clone(), false);
        assert_eq!(history.undo_len(),1);
        assert!(history.undo(&mut project, unchanged.clone()).is_some());
        assert!((project.tracks[0].volume-1.0).abs()<0.0001);
        assert!(history.can_redo());
        assert!(history.redo(&mut project, unchanged.clone()).is_some());
        assert!((project.tracks[0].volume-0.9).abs()<0.0001);
        project.tracks[0].name="New singer".into();
        history.observe(&project, unchanged.clone(), false);
        assert!(!history.can_redo(),"new edit must invalidate redo history");
    }

    #[test]
    fn undo_restores_the_selection_recorded_before_the_edit() {
        let mut project = Project::new("Selection", Tempo::default());
        let mut track = Track::new(1, "Lead", TrackKind::Midi);
        track.clips.push(Clip {
            id: 2, start: MusicalTime::ZERO,
            duration: MusicalTime::from_ticks(PPQ),
            kind: ClipKind::Midi(MidiClip { notes: vec![MidiNote {
                pitch: 60, velocity: 100, muted: false,
                start: MusicalTime::ZERO, duration: MusicalTime::from_ticks(PPQ),
            }]}),
            scale_override: None,
        });
        project.tracks.push(track);
        let mut history = ProjectHistory::new(&project);
        let before = selection(1, 2, Some(0));
        // A normal frame records the pre-edit selection before the gesture.
        history.observe(&project, before.clone(), false);
        project.tracks[0].clips[0].duration = MusicalTime::from_ticks(PPQ * 2);
        let after = selection(1, 2, Some(0));
        history.observe(&project, after.clone(), false);
        let restored = history.undo(&mut project, after.clone()).unwrap();
        assert_eq!(restored, before);
        assert_eq!(restored.clip, Some(2));
        let redone = history.redo(&mut project, before).unwrap();
        assert_eq!(redone, after);
    }

    #[test]
    fn selection_only_changes_do_not_create_history_entries() {
        let project = Project::new("No edits", Tempo::default());
        let mut history = ProjectHistory::new(&project);
        history.observe(&project, selection(9, 9, Some(3)), false);
        history.observe(&project, selection(10, 11, None), false);
        assert_eq!(history.undo_len(), 0);
        assert!(!history.can_undo());
    }

    #[test]
    fn multi_step_operations_create_one_entry_per_committed_step() {
        let mut project = Project::new("Steps", Tempo::default());
        project.tracks.push(Track::new(1, "A", TrackKind::Midi));
        let mut history = ProjectHistory::new(&project);
        let nothing = EditorSelection::default();
        project.tracks.push(Track::new(2, "B", TrackKind::Midi));
        history.observe(&project, nothing.clone(), false);
        project.tracks.push(Track::new(3, "C", TrackKind::Midi));
        history.observe(&project, nothing.clone(), false);
        assert_eq!(history.undo_len(), 2);
        history.undo(&mut project, nothing.clone());
        assert_eq!(project.tracks.len(), 2);
        history.undo(&mut project, nothing.clone());
        assert_eq!(project.tracks.len(), 1);
    }

    #[test]
    fn history_is_bounded() {
        let mut project = Project::new("Bounded", Tempo::default());
        project.tracks.push(Track::new(1, "A", TrackKind::Midi));
        let mut history = ProjectHistory::new(&project);
        let nothing = EditorSelection::default();
        for value in 0..(HISTORY_LIMIT + 12) {
            project.tracks[0].volume = value as f32 / 100.0;
            history.observe(&project, nothing.clone(), false);
        }
        assert_eq!(history.undo_len(), HISTORY_LIMIT);
    }

    #[test]
    fn restoring_a_stale_selection_falls_back_to_real_objects() {
        // The validation rule lives in Studio; this captures the contract used
        // by restore_editor_selection without constructing a full audio Studio.
        let project = Project::new("Fallback", Tempo::default());
        let stale = EditorSelection { track: Some(99), clip: Some(98), piano_note: Some(7), clips: vec![98] };
        let track = stale.track.filter(|id| project.tracks.iter().any(|t| t.id == *id))
            .or_else(|| project.tracks.first().map(|t| t.id));
        let clip = stale.clip.filter(|id| project.tracks.iter()
            .any(|t| t.all_clips().any(|c| c.id == *id)));
        assert_eq!(track, None);
        assert_eq!(clip, None);
    }
}
