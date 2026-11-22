//! Fixed-latency, bounded lock-free SPSC bridge for isolated CLAP effects.
//! A worker thread owns child-process stdin/stdout; the callback owns ONLY
//! the ring endpoints and does not block, allocate, lock, or invoke plugin code.
use crate::graph::StereoFrame;
use rtrb::{Consumer, Producer, RingBuffer};
use std::sync::{
    Arc, Condvar, Mutex,
    atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
};
use std::time::Duration;

pub const LIVE_BLOCK: usize = 128;
/// Maximum number of timestamped note messages per 128-frame callback block.
pub const LIVE_MIDI_CAPACITY: usize = 64;
pub const LIVE_MIDI_EVENT_BYTES: usize = 4;
pub const LIVE_MIDI_PACKET_BYTES: usize = 1 + LIVE_MIDI_CAPACITY * LIVE_MIDI_EVENT_BYTES;
/// Upper bound for one opaque native plugin state blob. A worker that reports
/// more than this is treated as failed; the project is never inflated.
pub use daw_core::plugin_state::MAX_PLUGIN_STATE_BYTES;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LiveMidiEvent {
    pub offset: u8,
    pub note: u8,
    pub velocity: u8,
    pub channel: u8,
    /// 0=note off, 1=note on, 2=global panic, 3=sustain, 4=pitch bend,
    /// 5=channel panic. The two data fields carry MIDI 7-bit values.
    pub kind: u8,
}
impl LiveMidiEvent {
    pub const fn to_wire(self) -> [u8; LIVE_MIDI_EVENT_BYTES] {
        [
            self.offset,
            self.note,
            self.velocity,
            self.channel << 4 | self.kind,
        ]
    }

