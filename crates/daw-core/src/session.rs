//! Sparse Session cells have the stable identity (track ID, scene ID).
use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::{ClipKind, Project, TrackKind};

pub const MAX_SESSION_CLIPS: usize = 48;
pub const MAX_SCENES: usize = 48;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub enum LaunchGrid {
    Sixteenth,
    Quarter,
    #[default]
    Bar,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Session {
    pub scenes: Vec<u64>,
    #[serde(default)]
    pub quantization: LaunchGrid,
}

impl Default for Session {
    fn default() -> Self {
        Self {
            scenes: (1..=8).collect(),
            quantization: LaunchGrid::Bar,
        }
    }
}

impl Project {
    pub fn next_object_id(&self) -> Option<u64> {
        self.tracks
            .iter()
            .flat_map(|t| std::iter::once(t.id).chain(t.all_clips().map(|c| c.id)))
            .max()
            .unwrap_or(0)
            .checked_add(1)
    }

    /// Called before installing a deserialized project or decoding its assets.
    pub fn prepare_session(&mut self) -> Result<(), &'static str> {
        let mut ids = HashSet::new();
        for track in &self.tracks {
            if !ids.insert(track.id) || !track.all_clips().all(|c| ids.insert(c.id)) {
                return Err("duplicate track or clip ID");
            }
        }
        let count: usize = self.tracks.iter().map(|t| t.clips.len()).sum();
        if count > 48 || self.tracks.len() > 48 {
            return Err("project exceeds track or arrangement clip limit");
        }
        if self.session.is_none() {
            if self.tracks.iter().any(|t| !t.session_slots.is_empty()) {
                return Err("Session slots require scenes");
            }
            let rows = self
                .tracks
                .iter()
                .map(|t| t.clips.len())
                .max()
                .unwrap_or(0)
                .max(8);
            let mut id = self.next_object_id().ok_or("ID space exhausted")?;
            id.checked_add(count as u64).ok_or("ID space exhausted")?;
            for t in &mut self.tracks {
                if !matches!(t.kind, TrackKind::Midi | TrackKind::Audio) {
                    continue;
                }
                for (row, original) in t.clips.iter().enumerate() {
                    let mut clip = original.clone();
                    clip.id = id;
                    id += 1;
                    clip.start = crate::MusicalTime::ZERO;
                    t.session_slots.insert(row as u64 + 1, clip);
                }
            }
            self.session = Some(Session {
                scenes: (1..=rows as u64).collect(),
                ..Session::default()
            });
        }
        let session = self.session.as_ref().unwrap();
        let scenes: HashSet<_> = session.scenes.iter().copied().collect();
        if scenes.len() != session.scenes.len() || scenes.len() > MAX_SCENES || scenes.is_empty() {
            return Err("invalid or duplicate Session scenes");
        }
        if self
            .tracks
            .iter()
            .map(|t| t.session_slots.len())
            .sum::<usize>()
            > MAX_SESSION_CLIPS
        {
            return Err("too many Session clips");
        }
        for track in &self.tracks {
            for (scene, clip) in &track.session_slots {
                if !scenes.contains(scene)
                    || clip.duration.ticks() <= 0
                    || !matches!(
                        (track.kind, &clip.kind),
                        (TrackKind::Audio, ClipKind::Audio(_))
                            | (TrackKind::Midi, ClipKind::Midi(_))
                    )
                {
                    return Err("invalid Session slot assignment");
                }
            }
        }
        self.next_object_id().ok_or("ID space exhausted")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Clip, MidiClip, MusicalTime, PPQ, Tempo, Track};

    fn legacy() -> Project {
        let mut project = Project::new("Review", Tempo::default());
        let mut track = Track::new(7, "Piano", TrackKind::Midi);
        track.clips.push(Clip {
            id: 9,
            start: MusicalTime::from_ticks(PPQ),
            duration: MusicalTime::from_ticks(2 * PPQ),
            kind: ClipKind::Midi(MidiClip::default()),
            scale_override: None,
        });
        project.tracks.push(track);
        project.tracks.push(Track::new(8, "Bus", TrackKind::Group));
        let mut old = serde_json::to_value(project).unwrap();
        old.as_object_mut().unwrap().remove("session");
        serde_json::from_value(old).unwrap()
    }

    #[test]
    fn legacy_migration_then_sparse_reordered_scene_roundtrip() {
        let mut p = legacy();
        let arrangement = p.tracks[0].clips.clone();
        p.prepare_session().unwrap();
        assert_eq!(p.tracks[0].clips, arrangement);
        assert!(p.tracks[1].session_slots.is_empty());
        let mut slot = p.tracks[0].session_slots.remove(&1).unwrap();
        assert_eq!(slot.id, 10);
        assert_eq!(slot.start, MusicalTime::ZERO);
        slot.duration = MusicalTime::from_ticks(PPQ / 2);
        p.tracks[0].session_slots.insert(8, slot);
        let layout = p.session.as_mut().unwrap();
        layout.scenes.reverse();
        layout.quantization = LaunchGrid::Sixteenth;
        let mut restored: Project =
            serde_json::from_slice(&serde_json::to_vec(&p).unwrap()).unwrap();
        restored.prepare_session().unwrap();
        assert_eq!(p, restored);
        assert_eq!(restored.tracks[0].clips, arrangement);
        assert_eq!(restored.next_object_id(), Some(11));
        restored.tracks[0].session_slots.clear();
        restored.prepare_session().unwrap();
        assert!(restored.tracks[0].session_slots.is_empty());
    }

    #[test]
    fn reject_orphan_scene_duplicate_ids_and_bus_cells() {
        let mut p = legacy();
        p.prepare_session().unwrap();
        let slot = p.tracks[0].session_slots.remove(&1).unwrap();
        p.tracks[0].session_slots.insert(99, slot.clone());
        assert!(p.prepare_session().is_err());
        p.tracks[0].session_slots.clear();
        p.tracks[1].session_slots.insert(1, slot.clone());
        assert!(p.prepare_session().is_err());
        p.tracks[1].session_slots.clear();
        p.tracks[0].session_slots.insert(1, Clip { id: 9, ..slot });
        assert!(p.prepare_session().is_err());
    }
}
