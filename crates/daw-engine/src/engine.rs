//! Fixed-capacity control queue and sample-accurate real-time render loop.
//!
//! Absolute frame timestamps use the monotonic engine clock (not project time).
//! The audio callback is the sole owner of AudioGraph; no mutex, heap allocation,
//! filesystem access, or blocking calls are used in process_interleaved.

use std::sync::{
    Arc,
    atomic::{AtomicU32, AtomicU64, Ordering},
};

use rtrb::{Consumer, Producer, RingBuffer};

use crate::graph::{AudioGraph, StereoFrame};

pub const COMMAND_RING_CAPACITY: usize = 1024;
pub const MAX_PENDING_COMMANDS: usize = 1024;
const MAX_GAIN_RAMPS: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EngineCommand {
    Play,
    Stop,
    Seek {
        project_frame: u64,
    },
    SetGain {
        node: u32,
        gain: f32,
    },
    /// Linear ramp over exactly duration_frames engine samples.
    RampGain {
        node: u32,
        target_gain: f32,
        duration_frames: u32,
    },
    SetPan {
        node: u32,
        pan: f32,
    },
    SetRouteGain {
        from: u32,
        to: u32,
        gain: f32,
    },
    NoteOn {
        node: u32,
        note: u8,
        velocity: u8,
    },
    NoteOff {
        node: u32,
        note: u8,
    },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScheduledCommand {
    /// Timeline of *hardware engine* frames. Independent of play/stop/seek.
    pub at_engine_frame: u64,
    pub command: EngineCommand,
}

#[derive(Default)]
pub struct EngineTelemetry {
    /// Snapshot, updated once per audio callback rather than every sample.
    pub rendered_frames: AtomicU64,
    pub project_frame: AtomicU64,
    pub callbacks: AtomicU64,
    pub queue_overflows: AtomicU64,
    pub pending_overflows: AtomicU64,
    pub gain_ramp_overflows: AtomicU64,
    /// Maximum absolute rendered sample amplitude observed since engine start.
    pub output_peak_bits: AtomicU32,
    /// Per callback-block peak, for responsive UI metering. Separate from max peak.
    pub current_peak_bits: AtomicU32,
    /// Device-backend prediction of callback-to-output delay. Not round-trip latency.
    pub predicted_playback_delay_us: AtomicU64,
}

pub struct Controller {
    queue: Producer<ScheduledCommand>,
    telemetry: Arc<EngineTelemetry>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QueueError {
    Full,
}

impl Controller {
    /// Schedule at a precise *absolute* engine frame.
    pub fn schedule(&mut self, command: ScheduledCommand) -> Result<(), QueueError> {
        self.queue.push(command).map_err(|_| {
            self.telemetry
                .queue_overflows
                .fetch_add(1, Ordering::Relaxed);
            QueueError::Full
        })
    }

    /// For UI actions. Applies on or after the next hardware block boundary.
    pub fn immediate(&mut self, command: EngineCommand) -> Result<(), QueueError> {
        self.schedule(ScheduledCommand {
            at_engine_frame: 0,
            command,
        })
    }

    pub fn telemetry(&self) -> &Arc<EngineTelemetry> {
        &self.telemetry
    }

    pub fn engine_frame_snapshot(&self) -> u64 {
        self.telemetry.rendered_frames.load(Ordering::Relaxed)
    }
}

#[derive(Debug, Clone, Copy)]
struct GainRamp {
    node: u32,
    next_gain: f32,
    step: f32,
    target: f32,
    remaining: u32,
}

pub struct RenderEngine {
    graph: AudioGraph,
    commands: Consumer<ScheduledCommand>,
    telemetry: Arc<EngineTelemetry>,
    pending: [Option<ScheduledCommand>; MAX_PENDING_COMMANDS],
    pending_len: usize,
    gain_ramps: [Option<GainRamp>; MAX_GAIN_RAMPS],
    engine_frame: u64,
    project_frame: u64,
    playing: bool,
}

pub fn create_engine(graph: AudioGraph) -> (Controller, RenderEngine) {
    let (queue, commands) = RingBuffer::new(COMMAND_RING_CAPACITY);
    let telemetry = Arc::new(EngineTelemetry::default());
    let controller = Controller {
        queue,
        telemetry: Arc::clone(&telemetry),
    };
    let engine = RenderEngine {
        graph,
        commands,
        telemetry,
        pending: [None; MAX_PENDING_COMMANDS],
        pending_len: 0,
        gain_ramps: [None; MAX_GAIN_RAMPS],
        engine_frame: 0,
        project_frame: 0,
        playing: false,
    };
    (controller, engine)
}

impl RenderEngine {
    pub fn graph(&self) -> &AudioGraph {
        &self.graph
    }

    pub fn engine_frame(&self) -> u64 {
        self.engine_frame
    }

    pub fn project_frame(&self) -> u64 {
        self.project_frame
    }

    pub fn playing(&self) -> bool {
        self.playing
    }

    fn drain_inbox(&mut self) {
        // SPSC queue is bounded. Every command is Copy, so popped payloads cannot free memory.
        while let Ok(event) = self.commands.pop() {
            if self.pending_len == MAX_PENDING_COMMANDS {
                self.telemetry
                    .pending_overflows
                    .fetch_add(1, Ordering::Relaxed);
                continue;
            }
            // Stable insertion order for equal timestamps.
            let mut index = self.pending_len;
            while index > 0
                && self.pending[index - 1]
                    .is_some_and(|entry| entry.at_engine_frame > event.at_engine_frame)
            {
                self.pending[index] = self.pending[index - 1];
                index -= 1;
            }
            self.pending[index] = Some(event);
            self.pending_len += 1;
        }
    }

    fn dispatch_due(&mut self) {
        while self.pending_len > 0 {
            let Some(front) = self.pending[0] else { break };
            if front.at_engine_frame > self.engine_frame {
                break;
            }
            // Shift remaining Copy event handles, never reallocates.
            for i in 1..self.pending_len {
                self.pending[i - 1] = self.pending[i];
            }
            self.pending_len -= 1;
            self.pending[self.pending_len] = None;
            match front.command {
                EngineCommand::Play => self.playing = true,
                EngineCommand::Stop => {
                    self.playing = false;
                    self.graph.reset();
                }
                EngineCommand::Seek { project_frame } => {
                    self.project_frame = project_frame;
                    self.graph.reset();
                }
                EngineCommand::SetGain { node, gain } => {
                    self.cancel_gain_ramp(node);
                    self.graph.set_gain(node, gain);
                }
                EngineCommand::RampGain {
                    node,
                    target_gain,
                    duration_frames,
                } => {
                    self.begin_gain_ramp(node, target_gain, duration_frames);
                }
                EngineCommand::SetPan { node, pan } => self.graph.set_pan(node, pan),
                EngineCommand::SetRouteGain { from, to, gain } => {
                    self.graph.set_route_gain(from, to, gain);
                }
                EngineCommand::NoteOn {
                    node,
                    note,
                    velocity,
                } => {
                    self.graph.note_on(node, note, velocity);
                }
                EngineCommand::NoteOff { node, note } => self.graph.note_off(node, note),
            }
        }
    }

    fn cancel_gain_ramp(&mut self, node: u32) {
        for slot in &mut self.gain_ramps {
            if slot.is_some_and(|ramp| ramp.node == node) {
                *slot = None;
            }
        }
    }

    fn begin_gain_ramp(&mut self, node: u32, target_gain: f32, duration_frames: u32) {
        self.cancel_gain_ramp(node);
        let Some(current) = self.graph.gain_of(node) else {
            return;
        };
        if !target_gain.is_finite() {
            return;
        }
        let target = target_gain.clamp(0.0, 4.0);
        if duration_frames == 0 {
            self.graph.set_gain(node, target);
            return;
        }
        let ramp = GainRamp {
            node,
            next_gain: current,
            step: (target - current) / duration_frames as f32,
            target,
            remaining: duration_frames,
        };
        if let Some(slot) = self.gain_ramps.iter_mut().find(|slot| slot.is_none()) {
            *slot = Some(ramp);
        } else {
            self.telemetry
                .gain_ramp_overflows
                .fetch_add(1, Ordering::Relaxed);
            self.graph.set_gain(node, target);
        }
    }

    fn advance_gain_ramps(&mut self) {
        for slot in &mut self.gain_ramps {
            if let Some(ramp) = slot.as_mut() {
                if ramp.remaining <= 1 {
                    self.graph.set_gain(ramp.node, ramp.target);
                    *slot = None;
                } else {
                    ramp.next_gain += ramp.step;
                    self.graph.set_gain(ramp.node, ramp.next_gain);
                    ramp.remaining -= 1;
                }
            }
        }
    }

    fn next_stereo_frame(&mut self) -> StereoFrame {
        self.dispatch_due();
        self.advance_gain_ramps();
        let sample = if self.playing {
            let result = self.graph.render_frame(self.project_frame);
            self.project_frame = self.project_frame.saturating_add(1);
            result
        } else {
            StereoFrame::ZERO
        };
        self.engine_frame = self.engine_frame.saturating_add(1);
        sample
    }

    /// Render interleaved f32. Supports mono, stereo, and multi-channel streams;
    /// mono folds down L/R; extra channels are silent.
    pub fn process_interleaved(&mut self, data: &mut [f32], channels: usize) {
        if channels == 0 {
            data.fill(0.0);
            return;
        }
        self.drain_inbox();
        let mut peak = 0.0_f32;
        let mut chunks = data.chunks_exact_mut(channels);
        for frame in &mut chunks {
            let sample = self.next_stereo_frame();
            if channels == 1 {
                frame[0] = sanitize((sample.left + sample.right) * 0.5);
            } else {
                frame[0] = sanitize(sample.left);
                frame[1] = sanitize(sample.right);
                frame[2..].fill(0.0);
            }
            for &sample in &frame[..channels.min(2)] {
                peak = peak.max(sample.abs());
            }
        }
        chunks.into_remainder().fill(0.0);
        self.telemetry
            .output_peak_bits
            .fetch_max(peak.to_bits(), Ordering::Relaxed);
        self.telemetry
            .current_peak_bits
            .store(peak.to_bits(), Ordering::Relaxed);
        self.telemetry
            .rendered_frames
            .store(self.engine_frame, Ordering::Relaxed);
        self.telemetry
            .project_frame
            .store(self.project_frame, Ordering::Relaxed);
        self.telemetry.callbacks.fetch_add(1, Ordering::Relaxed);
    }
}

#[inline]
fn sanitize(x: f32) -> f32 {
    if x.is_finite() {
        x.clamp(-1.0, 1.0)
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::{GraphBuilder, NodeSpec, Source};

    fn fixture() -> (Controller, RenderEngine) {
        let mut builder = GraphBuilder::new(48_000);
        builder
            .add_node(NodeSpec::audio(
                1,
                Source::Samples {
                    frames: vec![
                        StereoFrame {
                            left: 0.5,
                            right: 0.25
                        };
                        16
                    ]
                    .into(),
                    start_frame: 0,
                },
            ))
            .add_node(NodeSpec::master(10))
            .route(1, 10, 1.0);
        create_engine(builder.build().unwrap())
    }

    #[test]
    fn transport_pause_seek_and_exact_rendered_audio() {
        let (mut ui, mut engine) = fixture();
        let mut out = [9.0; 8];
        engine.process_interleaved(&mut out, 2);
        assert_eq!(out, [0.0; 8]);
        ui.immediate(EngineCommand::Play).unwrap();
        engine.process_interleaved(&mut out, 2);
        assert_eq!(out, [0.5, 0.25, 0.5, 0.25, 0.5, 0.25, 0.5, 0.25]);
        assert_eq!(engine.project_frame(), 4);
        ui.immediate(EngineCommand::Stop).unwrap();
        engine.process_interleaved(&mut out, 2);
        assert_eq!(out, [0.0; 8]);
        assert_eq!(engine.project_frame(), 4);
        ui.immediate(EngineCommand::Seek { project_frame: 1 })
            .unwrap();
        ui.immediate(EngineCommand::Play).unwrap();
        engine.process_interleaved(&mut out, 2);
        assert_eq!(engine.project_frame(), 5);
        assert_eq!(out, [0.5, 0.25, 0.5, 0.25, 0.5, 0.25, 0.5, 0.25]);
    }

    #[test]
    fn sample_accurate_automation_mid_buffer_and_future_schedule() {
        let (mut ui, mut engine) = fixture();
        ui.immediate(EngineCommand::Play).unwrap();
        ui.schedule(ScheduledCommand {
            at_engine_frame: 3,
            command: EngineCommand::SetGain { node: 1, gain: 0.0 },
        })
        .unwrap();
        let mut out = [0.0; 10];
        engine.process_interleaved(&mut out, 2);
        assert_eq!(out, [0.5, 0.25, 0.5, 0.25, 0.5, 0.25, 0.0, 0.0, 0.0, 0.0]);
        assert_eq!(engine.engine_frame(), 5);
    }

    #[test]
    fn buffer_boundaries_do_not_lose_scheduled_events() {
        let (mut ui, mut engine) = fixture();
        ui.immediate(EngineCommand::Play).unwrap();
        ui.schedule(ScheduledCommand {
            at_engine_frame: 5,
            command: EngineCommand::SetGain { node: 1, gain: 0.0 },
        })
        .unwrap();
        let mut out = [0.0; 8];
        engine.process_interleaved(&mut out, 2);
        assert_eq!(out[6], 0.5);
        engine.process_interleaved(&mut out, 2);
        assert_eq!(out[0], 0.5);
        assert_eq!(out[2], 0.0);
    }

    #[test]
    fn stereo_and_mono_channel_layouts_and_telemetry() {
        let (mut ui, mut engine) = fixture();
        ui.immediate(EngineCommand::Play).unwrap();
        let mut out = [0.0; 4];
        engine.process_interleaved(&mut out, 1);
        assert_eq!(out, [0.375; 4]);
        assert_eq!(ui.telemetry().rendered_frames.load(Ordering::Relaxed), 4);
        assert_eq!(ui.telemetry().callbacks.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn bounded_ring_reports_overflow_to_ui() {
        let (mut ui, _engine) = fixture();
        for _ in 0..COMMAND_RING_CAPACITY {
            ui.immediate(EngineCommand::Play).unwrap();
        }
        assert_eq!(ui.immediate(EngineCommand::Stop), Err(QueueError::Full));
        assert_eq!(ui.telemetry().queue_overflows.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn gain_curve_is_sample_accurate_in_single_buffer() {
        let (mut ui, mut engine) = fixture();
        ui.immediate(EngineCommand::Play).unwrap();
        ui.schedule(ScheduledCommand {
            at_engine_frame: 2,
            command: EngineCommand::RampGain {
                node: 1,
                target_gain: 0.0,
                duration_frames: 4,
            },
        })
        .unwrap();
        let mut out = [0.0_f32; 14];
        engine.process_interleaved(&mut out, 2);
        let left = out.chunks_exact(2).map(|f| f[0]).collect::<Vec<_>>();
        assert_eq!(left, vec![0.5, 0.5, 0.375, 0.25, 0.125, 0.0, 0.0]);
    }

    #[test]
    fn zero_duration_ramp_updates_immediately() {
        let (mut ui, mut engine) = fixture();
        ui.immediate(EngineCommand::Play).unwrap();
        ui.immediate(EngineCommand::RampGain {
            node: 1,
            target_gain: 0.0,
            duration_frames: 0,
        })
        .unwrap();
        let mut out = [1.0_f32; 4];
        engine.process_interleaved(&mut out, 2);
        assert_eq!(out, [0.0; 4]);
    }

    #[test]
    fn invalid_floats_are_sanitized_at_output() {
        assert_eq!(sanitize(f32::NAN), 0.0);
        assert_eq!(sanitize(f32::INFINITY), 0.0);
        assert_eq!(sanitize(-2.0), -1.0);
    }
}