    pub fn decode_packet(
        packet: &[u8; LIVE_MIDI_PACKET_BYTES],
    ) -> Result<impl Iterator<Item = Result<Self, &'static str>> + '_, &'static str> {
        let count = usize::from(packet[0]);
        if count > LIVE_MIDI_CAPACITY {
            return Err("Invalid live MIDI event count");
        }
        Ok(packet[1..]
            .as_chunks::<LIVE_MIDI_EVENT_BYTES>()
            .0
            .iter()
            .take(count)
            .map(|event| {
                let [offset, note, velocity, status] = [event[0], event[1], event[2], event[3]];
                let kind = status & 15;
                if usize::from(offset) >= LIVE_BLOCK || note > 127 || velocity > 127 || kind > 5 {
                    return Err("Invalid live MIDI event");
                }
                Ok(Self {
                    offset,
                    note,
                    velocity,
                    channel: status >> 4,
                    kind,
                })
            }))
    }

    pub fn message(self) -> LiveMidiMessage {
        match self.kind {
            0 => LiveMidiMessage::NoteOff(self.note),
            1 => LiveMidiMessage::NoteOn(self.note, self.velocity),
            2 => LiveMidiMessage::Panic,
            3 => LiveMidiMessage::Sustain(self.velocity),
            4 => LiveMidiMessage::PitchBend(u16::from(self.note) | u16::from(self.velocity) << 7),
            5 => LiveMidiMessage::ChannelPanic,
            _ => unreachable!("invalid live MIDI message kind"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LiveMidiMessage {
    NoteOn(u8, u8),
    NoteOff(u8),
    Sustain(u8),
    PitchBend(u16),
    ChannelPanic,
    Panic,
}

pub const LIVE_DELAY: usize = LIVE_BLOCK * 4;
const LIVE_QUEUE_BLOCKS: usize = 64;

#[derive(Clone, Copy, Debug)]
pub struct LiveAudioBlock {
    pub sequence: u64,
    pub frames: [StereoFrame; LIVE_BLOCK],
    pub midi_count: u8,
    pub midi: [LiveMidiEvent; LIVE_MIDI_CAPACITY],
}

impl LiveAudioBlock {
    pub fn new(sequence: u64) -> Self {
        Self {
            sequence,
            frames: [StereoFrame::ZERO; LIVE_BLOCK],
            midi_count: 0,
            midi: [LiveMidiEvent::default(); LIVE_MIDI_CAPACITY],
        }
    }
}

/// Control side of an isolated worker. It is used only by UI/worker threads —
/// never by the audio callback — so a mutex and condition variable are safe.
type StateExchange = Arc<(Mutex<Option<Result<Vec<u8>, String>>>, Condvar)>;

#[derive(Clone)]
pub struct LivePluginControl {
    state_request: Arc<AtomicBool>,
    state_slot: StateExchange,
    latency_samples: Arc<AtomicU32>,
    tail_samples: Arc<AtomicU32>,
}

impl std::fmt::Debug for LivePluginControl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LivePluginControl")
            .field("latency_samples", &self.latency_samples())
            .field("tail_samples", &self.tail_samples())
            .finish()
    }
}

impl LivePluginControl {
    /// Ask the worker for a native state snapshot and wait (bounded) for it.
    /// Intended for project save on the UI thread, never for the audio callback.
    pub fn capture_state(&self, timeout: Duration) -> Result<Vec<u8>, String> {
        {
            let (slot, _) = &*self.state_slot;
            *slot.lock().map_err(|_| "Plugin state channel poisoned")? = None;
        }
        self.state_request.store(true, Ordering::Release);
        let (slot, ready) = &*self.state_slot;
        let guard = slot.lock().map_err(|_| "Plugin state channel poisoned")?;
        let (guard, _) = ready
            .wait_timeout_while(guard, timeout, |answer| answer.is_none())
            .map_err(|_| "Plugin state channel poisoned")?;
        guard
            .clone()
            .unwrap_or_else(|| Err("Plugin worker did not return state before the deadline".into()))
    }

    pub fn take_state_request(&self) -> bool {
        self.state_request.swap(false, Ordering::AcqRel)
    }

    pub fn deliver_state(&self, answer: Result<Vec<u8>, String>) {
        if let Ok(mut slot) = self.state_slot.0.lock() {
            *slot = Some(answer);
            self.state_slot.1.notify_all();
        }
    }

    /// Plugin-reported processing latency in samples, if the worker announced it.
    pub fn latency_samples(&self) -> u32 {
        self.latency_samples.load(Ordering::Relaxed)
    }

    pub fn tail_samples(&self) -> u32 {
        self.tail_samples.load(Ordering::Relaxed)
    }

    pub fn set_reported_latency(&self, latency: u32, tail: u32) {
        self.latency_samples.store(latency, Ordering::Relaxed);
        self.tail_samples.store(tail, Ordering::Relaxed);
    }
}

pub struct LivePluginEffect {
    to_worker: Producer<LiveAudioBlock>,
    from_worker: Consumer<LiveAudioBlock>,
    pending_input: [StereoFrame; LIVE_BLOCK],
    pending_midi: [LiveMidiEvent; LIVE_MIDI_CAPACITY],
    pending_midi_count: u8,
    selected_output: Option<LiveAudioBlock>,
    next_output: Option<LiveAudioBlock>,
    dry: [StereoFrame; LIVE_DELAY],
    position: u64,
    sequence_base: u64,
    stop: Arc<AtomicBool>,
    missed_blocks: Arc<AtomicU64>,
    control: LivePluginControl,
}
impl std::fmt::Debug for LivePluginEffect {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LivePluginEffect")
            .field("position", &self.position)
            .field("missed_blocks", &self.missed_blocks.load(Ordering::Relaxed))
            .finish()
    }
}
impl Drop for LivePluginEffect {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
    }
}

