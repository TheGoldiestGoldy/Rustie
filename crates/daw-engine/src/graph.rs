//! Precompiled DAG for track/return/master routing. Construct or edit ONLY off the audio thread.
//! Every buffer, edge delay, and node is allocated before the processor starts.

use crate::effects::EffectRack;
use crate::instruments::Instrument;
use crate::live_plugin::LiveMidiMessage;
use crate::monitor::RecordingMonitor;
use std::f32::consts::TAU;

// 48 arrangement/session clips plus MIDI monitor nodes, buses and master.
const MAX_NODES: usize = 128;
const MAX_VOICES: usize = 16;
const MAX_DELAY_SAMPLES: usize = 262_144;

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct StereoFrame {
    pub left: f32,
    pub right: f32,
}

impl StereoFrame {
    pub const ZERO: Self = Self {
        left: 0.0,
        right: 0.0,
    };

    fn scaled(self, amount: f32) -> Self {
        Self {
            left: self.left * amount,
            right: self.right * amount,
        }
    }

    fn add(&mut self, other: Self) {
        self.left += other.left;
        self.right += other.right;
    }
}

#[derive(Debug, Clone, Copy)]
struct Voice {
    note: u8,
    phase: f32,
    phase_step: f32,
    velocity: f32,
    active: bool,
}

impl Voice {
    const SILENT: Self = Self {
        note: 0,
        phase: 0.0,
        phase_step: 0.0,
        velocity: 0.0,
        active: false,
    };
}

#[derive(Debug)]
pub enum Source {
    Silence,
    /// Input-to-output monitoring only while armed. The raw microphone path
    /// never overwrites recorded WAV data and is disabled by default.
    LiveInput {
        monitor: std::sync::Arc<RecordingMonitor>,
        cursor: u64,
    },
    /// Sample-aligned synthesized metronome; first beat of the bar is accented.
    /// No samples, timers or allocations are required in the audio callback.
    Metronome {
        bpm: f64,
        beats_per_bar: u8,
        beat_denominator: u8,
    },
    /// Lightweight built-in diagnostic generator; no disk or allocations on RT.
    Tone {
        frequency_hz: f32,
        phase: f32,
    },
    /// Buffers must be prepared/resampled by the control thread, in project sample rate.
    Samples {
        frames: std::sync::Arc<[StereoFrame]>,
        start_frame: u64,
    },
    /// Arrangement playback bounded by a clip's edited end position.
    /// Non-destructive trim prevents audio from continuing past the visible clip.
    TrimmedSamples {
        frames: std::sync::Arc<[StereoFrame]>,
        start_frame: u64,
        end_frame: u64,
        /// Non-destructive source start offset for split clips.
        source_offset_frames: u64,
        gain: f32,
        fade_in_frames: u64,
        fade_out_frames: u64,
    },
    /// Session-mode playback with zero allocation in the audio callback.
    /// Short samples are silent for the remainder of a quantized loop.
    LoopedSamples {
        frames: std::sync::Arc<[StereoFrame]>,
        start_frame: u64,
        loop_frames: u64,
        source_offset_frames: u64,
        gain: f32,
        fade_in_frames: u64,
        fade_out_frames: u64,
    },
    /// Fixed 16-voice sine synth for MIDI engine verification (not a plugin replacement).
    MidiSine {
        voices: Box<[MidiVoice; MAX_VOICES]>,
        next_voice: usize,
    },
    /// Built-in SoundFont instrument engine (rustysynth). Owned exclusively by
    /// the render thread; presets swap in through a lock-free ring inside the
    /// instrument, never through a graph rebuild.
    SoundFont {
        instrument: Box<dyn Instrument>,
    },
}

#[derive(Debug, Clone, Copy)]
pub struct MidiVoice(Voice);

/// All operations are scalar and realtime-safe. A linear envelope decays
/// before each clip boundary, avoiding abrupt jumps when editing vocals.
fn envelope_gain(
    gain: f32,
    cursor: u64,
    duration: u64,
    fade_in_frames: u64,
    fade_out_frames: u64,
) -> f32 {
    let gain = if gain.is_finite() {
        gain.clamp(0.0, 4.0)
    } else {
        0.0
    };
    let entry = if fade_in_frames == 0 {
        1.0
    } else {
        (cursor as f32 / fade_in_frames as f32).clamp(0.0, 1.0)
    };
    let exit = if fade_out_frames == 0 {
        1.0
    } else {
        (duration.saturating_sub(cursor) as f32 / fade_out_frames as f32).clamp(0.0, 1.0)
    };
    gain * entry.min(exit)
}

impl Source {
    pub fn midi_sine() -> Self {
        Self::MidiSine {
            voices: Box::new([MidiVoice(Voice::SILENT); MAX_VOICES]),
            next_voice: 0,
        }
    }

