//! Precompiled DAG for track/return/master routing. Construct or edit ONLY off the audio thread.
//! Every buffer, edge delay, and node is allocated before the processor starts.

use std::f32::consts::TAU;

const MAX_NODES: usize = 64;
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
    /// Fixed 16-voice sine synth for MIDI engine verification (not a plugin replacement).
    MidiSine {
        voices: Box<[MidiVoice; MAX_VOICES]>,
        next_voice: usize,
    },
}

#[derive(Debug, Clone, Copy)]
pub struct MidiVoice(Voice);

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
        }
    }

    fn note_on(&mut self, note: u8, velocity: u8, sample_rate: u32) {
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
    }

    fn note_off(&mut self, note: u8) {
        if let Self::MidiSine { voices, .. } = self {
            for voice in voices.iter_mut() {
                if voice.0.note == note {
                    voice.0.active = false;
                }
            }
        }
    }

    fn all_notes_off(&mut self) {
        if let Self::MidiSine { voices, .. } = self {
            for voice in voices.iter_mut() {
                voice.0.active = false;
            }
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
        if let Some(node) = self
            .nodes
            .iter_mut()
            .find(|n| n.id == id && n.kind == NodeKind::MidiTrack)
        {
            node.source.note_on(note, velocity, self.sample_rate);
        }
    }

    pub fn note_off(&mut self, id: u32, note: u8) {
        if let Some(node) = self
            .nodes
            .iter_mut()
            .find(|n| n.id == id && n.kind == NodeKind::MidiTrack)
        {
            node.source.note_off(note);
        }
    }

    pub fn reset(&mut self) {
        for node in &mut self.nodes {
            node.source.all_notes_off();
        }
        for edge in &mut self.edges {
            edge.delay.clear();
        }
    }

    /// One frame of signal flow, no allocation, file IO, locks, or sleeping.
    /// The graph and all routes must remain structurally unchanged while rendering.
    pub fn render_frame(&mut self, project_frame: u64) -> StereoFrame {
        self.scratch.fill(StereoFrame::ZERO);
        for &index in &self.topological_order {
            let node = &mut self.nodes[index];
            let source = node.source.sample(project_frame, self.sample_rate);
            let mut result = self.scratch[index];
            result.add(source);
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