impl LivePluginEffect {
    /// Called on the audio callback before the frame's render. Events are
    /// sample-aligned inside the next 128-frame plug-in block, without allocation.
    pub fn midi(&mut self, channel: u8, message: LiveMidiMessage) {
        if channel >= 16 {
            return;
        }
        let (kind, note, velocity) = match message {
            LiveMidiMessage::NoteOn(note, velocity) if note < 128 => {
                (u8::from(velocity != 0), note, velocity.min(127))
            }
            LiveMidiMessage::NoteOff(note) if note < 128 => (0, note, 0),
            LiveMidiMessage::Panic => (2, 0, 0),
            LiveMidiMessage::Sustain(value) => (3, 64, value.min(127)),
            LiveMidiMessage::PitchBend(value) if value < 16_384 => {
                (4, (value & 127) as u8, (value >> 7) as u8)
            }
            LiveMidiMessage::ChannelPanic => (5, 0, 0),
            _ => return,
        };
        let event = LiveMidiEvent {
            offset: (self.position % LIVE_BLOCK as u64) as u8,
            note,
            velocity,
            channel,
            kind,
        };
        let index = self.pending_midi_count as usize;
        if index == LIVE_MIDI_CAPACITY {
            // Drop later events and silence all channels after the retained prefix.
            self.pending_midi[LIVE_MIDI_CAPACITY - 1] = LiveMidiEvent { kind: 2, ..event };
            self.missed_blocks.fetch_add(1, Ordering::Relaxed);
            return;
        }
        self.pending_midi[index] = event;
        self.pending_midi_count += 1;
    }

    pub fn note_on(&mut self, note: u8, velocity: u8) {
        self.midi(0, LiveMidiMessage::NoteOn(note, velocity));
    }

    pub fn note_off(&mut self, note: u8) {
        self.midi(0, LiveMidiMessage::NoteOff(note));
    }

    pub fn all_notes_off(&mut self) {
        self.midi(0, LiveMidiMessage::Panic);
    }

    /// One sample, strictly constant-time apart from bounded ring reads.
    /// The unprocessed dry fallback is delayed by the same fixed 512 frames,
    /// never an unaligned immediate bypass. Late plugin blocks are discarded.
    pub fn process(&mut self, input: StereoFrame) -> StereoFrame {
        let input_index = self.position as usize % LIVE_BLOCK;
        self.pending_input[input_index] = input;
        if input_index + 1 == LIVE_BLOCK {
            let block = LiveAudioBlock {
                sequence: self.sequence_base + self.position / LIVE_BLOCK as u64,
                frames: self.pending_input,
                midi_count: self.pending_midi_count,
                midi: self.pending_midi,
            };
            self.pending_midi_count = 0;
            if let Err(rtrb::PushError::Full(block)) = self.to_worker.push(block) {
                // Carry a panic at frame zero of the next admitted block if
                // this block may have contained a lost note or pedal release.
                if block.midi_count != 0 {
                    self.pending_midi[0] = LiveMidiEvent {
                        kind: 2,
                        ..LiveMidiEvent::default()
                    };
                    self.pending_midi_count = 1;
                }
                self.missed_blocks.fetch_add(1, Ordering::Relaxed);
            }
        }
        let dry_index = self.position as usize % LIVE_DELAY;
        let fallback = self.dry[dry_index];
        self.dry[dry_index] = input;

        let result = if self.position >= LIVE_DELAY as u64 {
            let output_frame = self.position - LIVE_DELAY as u64;
            let index = output_frame as usize % LIVE_BLOCK;
            if index == 0 {
                let expected = self.sequence_base + output_frame / LIVE_BLOCK as u64;
                self.selected_output = None;
                // Max 64 blocks in queue; no unbounded work in callback.
                for _ in 0..LIVE_QUEUE_BLOCKS {
                    let next = self
                        .next_output
                        .take()
                        .or_else(|| self.from_worker.pop().ok());
                    let Some(block) = next else { break };
                    if block.sequence < expected {
                        continue;
                    }
                    if block.sequence == expected {
                        self.selected_output = Some(block);
                    } else {
                        self.next_output = Some(block);
                    }
                    break;
                }
                if self.selected_output.is_none() {
                    self.missed_blocks.fetch_add(1, Ordering::Relaxed);
                }
            }
            self.selected_output
                .as_ref()
                .map_or(fallback, |block| block.frames[index])
        } else {
            StereoFrame::ZERO
        };
        self.position = self.position.saturating_add(1);
        result
    }

