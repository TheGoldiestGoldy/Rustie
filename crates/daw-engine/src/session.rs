//! Session intentions cross the control boundary as one fixed-size ring item.
//! The render thread chooses the grid line when it observes that item.
use crate::graph::{NodeSpec, Source};
use daw_core::{Clip, ClipKind, PPQ, TempoMap};
use std::sync::{
    Arc,
    atomic::{AtomicU8, AtomicU64, Ordering},
};

pub const SESSION_TRACKS: usize = 48;

#[derive(Debug, Clone, Copy, Default)]
pub enum TrackLaunch {
    #[default]
    Keep,
    Stop,
    Clip(u64),
    Cancel,
}

#[derive(Clone, Copy)]
pub struct SessionLaunch {
    pub grid_ticks: i64,
    pub tracks: [TrackLaunch; SESSION_TRACKS],
}

impl SessionLaunch {
    pub fn new(grid_ticks: i64) -> Self {
        Self {
            grid_ticks: grid_ticks.max(1),
            tracks: [TrackLaunch::Keep; SESSION_TRACKS],
        }
    }
}

#[derive(Debug)]
struct Segment {
    tick: i64,
    frame: f64,
    frames_per_tick: f64,
}

#[derive(Debug)]
pub struct SessionClock(Vec<Segment>);

impl SessionClock {
    pub fn new(map: &TempoMap, rate: u32) -> Result<Self, &'static str> {
        let mut segments: Vec<Segment> = Vec::new();
        for (at, tempo) in map.changes() {
            let tick = at.ticks();
            let frame = segments.last().map_or(0.0, |p| {
                p.frame + (tick - p.tick) as f64 * p.frames_per_tick
            });
            if tempo.micro_bpm() == 0 || rate == 0 {
                return Err("invalid Session clock");
            }
            segments.push(Segment {
                tick,
                frame,
                frames_per_tick: rate as f64 * 60.0 / (tempo.bpm() * PPQ as f64),
            });
        }
        if segments.first().is_none_or(|s| s.tick != 0) {
            return Err("missing initial tempo");
        }
        Ok(Self(segments))
    }
    pub fn frame(&self, tick: i64) -> u64 {
        let index = self.0.partition_point(|s| s.tick <= tick).saturating_sub(1);
        let s = &self.0[index];
        (s.frame + (tick - s.tick) as f64 * s.frames_per_tick).round() as u64
    }
    fn tick(&self, frame: f64) -> f64 {
        let index = self
            .0
            .partition_point(|s| s.frame <= frame)
            .saturating_sub(1);
        let s = &self.0[index];
        s.tick as f64 + (frame - s.frame) / s.frames_per_tick
    }
    fn next_grid(&self, frame: u64, step: i64) -> i64 {
        // Include rounded frame ties even at fractional BPM or sub-frame tick lengths.
        let index = (self.tick(frame as f64 + 0.5) / step as f64).ceil() as i64;
        let candidate = index.saturating_mul(step);
        if self.frame(candidate) <= frame {
            candidate.saturating_add(step)
        } else {
            candidate
        }
    }
}

#[derive(Debug, Default)]
pub struct SlotMeter {
    flags: AtomicU8,
    origin: AtomicU64,
    cycle_start: AtomicU64,
    cycle_end: AtomicU64,
}
impl SlotMeter {
    pub fn progress(&self, frame: u64) -> f32 {
        let start = self.cycle_start.load(Ordering::Acquire);
        let end = self.cycle_end.load(Ordering::Relaxed);
        (frame.saturating_sub(start) as f64 / end.saturating_sub(start).max(1) as f64)
            .clamp(0.0, 1.0) as f32
    }
    pub fn state(&self) -> (bool, bool, u64) {
        let flags = self.flags.load(Ordering::Acquire);
        (
            flags & 1 != 0,
            flags & 2 != 0,
            self.origin.load(Ordering::Relaxed),
        )
    }
}

#[derive(Debug)]
struct NoteEvent {
    tick: i64,
    pitch: u8,
    velocity: u8,
    on: bool,
}

#[derive(Debug)]
pub struct SessionVoice {
    clock: Arc<SessionClock>,
    track: usize,
    clip: u64,
    active: bool,
    envelope: u8,
    pending: Option<(u64, i64, bool)>,
    meter: Arc<SlotMeter>,
    events: Vec<NoteEvent>,
    duration: i64,
    cycle_tick: i64,
    cycle_end: u64,
    next_event: usize,
    next_frame: u64,
}

impl SessionVoice {
    pub fn new(clock: Arc<SessionClock>, track: usize, clip: &Clip, origin: Option<u64>) -> Self {
        let duration = clip.duration.ticks().max(1);
        let mut events = Vec::new();
        if let ClipKind::Midi(midi) = &clip.kind {
            for note in &midi.notes {
                let tick = note.start.ticks();
                if note.muted
                    || note.pitch > 127
                    || !(0..duration).contains(&tick)
                    || note.duration.ticks() <= 0
                {
                    continue;
                }
                events.push(NoteEvent {
                    tick,
                    pitch: note.pitch,
                    velocity: note.velocity,
                    on: true,
                });
                events.push(NoteEvent {
                    tick: duration.min(tick.saturating_add(note.duration.ticks())),
                    pitch: note.pitch,
                    velocity: 0,
                    on: false,
                });
            }
            events.sort_by_key(|e| (e.tick, e.on));
        }
        let meter = Arc::new(SlotMeter::default());
        let active = origin.is_some();
        meter.flags.store(u8::from(active), Ordering::Relaxed);
        meter.origin.store(origin.unwrap_or(0), Ordering::Relaxed);
        let cycle_tick = clock.tick(origin.unwrap_or(0) as f64).round() as i64;
        Self {
            clock,
            track,
            clip: clip.id,
            active,
            pending: None,
            meter,
            events,
            duration,
            cycle_tick,
            cycle_end: 0,
            next_event: 0,
            next_frame: u64::MAX,
            envelope: 0,
        }
    }