    fn sample(&mut self, position: u64, sample_rate: u32) -> StereoFrame {
        match self {
            Self::Silence => StereoFrame::ZERO,
            Self::LiveInput { monitor, cursor } => monitor.take(cursor),
            Self::Metronome {
                bpm,
                beats_per_bar,
                beat_denominator,
            } => {
                // BPM is measured in quarter notes; the time signature's
                // denominator sets the spacing of the audible subdivisions.
                // E.g. 6/8 at 120 BPM clicks every eighth (0.25 seconds),
                // with the first of six notes accented.
                let frames_per_beat = sample_rate as f64 * 60.0 / *bpm
                    * (4.0 / f64::from((*beat_denominator).max(1)));
                if !frames_per_beat.is_finite() || frames_per_beat < 1.0 {
                    return StereoFrame::ZERO;
                }
                let beat = (position as f64 / frames_per_beat).floor() as u64;
                let beat_start = (beat as f64 * frames_per_beat).round() as u64;
                let local = position.saturating_sub(beat_start);
                let duration = (sample_rate as f64 * 0.035) as u64;
                if local >= duration {
                    return StereoFrame::ZERO;
                }
                let accent = beat.is_multiple_of(u64::from((*beats_per_bar).max(1)));
                let freq = if accent { 1600.0_f32 } else { 1050.0_f32 };
                let decay = (-8.0 * local as f32 / duration.max(1) as f32).exp();
                let click = (std::f32::consts::TAU * freq * local as f32 / sample_rate as f32)
                    .sin()
                    * decay
                    * if accent { 0.18 } else { 0.11 };
                StereoFrame {
                    left: click,
                    right: click,
                }
            }
            Self::Tone {
                frequency_hz,
                phase,
            } => {
                let amplitude = phase.sin() * 0.15;
                *phase += TAU * *frequency_hz / sample_rate as f32;
                if *phase >= TAU {
                    *phase -= TAU;
                }
                StereoFrame {
                    left: amplitude,
                    right: amplitude,
                }
            }
            Self::Samples {
                frames,
                start_frame,
            } => {
                match position
                    .checked_sub(*start_frame)
                    .and_then(|offset| frames.get(offset as usize))
                {
                    Some(sample) => *sample,
                    None => StereoFrame::ZERO,
                }
            }
            Self::TrimmedSamples {
                frames,
                start_frame,
                end_frame,
                source_offset_frames,
                gain,
                fade_in_frames,
                fade_out_frames,
            } => {
                if position >= *end_frame {
                    StereoFrame::ZERO
                } else if let Some(relative) = position.checked_sub(*start_frame) {
                    let factor = envelope_gain(
                        *gain,
                        relative,
                        end_frame.saturating_sub(*start_frame),
                        *fade_in_frames,
                        *fade_out_frames,
                    );
                    relative
                        .checked_add(*source_offset_frames)
                        .and_then(|offset| usize::try_from(offset).ok())
                        .and_then(|offset| frames.get(offset))
                        .copied()
                        .unwrap_or(StereoFrame::ZERO)
                        .scaled(factor)
                } else {
                    StereoFrame::ZERO
                }
            }
            Self::LoopedSamples {
                frames,
                start_frame,
                loop_frames,
                source_offset_frames,
                gain,
                fade_in_frames,
                fade_out_frames,
            } => match position.checked_sub(*start_frame) {
                Some(offset) if *loop_frames > 0 => {
                    let local = offset % *loop_frames;
                    let factor = envelope_gain(
                        *gain,
                        local,
                        (*loop_frames)
                            .min((frames.len() as u64).saturating_sub(*source_offset_frames)),
                        *fade_in_frames,
                        *fade_out_frames,
                    );
                    local
                        .checked_add(*source_offset_frames)
                        .and_then(|offset| usize::try_from(offset).ok())
                        .and_then(|offset| frames.get(offset))
                        .copied()
                        .unwrap_or(StereoFrame::ZERO)
                        .scaled(factor)
                }
                _ => StereoFrame::ZERO,
            },
            Self::MidiSine { voices, .. } => {
                let mut amplitude = 0.0;
                for voice in voices.iter_mut() {
                    if voice.0.active {
                        amplitude += voice.0.phase.sin() * voice.0.velocity * 0.06;
                        voice.0.phase += voice.0.phase_step;
                        if voice.0.phase >= TAU {
                            voice.0.phase -= TAU;
                        }
                    }
                }
                StereoFrame {
                    left: amplitude,
                    right: amplitude,
                }
            }
            Self::SoundFont { instrument } => {
                // One sample at a time; rustysynth internally buffers into its
                // configured block size, so this does not allocate per frame.
                let mut left = [0.0_f32; 1];
                let mut right = [0.0_f32; 1];
                instrument.render(&mut left, &mut right);
                StereoFrame {
                    left: left[0],
                    right: right[0],
                }
            }
        }
    }

    pub(crate) fn note_on(&mut self, note: u8, velocity: u8, sample_rate: u32) {
        if note > 127 || velocity == 0 {
            if velocity == 0 {
                self.note_off(note);
            }
            return;
        }
        if let Self::MidiSine { voices, next_voice } = self {
            let slot = voices
                .iter()
                .position(|voice| !voice.0.active)
                .unwrap_or(*next_voice);
            voices[slot] = MidiVoice(Voice {
                note,
                phase: 0.0,
                phase_step: TAU * 440.0 * 2.0_f32.powf((note as f32 - 69.0) / 12.0)
                    / sample_rate as f32,
                velocity: velocity as f32 / 127.0,
                active: true,
            });
            *next_voice = (slot + 1) % MAX_VOICES;
        }
        if let Self::SoundFont { instrument } = self {
            instrument.note_on(note, velocity);
        }
    }