    pub fn missed_counter(&self) -> Arc<AtomicU64> {
        Arc::clone(&self.missed_blocks)
    }

    /// UI/worker-side control handle for native state and reported latency.
    /// The audio callback never calls into this handle.
    pub fn control(&self) -> LivePluginControl {
        self.control.clone()
    }

    pub fn reset(&mut self) {
        // Increment the monotonic block epoch before resetting position so
        // buffered responses from the previous transport segment cannot be
        // mistaken for freshly processed plug-in audio after Seek.
        self.sequence_base = self
            .sequence_base
            .saturating_add(self.position / LIVE_BLOCK as u64 + LIVE_QUEUE_BLOCKS as u64 + 2);
        self.pending_input.fill(StereoFrame::ZERO);
        self.pending_midi_count = 0;
        self.position = 0;
        self.all_notes_off();
        self.dry.fill(StereoFrame::ZERO);
        self.selected_output = None;
        self.next_output = None;
        // Avoid playing old effect tails after transport seek. The worker's
        // queued blocks carry old sequence numbers and will be discarded.
    }
}

pub struct LivePluginWorker {
    input: Consumer<LiveAudioBlock>,
    output: Producer<LiveAudioBlock>,
    stop: Arc<AtomicBool>,
    missed_blocks: Arc<AtomicU64>,
    control: LivePluginControl,
}
impl LivePluginWorker {
    pub fn recv(&mut self) -> Option<LiveAudioBlock> {
        self.input.pop().ok()
    }
    pub fn send(&mut self, block: LiveAudioBlock) -> bool {
        self.output.push(block).is_ok()
    }
    pub fn stopped(&self) -> bool {
        self.stop.load(Ordering::Acquire)
    }
    pub fn missed_blocks(&self) -> u64 {
        self.missed_blocks.load(Ordering::Relaxed)
    }
    pub fn control(&self) -> LivePluginControl {
        self.control.clone()
    }
}