    pub fn meter(&self) -> Arc<SlotMeter> {
        Arc::clone(&self.meter)
    }

    pub(crate) fn queue(&mut self, launch: &SessionLaunch, now: u64) {
        let Some(action) = launch.tracks.get(self.track) else {
            return;
        };
        let target = match *action {
            TrackLaunch::Keep => return,
            TrackLaunch::Stop => Some(false),
            TrackLaunch::Clip(id) => Some(id == self.clip),
            TrackLaunch::Cancel => None,
        };
        self.pending = target
            .filter(|&active| active != self.active)
            .map(|active| {
                let tick = self.clock.next_grid(now, launch.grid_ticks.max(1));
                (self.clock.frame(tick), tick, active)
            });
        self.publish();
    }

    fn publish(&self) {
        self.meter.flags.store(
            u8::from(self.active) | (u8::from(self.pending.is_some()) << 1),
            Ordering::Release,
        );
    }

    fn update_next_event(&mut self) {
        self.next_frame = self.events.get(self.next_event).map_or(u64::MAX, |e| {
            self.clock.frame(self.cycle_tick.saturating_add(e.tick))
        });
    }

    fn begin_cycle(&mut self, frame: u64, source: &mut Source) {
        self.cycle_end = self
            .clock
            .frame(self.cycle_tick.saturating_add(self.duration))
            .max(frame.saturating_add(1));
        if let Source::LoopedSamples {
            start_frame,
            loop_frames,
            ..
        } = source
        {
            *start_frame = frame;
            *loop_frames = self.cycle_end - frame;
        }
        self.meter
            .cycle_end
            .store(self.cycle_end, Ordering::Relaxed);
        self.meter.cycle_start.store(frame, Ordering::Release);
        self.next_event = 0;
        self.update_next_event();
    }

    fn emit_due(
        &mut self,
        frame: u64,
        rate: u32,
        source: &mut Source,
        effects: &mut crate::effects::EffectRack,
    ) {
        while self.next_event < self.events.len() && self.next_frame <= frame {
            let e = &self.events[self.next_event];
            if e.on {
                source.note_on(e.pitch, e.velocity, rate);
                effects.note_on(e.pitch, e.velocity);
            } else {
                source.note_off(e.pitch);
                effects.note_off(e.pitch);
            }
            self.next_event += 1;
            self.update_next_event();
        }
    }

    pub(crate) fn advance(
        &mut self,
        frame: u64,
        rate: u32,
        source: &mut Source,
        effects: &mut crate::effects::EffectRack,
    ) -> f32 {
        if let Some((at, tick, active)) = self.pending
            && frame >= at
        {
            self.pending = None;
            self.active = active;
            if active {
                self.cycle_tick = tick;
                self.cycle_end = 0;
                self.meter.origin.store(at, Ordering::Relaxed);
            } else {
                source.all_notes_off();
                effects.all_notes_off();
            }
            self.publish();
        }
        if self.active {
            if self.cycle_end == 0 {
                self.begin_cycle(frame, source);
            }
            self.emit_due(frame, rate, source, effects);
            if frame >= self.cycle_end {
                self.cycle_tick = self.cycle_tick.saturating_add(self.duration);
                self.begin_cycle(frame, source);
                self.emit_due(frame, rate, source, effects);
            }
            self.envelope = self.envelope.saturating_add(1).min(128);
        } else {
            self.envelope = self.envelope.saturating_sub(1);
        }
        f32::from(self.envelope) / 128.0
    }
}

impl NodeSpec {
    pub(crate) fn queue_session(&mut self, launch: &SessionLaunch, now: u64) {
        if let Some(voice) = &mut self.session {
            voice.queue(launch, now);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use daw_core::{MusicalTime, Tempo};

    #[test]
    fn session_grid_before_on_and_after_fractional_bpm_meter_and_tempo_boundaries() {
        for (bar, bpm) in [(PPQ * 4, 120.0), (PPQ * 7 / 2, 137.125), (PPQ * 3, 99.9)] {
            let mut map = TempoMap::new(Tempo::from_bpm(bpm).unwrap());
            map.insert(
                MusicalTime::from_ticks(3 * bar + PPQ / 3),
                Tempo::from_bpm(87.25).unwrap(),
            )
            .unwrap();
            for rate in [44_100, 48_000] {
                let clock = SessionClock::new(&map, rate).unwrap();
                for step in [PPQ / 4, PPQ, bar] {
                    for n in [1, 2, 4, 101, 4096] {
                        let tick = step * n;
                        let frame = map
                            .sample_frame_at(MusicalTime::from_ticks(tick), rate)
                            .unwrap();
                        assert_eq!(clock.frame(tick), frame);
                        assert_eq!(clock.next_grid(frame - 1, step), tick);
                        assert_eq!(clock.next_grid(frame, step), tick + step);
                        assert_eq!(clock.next_grid(frame + 1, step), tick + step);
                    }
                }
            }
        }
    }
}