    pub(crate) fn note_off(&mut self, note: u8) {
        if let Self::MidiSine { voices, .. } = self {
            for voice in voices.iter_mut() {
                if voice.0.note == note {
                    voice.0.active = false;
                }
            }
        }
        if let Self::SoundFont { instrument } = self {
            instrument.note_off(note);
        }
    }

    pub(crate) fn all_notes_off(&mut self) {
        if let Self::MidiSine { voices, .. } = self {
            for voice in voices.iter_mut() {
                voice.0.active = false;
            }
        }
        if let Self::SoundFont { instrument } = self {
            instrument.all_notes_off();
        }
    }

    pub(crate) fn pitch_bend(&mut self, value: u16) {
        if let Self::SoundFont { instrument } = self {
            instrument.pitch_bend(value);
        }
    }

    pub(crate) fn control_change(&mut self, controller: u8, value: u8) {
        if let Self::SoundFont { instrument } = self {
            instrument.control_change(controller, value);
        }
    }

    pub(crate) fn program_change(&mut self, program: u8) {
        if let Self::SoundFont { instrument } = self {
            instrument.program_change(program);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeKind {
    AudioTrack,
    MidiTrack,
    Return,
    Master,
}

#[derive(Debug)]
pub struct NodeSpec {
    pub id: u32,
    pub kind: NodeKind,
    pub gain: f32,
    pub pan: f32,
    pub latency_samples: usize,
    pub source: Source,
    pub effects: EffectRack,
    pub session: Option<crate::session::SessionVoice>,
}

impl NodeSpec {
    pub fn audio(id: u32, source: Source) -> Self {
        Self {
            id,
            kind: NodeKind::AudioTrack,
            gain: 1.0,
            pan: 0.0,
            latency_samples: 0,
            source,
            effects: EffectRack::default(),
            session: None,
        }
    }
    pub fn midi(id: u32) -> Self {
        Self {
            id,
            kind: NodeKind::MidiTrack,
            gain: 1.0,
            pan: 0.0,
            latency_samples: 0,
            source: Source::midi_sine(),
            effects: EffectRack::default(),
            session: None,
        }
    }
    pub fn returns(id: u32) -> Self {
        Self {
            id,
            kind: NodeKind::Return,
            gain: 1.0,
            pan: 0.0,
            latency_samples: 0,
            source: Source::Silence,
            effects: EffectRack::default(),
            session: None,
        }
    }
    pub fn master(id: u32) -> Self {
        Self {
            id,
            kind: NodeKind::Master,
            gain: 1.0,
            pan: 0.0,
            latency_samples: 0,
            source: Source::Silence,
            effects: EffectRack::default(),
            session: None,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Route {
    pub from: u32,
    pub to: u32,
    pub gain: f32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GraphError {
    InvalidSampleRate,
    TooManyNodes,
    DuplicateId(u32),
    MissingMaster,
    MultipleMasters,
    InvalidRoute,
    RoutingCycle,
    ExcessiveLatency,
    NonFiniteParameter,
}

#[derive(Debug)]
struct DelayLine {
    buffer: Box<[StereoFrame]>,
    cursor: usize,
    valid_frames: usize,
}

impl DelayLine {
    fn new(samples: usize) -> Self {
        Self {
            buffer: vec![StereoFrame::ZERO; samples].into(),
            cursor: 0,
            valid_frames: 0,
        }
    }

    fn process(&mut self, input: StereoFrame) -> StereoFrame {
        if self.buffer.is_empty() {
            return input;
        }
        let output = if self.valid_frames == self.buffer.len() {
            self.buffer[self.cursor]
        } else {
            self.valid_frames += 1;
            StereoFrame::ZERO
        };
        self.buffer[self.cursor] = input;
        self.cursor += 1;
        if self.cursor == self.buffer.len() {
            self.cursor = 0;
        }
        output
    }

    fn delay_samples(&self) -> usize {
        self.buffer.len()
    }

    /// Constant-time reset: stale buffer values are ignored until re-primed.
    fn clear(&mut self) {
        self.cursor = 0;
        self.valid_frames = 0;
    }
}

#[derive(Debug)]
struct Edge {
    destination: usize,
    gain: f32,
    delay: DelayLine,
}

pub struct AudioGraph {
    sample_rate: u32,
    nodes: Vec<NodeSpec>,
    edges: Vec<Edge>,
    outgoing: Vec<Vec<usize>>,
    topological_order: Vec<usize>,
    scratch: Vec<StereoFrame>,
    master: usize,
}

impl AudioGraph {
    pub(crate) fn queue_session(&mut self, launch: &crate::session::SessionLaunch, now: u64) {
        for node in &mut self.nodes {
            node.queue_session(launch, now);
        }
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn delay_for_route(&self, from: u32, to: u32) -> Option<usize> {
        let src_idx = self.nodes.iter().position(|n| n.id == from)?;
        let dest_idx = self.nodes.iter().position(|n| n.id == to)?;
        self.outgoing[src_idx]
            .iter()
            .find(|&&i| self.edges[i].destination == dest_idx)
            .map(|&i| self.edges[i].delay.delay_samples())
    }

    pub fn gain_of(&self, id: u32) -> Option<f32> {
        self.nodes
            .iter()
            .find(|node| node.id == id)
            .map(|node| node.gain)
    }

    pub fn set_gain(&mut self, id: u32, gain: f32) {
        if let Some(node) = self.nodes.iter_mut().find(|node| node.id == id)
            && gain.is_finite()
        {
            node.gain = gain.clamp(0.0, 4.0);
        }
    }

    pub fn set_pan(&mut self, id: u32, pan: f32) {
        if let Some(node) = self.nodes.iter_mut().find(|node| node.id == id)
            && pan.is_finite()
        {
            node.pan = pan.clamp(-1.0, 1.0);
        }
    }

    pub fn set_route_gain(&mut self, from: u32, to: u32, gain: f32) {
        if !gain.is_finite() {
            return;
        }
        if let (Some(a), Some(b)) = (
            self.nodes.iter().position(|n| n.id == from),
            self.nodes.iter().position(|n| n.id == to),
        ) {
            for &edge in &self.outgoing[a] {
                if self.edges[edge].destination == b {
                    self.edges[edge].gain = gain.clamp(0.0, 4.0);
                }
            }
        }
    }

    pub fn note_on(&mut self, id: u32, note: u8, velocity: u8) {
        self.live_midi(id, 0, LiveMidiMessage::NoteOn(note, velocity));
    }

    pub fn note_off(&mut self, id: u32, note: u8) {
        self.live_midi(id, 0, LiveMidiMessage::NoteOff(note));
    }

    /// Kill only this MIDI clip's voices when a Session slot switches.
    pub fn all_notes_off(&mut self, id: u32) {
        self.live_midi(id, 0, LiveMidiMessage::Panic);
    }

    pub fn live_midi(&mut self, id: u32, channel: u8, message: LiveMidiMessage) {
        if let Some(node) = self
            .nodes
            .iter_mut()
            .find(|n| n.id == id && n.kind == NodeKind::MidiTrack)
        {
            match message {
                LiveMidiMessage::NoteOn(note, velocity) => {
                    node.source.note_on(note, velocity, self.sample_rate);
                }
                LiveMidiMessage::NoteOff(note) => node.source.note_off(note),
                LiveMidiMessage::Sustain(value) => node.source.control_change(64, value),
                LiveMidiMessage::PitchBend(value) => node.source.pitch_bend(value),
                LiveMidiMessage::ChannelPanic => node.source.control_change(64, 0),
                LiveMidiMessage::Panic => node.source.all_notes_off(),
            }
            node.effects.live_midi(channel, message);
        }
    }

    pub fn control_change(&mut self, id: u32, controller: u8, value: u8) {
        if let Some(node) = self
            .nodes
            .iter_mut()
            .find(|n| n.id == id && n.kind == NodeKind::MidiTrack)
        {
            node.source.control_change(controller, value);
        }
    }

    pub fn program_change(&mut self, id: u32, program: u8) {
        if let Some(node) = self
            .nodes
            .iter_mut()
            .find(|n| n.id == id && n.kind == NodeKind::MidiTrack)
        {
            node.source.program_change(program);
        }
    }

    /// Re-anchor a preloaded Session audio loop at a future launch boundary.
    /// No graph rebuild or allocation is needed in the render callback.
    pub fn set_loop_start(&mut self, id: u32, frame: u64) {
        if let Some(node) = self.nodes.iter_mut().find(|n| n.id == id)
            && let Source::LoopedSamples { start_frame, .. } = &mut node.source
        {
            *start_frame = frame;
        }
    }

    pub fn reset(&mut self) {
        for node in &mut self.nodes {
            node.source.all_notes_off();
            node.effects.reset();
        }
        for edge in &mut self.edges {
            edge.delay.clear();
        }
    }

    /// One frame of signal flow, no allocation, file IO, locks, or sleeping.
    /// The graph and all routes must remain structurally unchanged while rendering.
    pub fn render_frame(&mut self, project_frame: u64) -> StereoFrame {
        self.render_frame_scoped(project_frame, true)
    }

    /// Audition live MIDI when transport is stopped. Route synths through
    /// groups and master, but never play an audio clip or advance its clock.
    pub fn render_midi_frame(&mut self, project_frame: u64) -> StereoFrame {
        self.render_frame_scoped(project_frame, false)
    }

    fn render_frame_scoped(&mut self, project_frame: u64, with_audio: bool) -> StereoFrame {
        self.scratch.fill(StereoFrame::ZERO);
        for &index in &self.topological_order {
            let node = &mut self.nodes[index];
            let gate = if let Some(session) = &mut node.session {
                if with_audio {
                    session.advance(
                        project_frame,
                        self.sample_rate,
                        &mut node.source,
                        &mut node.effects,
                    )
                } else {
                    0.0
                }
            } else {
                1.0
            };
            let source = if with_audio || node.kind == NodeKind::MidiTrack {
                node.source.sample(project_frame, self.sample_rate)
            } else {
                StereoFrame::ZERO
            };
            let mut result = self.scratch[index];
            result.add(source);
            result = node.effects.process(result).scaled(gate);
            result.left *= node.gain * if node.pan > 0.0 { 1.0 - node.pan } else { 1.0 };
            result.right *= node.gain * if node.pan < 0.0 { 1.0 + node.pan } else { 1.0 };
            self.scratch[index] = result;
            for &edge_id in &self.outgoing[index] {
                let edge = &mut self.edges[edge_id];
                let signal = edge.delay.process(result.scaled(edge.gain));
                self.scratch[edge.destination].add(signal);
            }
        }
        self.scratch[self.master]
    }
}

pub struct GraphBuilder {
    sample_rate: u32,
    nodes: Vec<NodeSpec>,
    routes: Vec<Route>,
}

impl GraphBuilder {
    pub fn new(sample_rate: u32) -> Self {
        Self {
            sample_rate,
            nodes: Vec::new(),
            routes: Vec::new(),
        }
    }

    pub fn add_node(&mut self, node: NodeSpec) -> &mut Self {
        self.nodes.push(node);
        self
    }

    pub fn route(&mut self, from: u32, to: u32, gain: f32) -> &mut Self {
        self.routes.push(Route { from, to, gain });
        self
    }

    pub fn build(self) -> Result<AudioGraph, GraphError> {
        let count = self.nodes.len();
        if self.sample_rate == 0 {
            return Err(GraphError::InvalidSampleRate);
        }
        if count > MAX_NODES {
            return Err(GraphError::TooManyNodes);
        }
        let masters = self
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| n.kind == NodeKind::Master)
            .map(|(i, _)| i)
            .collect::<Vec<_>>();
        let master = match masters.as_slice() {
            [] => return Err(GraphError::MissingMaster),
            [index] => *index,
            _ => return Err(GraphError::MultipleMasters),
        };
        for (i, node) in self.nodes.iter().enumerate() {
            if self.nodes[..i]
                .iter()
                .any(|existing| existing.id == node.id)
            {
                return Err(GraphError::DuplicateId(node.id));
            }
            if node.latency_samples > MAX_DELAY_SAMPLES {
                return Err(GraphError::ExcessiveLatency);
            }
            if !node.gain.is_finite() || !node.pan.is_finite() {
                return Err(GraphError::NonFiniteParameter);
            }
            if node.kind == NodeKind::Master && i != master {
                return Err(GraphError::MultipleMasters);
            }
        }

        let mut degrees = vec![0_usize; count];
        let mut outgoing = vec![Vec::<usize>::new(); count];
        let mut routes = Vec::new();
        for route in self.routes {
            if !route.gain.is_finite() {
                return Err(GraphError::NonFiniteParameter);
            }
            let a = self
                .nodes
                .iter()
                .position(|n| n.id == route.from)
                .ok_or(GraphError::InvalidRoute)?;
            let b = self
                .nodes
                .iter()
                .position(|n| n.id == route.to)
                .ok_or(GraphError::InvalidRoute)?;
            if a == b
                || a == master
                || self.nodes[b].kind == NodeKind::AudioTrack
                || self.nodes[b].kind == NodeKind::MidiTrack
            {
                return Err(GraphError::InvalidRoute);
            }
            degrees[b] += 1;
            outgoing[a].push(routes.len());
            routes.push((a, b, route.gain));
        }

        // Kahn's DAG ordering, deterministic based on user node insertion order.
        let mut queue = std::collections::VecDeque::new();
        for (i, &degree) in degrees.iter().enumerate() {
            if degree == 0 {
                queue.push_back(i);
            }
        }
        let mut order = Vec::with_capacity(count);
        while let Some(a) = queue.pop_front() {
            order.push(a);
            for &edge_index in &outgoing[a] {
                let (_, b, _) = routes[edge_index];
                degrees[b] -= 1;
                if degrees[b] == 0 {
                    queue.push_back(b);
                }
            }
        }
        if order.len() != count {
            return Err(GraphError::RoutingCycle);
        }

        // Longest path through the DAG including each node's *declared* DSP latency.
        let mut arrival = vec![0_usize; count];
        for &a in &order {
            arrival[a] = arrival[a]
                .checked_add(self.nodes[a].latency_samples)
                .ok_or(GraphError::ExcessiveLatency)?;
            if arrival[a] > MAX_DELAY_SAMPLES {
                return Err(GraphError::ExcessiveLatency);
            }
            for &edge_index in &outgoing[a] {
                let (_, b, _) = routes[edge_index];
                arrival[b] = arrival[b].max(arrival[a]);
            }
        }
        // Each destination is aligned to the slowest incoming path.
        let mut edges = Vec::with_capacity(routes.len());
        for (a, b, gain) in routes {
            let destination_input_latency = arrival[b]
                .checked_sub(self.nodes[b].latency_samples)
                .ok_or(GraphError::ExcessiveLatency)?;
            let compensation = destination_input_latency.saturating_sub(arrival[a]);
            if compensation > MAX_DELAY_SAMPLES {
                return Err(GraphError::ExcessiveLatency);
            }
            edges.push(Edge {
                destination: b,
                gain,
                delay: DelayLine::new(compensation),
            });
        }

        Ok(AudioGraph {
            sample_rate: self.sample_rate,
            scratch: vec![StereoFrame::ZERO; count],
            nodes: self.nodes,
            edges,
            outgoing,
            topological_order: order,
            master,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone_graph() -> AudioGraph {
        let mut b = GraphBuilder::new(48_000);
        b.add_node(NodeSpec::audio(
            1,
            Source::Tone {
                frequency_hz: 440.0,
                phase: 0.0,
            },
        ));
        b.add_node(NodeSpec::master(99));
        b.route(1, 99, 1.0);
        b.build().unwrap()
    }

    #[test]
    fn tone_is_audible_and_gain_changes_are_in_place() {
        let mut graph = tone_graph();
        assert_eq!(graph.render_frame(0), StereoFrame::ZERO);
        let a = graph.render_frame(1);
        assert!(a.left > 0.0 && a.left == a.right);
        graph.set_gain(1, 0.0);
        assert_eq!(graph.render_frame(2), StereoFrame::ZERO);
    }

    #[test]
    fn mono_audio_buffer_feeds_the_master_without_allocating_during_render() {
        let mut b = GraphBuilder::new(48_000);
        b.add_node(NodeSpec::audio(
            10,
            Source::Samples {
                frames: vec![
                    StereoFrame {
                        left: 0.5,
                        right: -0.5
                    };
                    2
                ]
                .into(),
                start_frame: 4,
            },
        ))
        .add_node(NodeSpec::master(100))
        .route(10, 100, 1.0);
        let mut g = b.build().unwrap();
        assert_eq!(g.render_frame(3), StereoFrame::ZERO);
        assert_eq!(
            g.render_frame(4),
            StereoFrame {
                left: 0.5,
                right: -0.5
            }
        );
        assert_eq!(
            g.render_frame(5),
            StereoFrame {
                left: 0.5,
                right: -0.5
            }
        );
        assert_eq!(g.render_frame(6), StereoFrame::ZERO);
    }

    #[test]
    fn looped_samples_repeat_at_boundary_without_padding_allocations() {
        let frames: std::sync::Arc<[StereoFrame]> = vec![
            StereoFrame {
                left: 0.25,
                right: -0.25,
            },
            StereoFrame {
                left: 0.5,
                right: -0.5,
            },
        ]
        .into();
        let mut source = Source::LoopedSamples {
            frames,
            start_frame: 4,
            loop_frames: 3,
            source_offset_frames: 0,
            gain: 1.0,
            fade_in_frames: 0,
            fade_out_frames: 0,
        };
        assert_eq!(source.sample(3, 48_000), StereoFrame::ZERO);
        assert_eq!(source.sample(4, 48_000).left, 0.25);
        assert_eq!(source.sample(5, 48_000).left, 0.5);
        assert_eq!(source.sample(6, 48_000), StereoFrame::ZERO);
        assert_eq!(source.sample(7, 48_000).left, 0.25);
    }

    #[test]
    fn split_audio_source_offset_preserves_both_halves() {
        let samples: std::sync::Arc<[StereoFrame]> = (0..20)
            .map(|i| StereoFrame {
                left: i as f32 / 20.0,
                right: 0.0,
            })
            .collect::<Vec<_>>()
            .into();
        let mut left = Source::TrimmedSamples {
            frames: std::sync::Arc::clone(&samples),
            start_frame: 10,
            end_frame: 15,
            source_offset_frames: 0,
            gain: 1.0,
            fade_in_frames: 0,
            fade_out_frames: 0,
        };
        let mut right = Source::TrimmedSamples {
            frames: std::sync::Arc::clone(&samples),
            start_frame: 15,
            end_frame: 20,
            source_offset_frames: 5,
            gain: 1.0,
            fade_in_frames: 0,
            fade_out_frames: 0,
        };
        assert_eq!(left.sample(14, 48_000).left, 0.2);
        assert_eq!(left.sample(15, 48_000), StereoFrame::ZERO);
        assert_eq!(right.sample(15, 48_000).left, 0.25);
        assert_eq!(right.sample(19, 48_000).left, 0.45);
        let mut looped = Source::LoopedSamples {
            frames: samples,
            start_frame: 15,
            loop_frames: 5,
            source_offset_frames: 5,
            gain: 1.0,
            fade_in_frames: 0,
            fade_out_frames: 0,
        };
        assert_eq!(looped.sample(15, 48_000).left, 0.25);
        assert_eq!(looped.sample(20, 48_000).left, 0.25);
    }

    #[test]
    fn clip_gain_and_fades_are_sample_accurate() {
        let samples: std::sync::Arc<[StereoFrame]> = vec![
            StereoFrame {
                left: 1.0,
                right: -1.0
            };
            30
        ]
        .into();
        let mut src = Source::TrimmedSamples {
            frames: std::sync::Arc::clone(&samples),
            start_frame: 10,
            end_frame: 20,
            source_offset_frames: 0,
            gain: 0.5,
            fade_in_frames: 4,
            fade_out_frames: 4,
        };
        assert_eq!(src.sample(9, 48_000), StereoFrame::ZERO);
        assert_eq!(src.sample(10, 48_000), StereoFrame::ZERO);
        assert_eq!(src.sample(12, 48_000).left, 0.25);
        assert_eq!(src.sample(14, 48_000).left, 0.5);
        assert_eq!(src.sample(18, 48_000).left, 0.25);
        assert_eq!(src.sample(20, 48_000), StereoFrame::ZERO);
        let mut looped = Source::LoopedSamples {
            frames: samples,
            start_frame: 0,
            loop_frames: 10,
            source_offset_frames: 0,
            gain: 0.5,
            fade_in_frames: 4,
            fade_out_frames: 4,
        };
        assert_eq!(looped.sample(0, 48_000), StereoFrame::ZERO);
        assert_eq!(looped.sample(4, 48_000).left, 0.5);
        assert_eq!(looped.sample(10, 48_000), StereoFrame::ZERO);
    }

    #[test]
    fn plugin_latency_compensates_parallel_track_paths() {
        let mut b = GraphBuilder::new(48_000);
        let mut slow = NodeSpec::audio(1, Source::Silence);
        slow.latency_samples = 64;
        b.add_node(slow)
            .add_node(NodeSpec::audio(2, Source::Silence))
            .add_node(NodeSpec::master(99));
        b.route(1, 99, 1.0).route(2, 99, 1.0);
        let g = b.build().unwrap();
        assert_eq!(g.delay_for_route(1, 99), Some(0));
        assert_eq!(g.delay_for_route(2, 99), Some(64));
    }

    #[test]
    fn return_paths_are_compensated_against_direct_paths() {
        let mut b = GraphBuilder::new(48_000);
        let mut ret = NodeSpec::returns(2);
        ret.latency_samples = 32;
        b.add_node(NodeSpec::audio(1, Source::Silence))
            .add_node(ret)
            .add_node(NodeSpec::master(3));
        b.route(1, 2, 0.4).route(1, 3, 1.0).route(2, 3, 1.0);
        let g = b.build().unwrap();
        assert_eq!(g.delay_for_route(1, 3), Some(32));
        assert_eq!(g.delay_for_route(2, 3), Some(0));
        assert_eq!(g.delay_for_route(1, 2), Some(0));
    }

    #[test]
    fn rejects_cycles_invalid_topology_and_nan() {
        let mut b = GraphBuilder::new(48_000);
        b.add_node(NodeSpec::returns(1))
            .add_node(NodeSpec::returns(2))
            .add_node(NodeSpec::master(3));
        b.route(1, 2, 1.0).route(2, 1, 1.0);
        assert!(matches!(b.build(), Err(GraphError::RoutingCycle)));
        let mut b = GraphBuilder::new(48_000);
        b.add_node(NodeSpec::audio(1, Source::Silence))
            .add_node(NodeSpec::master(2));
        b.route(1, 2, f32::NAN);
        assert!(matches!(b.build(), Err(GraphError::NonFiniteParameter)));
    }

    #[test]
    fn fixed_voice_midi_note_on_and_off() {
        let mut b = GraphBuilder::new(48_000);
        b.add_node(NodeSpec::midi(1))
            .add_node(NodeSpec::master(2))
            .route(1, 2, 1.0);
        let mut g = b.build().unwrap();
        g.note_on(1, 69, 127);
        assert_eq!(g.render_frame(0), StereoFrame::ZERO);
        assert!(g.render_frame(1).left > 0.0);
        g.note_off(1, 69);
        assert_eq!(g.render_frame(2), StereoFrame::ZERO);
    }
}

#[cfg(test)]
mod metronome_tests {
    use super::*;

    #[test]
    fn metronome_respects_eighth_note_denominator_and_six_beat_bar() {
        let mut click = Source::Metronome {
            bpm: 120.0,
            beats_per_bar: 6,
            beat_denominator: 8,
        };
        let peak = |source: &mut Source, at: u64| {
            (at + 1..at + 200)
                .map(|frame| source.sample(frame, 48_000).left.abs())
                .fold(0.0_f32, f32::max)
        };
        let first = peak(&mut click, 0);
        let second = peak(&mut click, 12_000);
        let next_bar = peak(&mut click, 72_000);
        assert!(first > second * 1.5, "bar downbeat has stronger accent");
        assert!(
            (first - next_bar).abs() < 1e-5,
            "accent repeats after six eighths"
        );
        assert_eq!(click.sample(6_000, 48_000), StereoFrame::ZERO);
        assert!(second > 0.005, "second eighth clicks at frame 12k");
    }

    #[test]
    fn click_is_accented_on_bar_one_and_silent_between_beats() {
        let mut b = GraphBuilder::new(48_000);
        b.add_node(NodeSpec::audio(
            10,
            Source::Metronome {
                bpm: 120.0,
                beats_per_bar: 4,
                beat_denominator: 4,
            },
        ));
        b.add_node(NodeSpec::master(20));
        b.route(10, 20, 1.0);
        let mut graph = b.build().unwrap();
        let first = (1..150)
            .map(|frame| graph.render_frame(frame).left.abs())
            .fold(0f32, f32::max);
        let between = (6000..7000)
            .map(|frame| graph.render_frame(frame).left.abs())
            .fold(0f32, f32::max);
        let second = (24_001..24_150)
            .map(|frame| graph.render_frame(frame).left.abs())
            .fold(0f32, f32::max);
        assert!(first > second * 1.5, "first beat should be accented");
        assert_eq!(between, 0.0);
        assert!(second > 0.01);
        assert_eq!(
            graph.render_midi_frame(48_003),
            StereoFrame::ZERO,
            "click must not sound while the transport is stopped"
        );
    }
}

#[cfg(test)]
mod live_input_graph_regressions {
    use super::*;
    use crate::monitor::RecordingMonitor;
    use std::sync::Arc;

    #[test]
    fn capture_samples_route_to_master_and_do_not_replay_after_empty() {
        let monitor = Arc::new(RecordingMonitor::new());
        monitor.write(StereoFrame {
            left: 0.25,
            right: -0.125,
        });
        monitor.write(StereoFrame {
            left: 0.35,
            right: -0.225,
        });
        let mut builder = GraphBuilder::new(44_100);
        builder
            .add_node(NodeSpec::audio(5, Source::LiveInput { monitor, cursor: 0 }))
            .add_node(NodeSpec::master(10));
        builder.route(5, 10, 1.0);
        let mut graph = builder.build().unwrap();
        assert_eq!(
            graph.render_frame(0),
            StereoFrame {
                left: 0.25,
                right: -0.125
            }
        );
        assert_eq!(
            graph.render_frame(1),
            StereoFrame {
                left: 0.35,
                right: -0.225
            }
        );
        assert_eq!(graph.render_frame(2), StereoFrame::ZERO);
    }
}

#[cfg(test)]
mod live_midi_graph_routing_tests {
    use super::*;
    use crate::live_plugin::{LIVE_BLOCK, live_plugin_pair};

    #[test]
    fn midi_graph_forwards_note_events_to_isolated_instrument_exactly() {
        let (instrument, mut worker) = live_plugin_pair();
        let mut node = NodeSpec::midi(7);
        node.source = Source::Silence;
        node.effects.add_live(instrument);
        node.latency_samples = crate::live_plugin::LIVE_DELAY;
        let mut builder = GraphBuilder::new(44_100);
        builder.add_node(node).add_node(NodeSpec::master(99));
        builder.route(7, 99, 1.0);
        let mut graph = builder.build().unwrap();

        graph.note_on(7, 60, 100);
        for frame in 0..LIVE_BLOCK as u64 {
            graph.render_frame(frame);
        }
        let first = worker.recv().unwrap();
        assert_eq!(first.midi_count, 1);
        assert_eq!(first.midi[0].kind, 1);
        assert_eq!(first.midi[0].note, 60);
        assert_eq!(first.midi[0].offset, 0);

        graph.note_off(7, 60);
        for frame in LIVE_BLOCK as u64..(LIVE_BLOCK * 2) as u64 {
            graph.render_frame(frame);
        }
        let second = worker.recv().unwrap();
        assert_eq!(second.midi_count, 1);
        assert_eq!(second.midi[0].kind, 0);
        assert_eq!(second.midi[0].note, 60);
        assert_eq!(second.midi[0].offset, 0);

        graph.all_notes_off(7);
        for frame in (LIVE_BLOCK * 2) as u64..(LIVE_BLOCK * 3) as u64 {
            graph.render_frame(frame);
        }
        let third = worker.recv().unwrap();
        assert_eq!(third.midi_count, 1);
        assert_eq!(third.midi[0].kind, 2);
    }
}

#[cfg(test)]
mod soundfont_graph_routing_tests {
    use super::*;

    #[derive(Debug, Default)]
    struct RecordingInstrument {
        pub level: f32,
    }

    impl Instrument for RecordingInstrument {
        fn render(&mut self, left: &mut [f32], right: &mut [f32]) {
            for (l, r) in left.iter_mut().zip(right.iter_mut()) {
                *l = self.level;
                *r = self.level;
            }
        }
        fn note_on(&mut self, _note: u8, _velocity: u8) {
            self.level = 0.4;
        }
        fn note_off(&mut self, _note: u8) {
            self.level = 0.0;
        }
        fn pitch_bend(&mut self, _value: u16) {}
        fn control_change(&mut self, _controller: u8, _value: u8) {}
        fn program_change(&mut self, _program: u8) {}
        fn all_notes_off(&mut self) {
            self.level = 0.0;
        }
        fn reset(&mut self) {
            self.level = 0.0;
        }
    }

    #[test]
    fn sound_font_node_receives_midi_and_routes_audio_to_master() {
        let mut builder = GraphBuilder::new(48_000);
        let mut node = NodeSpec::midi(7);
        node.source = Source::SoundFont {
            instrument: Box::new(RecordingInstrument::default()),
        };
        builder.add_node(node).add_node(NodeSpec::master(99));
        builder.route(7, 99, 1.0);
        let mut graph = builder.build().unwrap();

        // note_on sets the mock instrument's output level to 0.4, which must
        // travel through the DAG to the master output.
        graph.note_on(7, 60, 100);
        assert_eq!(graph.render_frame(0).left, 0.4);
        graph.note_off(7, 60);
        assert_eq!(graph.render_frame(1), StereoFrame::ZERO);
        graph.note_on(7, 64, 127);
        graph.all_notes_off(7);
        assert_eq!(graph.render_frame(2), StereoFrame::ZERO);
    }
}