pub fn live_plugin_pair() -> (LivePluginEffect, LivePluginWorker) {
    let (input_tx, input_rx) = RingBuffer::new(LIVE_QUEUE_BLOCKS);
    let (output_tx, output_rx) = RingBuffer::new(LIVE_QUEUE_BLOCKS);
    let stop = Arc::new(AtomicBool::new(false));
    let missed_blocks = Arc::new(AtomicU64::new(0));
    let control = LivePluginControl {
        state_request: Arc::new(AtomicBool::new(false)),
        state_slot: Arc::new((Mutex::new(None), Condvar::new())),
        latency_samples: Arc::new(AtomicU32::new(0)),
        tail_samples: Arc::new(AtomicU32::new(0)),
    };
    let rt = LivePluginEffect {
        to_worker: input_tx,
        from_worker: output_rx,
        pending_input: [StereoFrame::ZERO; LIVE_BLOCK],
        pending_midi: [LiveMidiEvent::default(); LIVE_MIDI_CAPACITY],
        pending_midi_count: 0,
        selected_output: None,
        next_output: None,
        dry: [StereoFrame::ZERO; LIVE_DELAY],
        position: 0,
        sequence_base: 0,
        stop: Arc::clone(&stop),
        missed_blocks: Arc::clone(&missed_blocks),
        control: control.clone(),
    };
    let worker = LivePluginWorker {
        input: input_rx,
        output: output_tx,
        stop,
        missed_blocks,
        control,
    };
    (rt, worker)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounded_midi_packet_preserves_offset_channel_and_expression() {
        let (mut audio, mut worker) = live_plugin_pair();
        for _ in 0..11 {
            audio.process(StereoFrame::ZERO);
        }
        audio.midi(6, LiveMidiMessage::NoteOn(50, 100));
        audio.midi(6, LiveMidiMessage::Sustain(127));
        audio.midi(6, LiveMidiMessage::PitchBend(9000));
        for _ in 11..LIVE_BLOCK - 1 {
            audio.process(StereoFrame::ZERO);
        }
        audio.midi(6, LiveMidiMessage::NoteOff(50));
        audio.midi(6, LiveMidiMessage::Sustain(0));
        audio.process(StereoFrame::ZERO);
        let block = worker.recv().unwrap();
        let mut packet = [0; LIVE_MIDI_PACKET_BYTES];
        packet[0] = block.midi_count;
        for (slot, event) in packet[1..]
            .as_chunks_mut::<LIVE_MIDI_EVENT_BYTES>()
            .0
            .iter_mut()
            .zip(block.midi)
        {
            slot.copy_from_slice(&event.to_wire());
        }
        let events = LiveMidiEvent::decode_packet(&packet)
            .unwrap()
            .map(|event| {
                let event = event.unwrap();
                (event.offset, event.channel, event.message())
            })
            .collect::<Vec<_>>();
        assert_eq!(
            events,
            [
                (11, 6, LiveMidiMessage::NoteOn(50, 100)),
                (11, 6, LiveMidiMessage::Sustain(127)),
                (11, 6, LiveMidiMessage::PitchBend(9000)),
                (127, 6, LiveMidiMessage::NoteOff(50)),
                (127, 6, LiveMidiMessage::Sustain(0)),
            ]
        );
        packet[1] = 128;
        assert!(
            LiveMidiEvent::decode_packet(&packet)
                .unwrap()
                .next()
                .unwrap()
                .is_err()
        );
    }

    #[test]
    fn queue_overflow_carries_panic_until_a_block_is_admitted() {
        let (mut audio, mut worker) = live_plugin_pair();
        for _ in 0..LIVE_QUEUE_BLOCKS * LIVE_BLOCK {
            audio.process(StereoFrame::ZERO);
        }
        audio.midi(4, LiveMidiMessage::Sustain(0));
        for _ in 0..LIVE_BLOCK * 2 {
            audio.process(StereoFrame::ZERO);
        }
        while worker.recv().is_some() {}
        for _ in 0..LIVE_BLOCK {
            audio.process(StereoFrame::ZERO);
        }
        let block = worker.recv().unwrap();
        assert_eq!(block.midi_count, 1);
        assert_eq!(block.midi[0].offset, 0);
        assert_eq!(block.midi[0].message(), LiveMidiMessage::Panic);
    }

    #[test]
    fn live_processor_uses_isolated_worker_and_exact_delayed_fallback() {
        let (mut callback, mut worker) = live_plugin_pair();
        for n in 0..1_024 {
            let value = StereoFrame {
                left: n as f32 / 1_024.0,
                right: 0.0,
            };
            let output = callback.process(value);
            if n < LIVE_DELAY {
                assert_eq!(output, StereoFrame::ZERO);
            } else {
                let original = (n - LIVE_DELAY) as f32 / 1_024.0;
                // The worker returns a frame multiplied by 2.
                assert!((output.left - original * 2.0).abs() < 0.000001, "frame {n}");
            }
            if n % LIVE_BLOCK == LIVE_BLOCK - 1 {
                let in_block = worker.recv().expect("callback sent block without blocking");
                let mut result = in_block;
                for sample in &mut result.frames {
                    sample.left *= 2.0;
                }
                assert!(worker.send(result));
            }
        }
        assert!(!worker.stopped());
        drop(callback);
        assert!(worker.stopped());
    }

    #[test]
    fn late_or_crashed_plugin_falls_back_to_aligned_dry_audio() {
        let (mut callback, _idle_worker) = live_plugin_pair();
        for n in 0..2_048 {
            let out = callback.process(StereoFrame {
                left: n as f32 / 2_048.0,
                right: -(n as f32 / 2_048.0),
            });
            if n >= LIVE_DELAY {
                let expected = (n - LIVE_DELAY) as f32 / 2_048.0;
                assert!((out.left - expected).abs() < 0.000001);
                assert!((out.right + expected).abs() < 0.000001);
            }
        }
    }
}

#[cfg(test)]
mod live_reset_tests {
    use super::*;

    #[test]
    fn transport_seek_never_replays_stale_clap_audio_from_previous_epoch() {
        let (mut callback, mut worker) = live_plugin_pair();
        for _ in 0..LIVE_BLOCK {
            callback.process(StereoFrame {
                left: 0.3,
                right: 0.3,
            });
        }
        let mut stale = worker.recv().expect("pre-seek audio was queued");
        stale.frames.fill(StereoFrame {
            left: 99.0,
            right: 99.0,
        });
        assert!(worker.send(stale));
        callback.reset();
        for n in 0..(LIVE_DELAY + LIVE_BLOCK) {
            let out = callback.process(StereoFrame {
                left: 0.2,
                right: 0.2,
            });
            let expected = if n < LIVE_DELAY { 0.0 } else { 0.2 };
            assert!(
                (out.left - expected).abs() < 1e-6,
                "frame {n} stale plugin output"
            );
        }
    }
}

#[cfg(test)]
mod live_state_control_tests {
    use super::*;

    #[test]
    fn state_request_is_bounded_and_returns_worker_bytes() {
        let (_callback, worker) = live_plugin_pair();
        let control = worker.control();
        let responder = control.clone();
        let thread = std::thread::spawn(move || {
            let deadline = std::time::Instant::now() + Duration::from_secs(2);
            while std::time::Instant::now() < deadline {
                if responder.take_state_request() {
                    responder.deliver_state(Ok(b"preset-bytes".to_vec()));
                    return;
                }
                std::thread::sleep(Duration::from_millis(1));
            }
        });
        assert_eq!(
            control.capture_state(Duration::from_secs(1)).unwrap(),
            b"preset-bytes"
        );
        thread.join().unwrap();
    }

    #[test]
    fn missing_worker_state_response_times_out_instead_of_blocking_save() {
        let (_callback, worker) = live_plugin_pair();
        let started = std::time::Instant::now();
        assert!(
            worker
                .control()
                .capture_state(Duration::from_millis(30))
                .is_err()
        );
        assert!(started.elapsed() < Duration::from_secs(1));
    }
}

#[cfg(test)]
mod live_instrument_event_tests {
    use super::*;
    #[test]
    fn notes_keep_exact_sample_offsets_in_fixed_size_audio_blocks() {
        let (mut effect, mut worker) = live_plugin_pair();
        effect.note_on(60, 100);
        for _ in 0..64 {
            effect.process(StereoFrame::ZERO);
        }
        effect.note_off(60);
        for _ in 64..LIVE_BLOCK {
            effect.process(StereoFrame::ZERO);
        }
        let block = worker.recv().unwrap();
        assert_eq!(block.midi_count, 2);
        assert_eq!(
            block.midi[0],
            LiveMidiEvent {
                offset: 0,
                note: 60,
                velocity: 100,
                kind: 1,
                channel: 0,
            }
        );
        assert_eq!(
            block.midi[1],
            LiveMidiEvent {
                offset: 64,
                note: 60,
                velocity: 0,
                kind: 0,
                channel: 0,
            }
        );
    }
    #[test]
    fn overflow_emits_panic_instead_of_dropping_release_events_silently() {
        let (mut effect, mut worker) = live_plugin_pair();
        for note in 0..80 {
            effect.note_on(note, 90);
        }
        for _ in 0..LIVE_BLOCK {
            effect.process(StereoFrame::ZERO);
        }
        let block = worker.recv().unwrap();
        assert_eq!(block.midi_count as usize, LIVE_MIDI_CAPACITY);
        assert_eq!(block.midi[LIVE_MIDI_CAPACITY - 1].kind, 2);
    }
}
