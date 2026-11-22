// PulseAudio microphone capture. The capture sink enqueues frames for a
// separate WAV worker without disk access, locks, or allocations.
// All files are saved outside the application/repository under ~/Music.
use daw_core::model::RecordInputChannels;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64};
use futures::executor::block_on;
use pulseaudio::protocol::{self as pulse, stream::StreamFlags};
use std::os::unix::net::UnixStream;

struct ActiveRecording {
    input_stream: Option<PinnedCaptureStream>,
    stop: Arc<AtomicBool>,
    worker: Option<std::thread::JoinHandle<Result<u64, String>>>,
    dropped: Arc<AtomicU64>,
    path: PathBuf,
    track_id: u64,
    start_frame: u64,
    arm_frame: u64,
    armed_ns: u64,
    clock: Arc<CaptureClock>,
    output_delay_frames: u64,
    monitor: Arc<RecordingMonitor>,
    preview: Arc<CapturePreview>,
    alignment: RecordingAlignment,
    progress_frames: u64,
    progress_at: std::time::Instant,
}

/// Predicted capture/playback geometry for one take. All values are estimates
/// derived from PulseAudio timing and CPAL output timestamps, never an acoustic round-trip
/// measurement. They are logged so alignment can be audited after a take.
#[derive(Debug, Clone, Copy, Default)]
struct RecordingAlignment {
    open_ms: u64,
    first_buffer_frames: u64,
    output_delay_frames: u64,
    arm_frame: u64,
}

impl RecordingAlignment {
    fn first_buffer_ms(self, rate: u32) -> f64 {
        self.first_buffer_frames as f64 * 1000.0 / rate as f64
    }
    fn output_delay_ms(self, rate: u32) -> f64 {
        self.output_delay_frames as f64 * 1000.0 / rate as f64
    }
}

fn ns_to_frames(ns: u64, rate: u32) -> u64 {
    ((u128::from(ns) * u128::from(rate)) / 1_000_000_000) as u64
}

/// Place frame zero of the take at the project frame the performer actually
/// heard when it was captured:
///   * `arm_frame` is the transport frame when capture was armed.
///   * the first accepted hardware buffer was captured `first_buffer_frames`
///     later, so that is where the WAV's first sample belongs.
///   * output already rendered but not yet audible is not part of the heard
///     timeline, so the predicted output delay is removed again.
fn aligned_start_frame(arm_frame: u64, first_buffer_frames: u64, output_delay_frames: u64) -> u64 {
    arm_frame
        .saturating_add(first_buffer_frames)
        .saturating_sub(output_delay_frames)
}

impl Drop for ActiveRecording {
    fn drop(&mut self) {
        self.input_stream.take();
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() { let _ = worker.join(); }
    }
}

const PREVIEW_BINS: usize = 2048;

struct CapturePreview(Mutex<PreviewBins>);

struct PreviewBins {
    peaks: [f32; PREVIEW_BINS],
    len: usize,
    frames_per_bin: u64,
}

impl CapturePreview {
    fn new() -> Self {
        Self(Mutex::new(PreviewBins {
            peaks: [0.0; PREVIEW_BINS],
            len: 0,
            frames_per_bin: 256,
        }))
    }

    fn snapshot(&self) -> (u64, Vec<f32>) {
        let bins = self.0.lock().unwrap();
        (bins.frames_per_bin, bins.peaks[..bins.len].to_vec())
    }
}

impl PreviewBins {
    fn push(&mut self, peak: f32) -> u64 {
        self.peaks[self.len] = peak;
        self.len += 1;
        if self.len == self.peaks.len() {
            for index in 0..self.len / 2 {
                self.peaks[index] = self.peaks[index * 2].max(self.peaks[index * 2 + 1]);
            }
            self.len /= 2;
            self.frames_per_bin *= 2;
        }
        self.frames_per_bin
    }
}

fn spawn_wav_writer(
    path: PathBuf,
    mut consumer: rtrb::Consumer<StereoFrame>,
    stopped: Arc<AtomicBool>,
    rate: u32,
    channels: u16,
    preview: Arc<CapturePreview>,
) -> Result<std::thread::JoinHandle<Result<u64, String>>, String> {
    std::thread::Builder::new().name("rustie-wav-recorder".into())
        .spawn(move || -> Result<u64, String> {
            let spec = hound::WavSpec { channels, sample_rate: rate,
                bits_per_sample: 32, sample_format: hound::SampleFormat::Float };
            let mut wav = hound::WavWriter::create(path, spec)
                .map_err(|e| format!("Create recording: {e}"))?;
            let mut frames = 0_u64;
            let mut frames_per_bin = preview.0.lock().unwrap().frames_per_bin;
            let mut peak = 0.0_f32;
            loop {
                // Read the stop flag before checking the ring. Once stop is
                // visible, all frames published by the retired input stream
                // must be visible to this pop too.
                let stopping = stopped.load(Ordering::Acquire);
                match consumer.pop() {
                    Ok(frame) => {
                        wav.write_sample(frame.left).map_err(|e| e.to_string())?;
                        if channels == 2 {
                            wav.write_sample(frame.right).map_err(|e| e.to_string())?;
                        }
                        frames += 1;
                        peak = peak.max(frame.left.abs()).max(frame.right.abs());
                        if frames.is_multiple_of(frames_per_bin) {
                            frames_per_bin = preview.0.lock().unwrap().push(peak);
                            peak = 0.0;
                        }
                    }
                    Err(_) if stopping => break,
                    Err(_) => std::thread::sleep(Duration::from_millis(2)),
                }
            }
            wav.finalize().map_err(|e| e.to_string())?;
            Ok(frames)
        }).map_err(|e| format!("Start WAV writer: {e}"))
}

const CAPTURE_FRAGMENT_FRAMES: u32 = 512;
const CAPTURE_OPEN_TIMEOUT: Duration = Duration::from_secs(2);
const CAPTURE_STALL_TIMEOUT: Duration = Duration::from_millis(1500);
const INPUT_DISCOVERY_INTERVAL: Duration = Duration::from_secs(2);

struct PinnedCaptureStream {
    socket: UnixStream,
    callback_done: std::sync::mpsc::Receiver<()>,
}

impl Drop for PinnedCaptureStream {
    fn drop(&mut self) {
        // A dedicated connection lets teardown immediately cancel outstanding
        // protocol operations. The sink is dropped after its last callback.
        let _ = self.socket.shutdown(std::net::Shutdown::Both);
        let _ = self.callback_done.recv();
    }
}

struct CaptureClock {
    origin: std::time::Instant,
    armed_ns: AtomicU64,
    first_capture: AtomicU64,
    first_callback: AtomicU64,
    input_age_ns: AtomicU64,
    failed: AtomicBool,
    last_buffer_ns: AtomicU64,
    meter_peak: AtomicU32,
    accepted_frames: AtomicU64,
}

impl Default for CaptureClock {
    fn default() -> Self {
        Self {
            origin: Instant::now(),
            armed_ns: AtomicU64::new(u64::MAX),
            first_capture: AtomicU64::new(u64::MAX),
            first_callback: AtomicU64::new(0),
            input_age_ns: AtomicU64::new(0),
            failed: AtomicBool::new(false),
            last_buffer_ns: AtomicU64::new(u64::MAX),
            meter_peak: AtomicU32::new(0),
            accepted_frames: AtomicU64::new(0),
        }
    }
}

impl CaptureClock {
    fn elapsed_ns(&self) -> u64 {
        self.origin.elapsed().as_nanos().min(u64::MAX as u128 - 1) as u64
    }
}

struct CaptureSink {
    physical_channels: usize,
    selected_channels: RecordInputChannels,
    producer: rtrb::Producer<StereoFrame>,
    dropped: Arc<AtomicU64>,
    monitor: Arc<RecordingMonitor>,
    clock: Arc<CaptureClock>,
    done: std::sync::mpsc::SyncSender<()>,
}

impl Drop for CaptureSink {
    fn drop(&mut self) {
        let _ = self.done.try_send(());
    }
}

fn capture_frame(bytes: &[u8], channels: RecordInputChannels) -> StereoFrame {
    let read = |index: usize| {
        let offset = index * 4;
        f32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
    };
    match channels {
        RecordInputChannels::Mono1 => {
            let sample = read(0);
            StereoFrame { left: sample, right: sample }
        }
        RecordInputChannels::Mono2 => {
            let sample = read(1);
            StereoFrame { left: sample, right: sample }
        }
        RecordInputChannels::Stereo => StereoFrame { left: read(0), right: read(1) },
    }
}

impl pulseaudio::RecordSink for CaptureSink {
    fn write(&mut self, data: &[u8]) {
        let stride = self.physical_channels * 4;
        if data.len() < stride {
            return;
        }
        let callback_ns = self.clock.elapsed_ns();
        let armed_ns = self.clock.armed_ns.load(Ordering::Acquire);
        let mut recording = armed_ns != u64::MAX;
        if recording && self.clock.first_capture.load(Ordering::Relaxed) == u64::MAX {
            let age = self.clock.input_age_ns.load(Ordering::Relaxed);
            let captured_ns = callback_ns.saturating_sub(age);
            recording = capture_time_is_fresh(captured_ns, armed_ns);
            if recording {
                self.clock.first_callback.store(callback_ns, Ordering::Relaxed);
                self.clock.first_capture.store(captured_ns, Ordering::Release);
            }
        }
        let mut peak = 0.0_f32;
        let mut accepted = 0;
        for bytes in data.chunks_exact(stride) {
            let frame = capture_frame(bytes, self.selected_channels);
            peak = peak.max(frame.left.abs()).max(frame.right.abs());
            if recording {
                self.monitor.write(frame);
                accepted += u64::from(queue_capture_frame(&mut self.producer, &self.dropped, frame));
            }
        }
        self.clock.accepted_frames.fetch_add(accepted, Ordering::Release);
        self.clock.meter_peak.store(peak.to_bits(), Ordering::Relaxed);
        self.clock.last_buffer_ns.store(callback_ns, Ordering::Release);
    }
}

fn update_capture_latency(
    stream: &pulseaudio::RecordStream,
    clock: &CaptureClock,
) -> Result<(), pulseaudio::ClientError> {
    let timing = block_on(stream.timing_info())?;
    let bytes = timing.write_offset.saturating_sub(timing.read_offset).max(0) as usize;
    let input_age = Duration::from_micros(timing.source_usec)
        .saturating_add(stream.sample_spec().bytes_to_duration(bytes));
    clock.input_age_ns.store(
        input_age.as_nanos().min(u64::MAX as u128) as u64,
        Ordering::Relaxed,
    );
    Ok(())
}

fn open_pinned_capture(
    source_id: &cpal::DeviceId,
    rate: u32,
    sink: CaptureSink,
    callback_done: std::sync::mpsc::Receiver<()>,
) -> Result<(PinnedCaptureStream, CaptureReady), String> {
    let source_name = std::ffi::CString::new(source_id.id()).map_err(|e| e.to_string())?;
    let server = pulseaudio::socket_path_from_env().ok_or("PulseAudio capture server unavailable")?;
    let socket = UnixStream::connect(server).map_err(|e| format!("Connect recording input: {e}"))?;
    socket.set_read_timeout(Some(CAPTURE_OPEN_TIMEOUT)).map_err(|e| e.to_string())?;
    socket.set_write_timeout(Some(CAPTURE_OPEN_TIMEOUT)).map_err(|e| e.to_string())?;
    let connection = socket.try_clone().map_err(|e| e.to_string())?;
    let handle = PinnedCaptureStream { socket, callback_done };
    let cookie = pulseaudio::cookie_path_from_env().and_then(|path| fs::read(path).ok());
    let clock = Arc::clone(&sink.clock);
    let channels = sink.physical_channels as u8;
    let (ready, ready_rx) = std::sync::mpsc::sync_channel(1);
    std::thread::Builder::new().name("rustie-input-clock".into()).spawn(move || {
        let result = (|| -> Result<(), String> {
            let client = pulseaudio::Client::new_unix(c"Rusty input", connection, cookie)
                .map_err(|e| e.to_string())?;
            let source = block_on(client.source_info_by_name(source_name))
                .map_err(|e| format!("Selected source unavailable: {e}"))?;
            if source.monitor_of_sink_index.is_some() {
                return Err("Selected source is an output monitor".into());
            }
            if channels != source.sample_spec.channels {
                return Err("The selected input's channel count changed".into());
            }
            let format = pulse::SampleFormat::Float32Le;
            let spec = pulse::SampleSpec { format, channels, sample_rate: rate };
            // Preserve the source's physical channel order and the bounded
            // fragment request that avoids stale PulseAudio pre-roll.
            let fragment_bytes = CAPTURE_FRAGMENT_FRAMES * u32::from(channels) * 4;
            let params = pulse::RecordStreamParams {
                sample_spec: spec,
                channel_map: source.channel_map,
                source_index: Some(source.index),
                buffer_attr: pulse::stream::BufferAttr {
                    max_length: fragment_bytes,
                    fragment_size: fragment_bytes,
                    ..Default::default()
                },
                flags: StreamFlags {
                    start_corked: true,
                    adjust_latency: true,
                    no_remap_channels: true,
                    // CPAL sets the source index but leaves it movable. A
                    // server can rescue the stream onto another microphone.
                    no_move: true,
                    ..Default::default()
                },
                ..Default::default()
            };
            let stream = block_on(client.create_record_stream(params, sink))
                .map_err(|e| e.to_string())?;
            if stream.sink() != source.index
                || stream.sample_spec() != &spec
                || stream.channel_map() != &source.channel_map
            {
                return Err("PulseAudio returned a different recording source or format".into());
            }
            block_on(stream.uncork()).map_err(|e| e.to_string())?;
            block_on(stream.started()).map_err(|e| e.to_string())?;
            update_capture_latency(&stream, &clock).map_err(|e| e.to_string())?;
            if ready.send(Ok(())).is_err() {
                return Ok(());
            }
            loop {
                std::thread::sleep(Duration::from_millis(100));
                update_capture_latency(&stream, &clock).map_err(|e| e.to_string())?;
            }
        })();
        if let Err(error) = result {
            clock.failed.store(true, Ordering::Release);
            let _ = ready.try_send(Err(error));
        }
    }).map_err(|e| format!("Start recording clock: {e}"))?;
    Ok((handle, ready_rx))
}

type CaptureReady = std::sync::mpsc::Receiver<Result<(), String>>;

struct CaptureInput {
    stream: PinnedCaptureStream,
    ready: Option<CaptureReady>,
    consumer: rtrb::Consumer<StereoFrame>,
    dropped: Arc<AtomicU64>,
    clock: Arc<CaptureClock>,
    monitor: Arc<RecordingMonitor>,
}

impl CaptureInput {
    fn open(
        source: &CaptureSource,
        channels: RecordInputChannels,
        rate: u32,
        monitor: Arc<RecordingMonitor>,
    ) -> Result<Self, String> {
        if !channels.supported_by(source.channels) {
            return Err(format!("{} is unavailable on {}", channels.label(), source.label));
        }
        let (producer, consumer) = rtrb::RingBuffer::<StereoFrame>::new(rate as usize * 3);
        let (done, callback_done) = std::sync::mpsc::sync_channel(1);
        let clock = Arc::new(CaptureClock::default());
        let dropped = Arc::new(AtomicU64::new(0));
        let sink = CaptureSink {
            physical_channels: source.channels,
            selected_channels: channels,
            producer,
            monitor: Arc::clone(&monitor),
            done,
            dropped: Arc::clone(&dropped),
            clock: Arc::clone(&clock),
        };
        let (stream, ready) = open_pinned_capture(&source.id, rate, sink, callback_done)?;
        Ok(Self { stream, ready: Some(ready), consumer, dropped, clock, monitor })
    }

    fn poll_ready(&mut self) -> Result<bool, String> {
        if let Some(ready) = &self.ready {
            match ready.try_recv() {
                Ok(result) => {
                    result?;
                    self.ready = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty)
                    if self.clock.origin.elapsed() < CAPTURE_OPEN_TIMEOUT =>
                {
                    return Ok(false);
                }
                Err(error) => return Err(format!("Wait for recording input: {error}")),
            }
        }
        if self.clock.failed.load(Ordering::Acquire) {
            return Err("Input device disconnected".into());
        }
        let last_buffer_ns = self.clock.last_buffer_ns.load(Ordering::Acquire);
        let elapsed = if last_buffer_ns == u64::MAX {
            self.clock.elapsed_ns()
        } else {
            self.clock.elapsed_ns().saturating_sub(last_buffer_ns)
        };
        if elapsed >= CAPTURE_STALL_TIMEOUT.as_nanos() as u64 {
            return Err("Input device stopped delivering frames".into());
        }
        Ok(true)
    }

    fn wait_ready(&mut self) -> Result<(), String> {
        if let Some(ready) = self.ready.take() {
            ready.recv_timeout(CAPTURE_OPEN_TIMEOUT)
                .map_err(|error| format!("Wait for recording input: {error}"))??;
        }
        self.poll_ready().map(|_| ())
    }

    fn prepare_take(
        self,
        path: &Path,
        rate: u32,
        channels: RecordInputChannels,
    ) -> Result<CaptureSession, String> {
        let stopped = Arc::new(AtomicBool::new(false));
        let preview = Arc::new(CapturePreview::new());
        let writer = spawn_wav_writer(
            partial_path(path),
            self.consumer,
            Arc::clone(&stopped),
            rate,
            if channels == RecordInputChannels::Stereo { 2 } else { 1 },
            Arc::clone(&preview),
        )?;
        Ok(CaptureSession {
            stream: self.stream,
            stopped,
            writer,
            dropped: self.dropped,
            clock: self.clock,
            preview,
        })
    }
}

struct CaptureSession {
    stream: PinnedCaptureStream,
    stopped: Arc<AtomicBool>,
    writer: std::thread::JoinHandle<Result<u64, String>>,
    dropped: Arc<AtomicU64>,
    clock: Arc<CaptureClock>,
    preview: Arc<CapturePreview>,
}

fn arm_capture(clock: &CaptureClock) -> u64 {
    let armed_ns = clock.elapsed_ns();
    clock.armed_ns.store(armed_ns, Ordering::Release);
    armed_ns
}

/// Reject pre-roll using the first-sample time estimated from PulseAudio latency.
fn capture_time_is_fresh(captured_ns: u64, armed_ns: u64) -> bool {
    armed_ns != u64::MAX && captured_ns >= armed_ns
}

/// Bounded enqueue used by both sample-format callbacks. A full ring is an
/// explicit, counted overrun: the take keeps the frames already accepted
/// rather than blocking the realtime callback on disk or sleeping.
#[inline]
fn queue_capture_frame(
    producer: &mut rtrb::Producer<StereoFrame>,
    dropped: &AtomicU64,
    frame: StereoFrame,
) -> bool {
    if producer.push(frame).is_ok() {
        true
    } else {
        dropped.fetch_add(1, Ordering::Relaxed);
        false
    }
}

/// Takes are written under a `.partial` suffix and only renamed after the WAV
/// header has been finalized. A crash, full disk or writer error therefore
/// leaves a clearly unfinished file instead of a plausible complete take.
fn partial_path(path: &Path) -> PathBuf {
    let mut value = path.as_os_str().to_os_string();
    value.push(".partial");
    PathBuf::from(value)
}

fn finalize_take(partial: &Path, final_path: &Path) -> Result<(), String> {
    std::fs::rename(partial, final_path).map_err(|e| {
        format!(
            "Recording finished but could not be published ({} → {}): {e}",
            partial.display(),
            final_path.display()
        )
    })
}

/// Convert a sample count at a constant tempo to musical ticks.
fn frames_to_ticks(frames: u64, rate: u32, bpm: f64) -> i64 {
    if rate == 0 {
        return 0;
    }
    ((frames as f64 / f64::from(rate)) * bpm * PPQ as f64 / 60.0)
        .round()
        .clamp(0.0, i64::MAX as f64) as i64
}

#[derive(Clone)]
struct CaptureSource {
    id: cpal::DeviceId,
    label: String,
    channels: usize,
}

fn is_physical_capture_source(id: &str, label: &str) -> bool {
    // PulseAudio source names ending in ".monitor" belong to sinks. The label
    // is user-facing and may be renamed or localized.
    !id.ends_with(".monitor") && !label.to_ascii_lowercase().starts_with("monitor of ")
}

fn select_capture_source<'a>(
    requested: Option<&cpal::DeviceId>,
    sources: &'a [CaptureSource],
) -> Result<&'a CaptureSource, String> {
    if let Some(id) = requested {
        return sources.iter().find(|source| source.id == *id).ok_or_else(|| {
            "The selected recording device is unavailable. Choose a new input in Devices.".into()
        });
    }
    sources
        .iter()
        .find(|source| source.label.to_lowercase().contains("air 192"))
        .or_else(|| sources.first())
        .ok_or_else(|| {
            "No microphone input found in PulseAudio. Check Devices and audio permissions".into()
        })
}

fn select_track_capture_source<'a>(
    stable_id: Option<&str>,
    legacy_label: Option<&str>,
    sources: &'a [CaptureSource],
) -> Result<&'a CaptureSource, String> {
    if let Some(id) = stable_id {
        return select_capture_source(Some(&cpal::DeviceId::new(cpal::HostId::PulseAudio, id)), sources);
    }
    let Some(label) = legacy_label else {
        return select_capture_source(None, sources);
    };
    let mut matches = sources.iter().filter(|source| source.label == label);
    match (matches.next(), matches.next()) {
        (Some(source), None) => Ok(source),
        _ => Err("The saved recording label is missing or ambiguous. Choose its input in Devices.".into()),
    }
}

enum TrackInputStatus<'a> {
    Selected,
    Ready { peak: f32 },
    Unavailable(&'a str),
}

struct InputProbe {
    track_id: u64,
    source_id: cpal::DeviceId,
    channels: RecordInputChannels,
    rate: u32,
    input: Result<CaptureInput, String>,
    attempted_at: Instant,
}

impl InputProbe {
    fn matches(&self, source: &CaptureSource, channels: RecordInputChannels, rate: u32) -> bool {
        self.source_id == source.id && self.channels == channels && self.rate == rate
    }
}

impl Studio {
    fn available_input_devices() -> Result<Vec<CaptureSource>, String> {
        let server = pulseaudio::socket_path_from_env()
            .ok_or("PulseAudio server socket unavailable")?;
        let socket = UnixStream::connect(server).map_err(|err| err.to_string())?;
        socket.set_read_timeout(Some(CAPTURE_OPEN_TIMEOUT)).map_err(|err| err.to_string())?;
        socket.set_write_timeout(Some(CAPTURE_OPEN_TIMEOUT)).map_err(|err| err.to_string())?;
        let connection = socket.try_clone().map_err(|err| err.to_string())?;
        let cookie = pulseaudio::cookie_path_from_env().and_then(|path| fs::read(path).ok());
        let sources = (|| {
            let client = pulseaudio::Client::new_unix(c"Rusty input discovery", connection, cookie)?;
            block_on(client.list_sources())
        })();
        // Explicitly close the socket so the client's reactor cannot outlive
        // this short-lived discovery request.
        let _ = socket.shutdown(std::net::Shutdown::Both);
        let sources = sources.map_err(|err| err.to_string())?;
        Ok(sources.into_iter().filter_map(|source| {
            let id = source.name.to_str().ok()?;
            let label = source.description.as_deref()
                .and_then(|description| description.to_str().ok()).unwrap_or(id);
            (source.monitor_of_sink_index.is_none() && is_physical_capture_source(id, label))
                .then(|| CaptureSource {
                    id: cpal::DeviceId::new(cpal::HostId::PulseAudio, id),
                    label: label.to_owned(),
                    channels: usize::from(source.sample_spec.channels),
                })
        }).collect())
    }

    fn capture_device_cache(&mut self) -> &Result<Vec<CaptureSource>, String> {
        if self.capture_devices.as_ref().is_none_or(|(checked, _)| {
            checked.elapsed() >= INPUT_DISCOVERY_INTERVAL
        }) {
            self.capture_devices = Some((Instant::now(), Self::available_input_devices()));
        }
        &self.capture_devices.as_ref().unwrap().1
    }

    fn track_capture_settings(
        &self,
        track_id: Option<u64>,
    ) -> Result<(&CaptureSource, RecordInputChannels), String> {
        let devices = self.capture_devices.as_ref()
            .ok_or("PulseAudio inputs have not been discovered")?.1.as_ref()
            .map_err(|error| format!("Cannot query PulseAudio recording inputs: {error}"))?;
        let track = track_id.and_then(|id| self.project.tracks.iter().find(|track| {
            track.id == id && track.kind == TrackKind::Audio
        }));
        let source = select_track_capture_source(
            track.and_then(|track| track.record_input_id.as_deref()),
            track.and_then(|track| track.record_input.as_deref()),
            devices,
        )?;
        let channels = track.map_or(RecordInputChannels::default(), |track| {
            track.record_input_channels
        }).for_source(source.channels);
        if !channels.supported_by(source.channels) {
            return Err(format!("{} is unavailable on {}", channels.label(), source.label));
        }
        Ok((source, channels))
    }

    fn track_input_status(&self, track_id: u64) -> TrackInputStatus<'_> {
        if let Some(recording) = self.recorder.as_ref().filter(|recording| recording.track_id == track_id) {
            return TrackInputStatus::Ready {
                peak: f32::from_bits(recording.clock.meter_peak.load(Ordering::Relaxed)),
            };
        }
        if self.selected_track != Some(track_id) {
            return TrackInputStatus::Selected;
        }
        if let Some((_, Err(error))) = &self.capture_devices {
            return TrackInputStatus::Unavailable(error);
        }
        let Some(probe) = self.input_probe.as_ref().filter(|probe| probe.track_id == track_id) else {
            if self.capture_devices.is_some() && self.track_capture_settings(Some(track_id)).is_err() {
                return TrackInputStatus::Unavailable("The selected input or channel is unavailable");
            }
            return TrackInputStatus::Selected;
        };
        match &probe.input {
            Ok(input) if input.ready.is_none()
                && input.clock.last_buffer_ns.load(Ordering::Acquire) != u64::MAX =>
            {
                TrackInputStatus::Ready {
                    peak: f32::from_bits(input.clock.meter_peak.load(Ordering::Relaxed)),
                }
            }
            Ok(_) => TrackInputStatus::Selected,
            Err(error) => TrackInputStatus::Unavailable(error),
        }
    }

    fn poll_track_input(&mut self, ctx: &egui::Context) {
        if self.recorder.is_some() {
            return;
        }
        let Some(track_id) = self.selected_track.filter(|id| {
            self.project.tracks.iter().any(|track| track.id == *id && track.kind == TrackKind::Audio)
        }) else {
            self.input_probe = None;
            return;
        };
        ctx.request_repaint_after(Duration::from_millis(50));
        self.capture_device_cache();
        let mut probe = self.input_probe.take();
        let Ok((source, channels)) = self.track_capture_settings(Some(track_id)) else {
            return;
        };
        if let Some(current) = probe.as_mut().filter(|probe| {
            probe.matches(source, channels, self.sample_rate)
        }) {
            current.track_id = track_id;
            if let Ok(input) = &mut current.input {
                if let Err(error) = input.poll_ready() {
                    current.input = Err(error);
                    current.attempted_at = Instant::now();
                }
                self.input_probe = probe;
                return;
            }
            if current.attempted_at.elapsed() < INPUT_DISCOVERY_INTERVAL {
                self.input_probe = probe;
                return;
            }
        }
        // Retire the old connection before opening a new one, even if the
        // user is moving quickly between inputs with the Devices panel open.
        drop(probe);
        self.input_probe = Some(InputProbe {
            track_id,
            source_id: source.id.clone(),
            channels,
            rate: self.sample_rate,
            input: CaptureInput::open(
                source, channels, self.sample_rate, Arc::new(RecordingMonitor::new()),
            ),
            attempted_at: Instant::now(),
        });
    }

    fn start_recording(&mut self) {
        if self.recorder.is_some() { return; }
        if self.clip_count() >= MAX_CLIPS {
            self.error = Some(format!("Free an Arrangement clip slot before recording (limit {MAX_CLIPS})"));
            return;
        }
        self.capture_device_cache();
        let (source, channels) = match self.track_capture_settings(self.selected_track) {
            Ok((source, channels)) => (source.clone(), channels),
            Err(error) => {
                self.error = Some(error);
                return;
            }
        };
        let input_name = &source.label;
        // Prepare the destination before touching the transport so a disk
        // failure cannot leave playback running with a half-armed take.
        let directory = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default()
            .join("Music/Rusty Recordings");
        if let Err(e) = fs::create_dir_all(&directory) {
            self.error = Some(format!("Recording directory: {e}")); return;
        }
        let target = self.selected_track.and_then(|id| self.project.tracks.iter()
            .find(|t| t.id == id && t.kind == TrackKind::Audio).map(|t| t.id));
        let track_id = if let Some(id) = target { id } else {
            let id = self.next_id;
            self.add_audio_track();
            if self.selected_track != Some(id) { return; }
            id
        };
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH).map_or(0, |t| t.as_millis());
        let mut path = directory.join(format!("Take-{timestamp}-{track_id}.wav"));
        let mut suffix = 1;
        while path.exists() || partial_path(&path).exists() {
            path = directory.join(format!("Take-{timestamp}-{track_id}-{suffix}.wav"));
            suffix += 1;
        }
        let input = self.input_probe.take().filter(|probe| probe.matches(&source, channels, self.sample_rate));
        let probe_rate = input.as_ref().map(|probe| probe.rate);
        let input = input.and_then(|probe| probe.input.ok());
        // An opt-in monitor uses this same pinned stream. Metering alone
        // leaves the graph untouched and discards unarmed capture frames.
        let monitor = input.as_ref().map(|input| Arc::clone(&input.monitor))
            .unwrap_or_else(|| Arc::new(RecordingMonitor::new()));
        self.view = View::Arrangement;
        if self.input_monitor_enabled {
            self.pending_monitor = Some((track_id, Arc::clone(&monitor)));
            if self.playing {
                self.restart_for_edit();
                if !self.playing {
                    self.pending_monitor = None;
                    self.error = Some("Cannot route the selected input monitor before recording".into());
                    return;
                }
            }
        }
        let started_transport = !self.playing;
        // Start the song clock before arming capture so the WAV never includes
        // samples taken before Play.
        if started_transport {
            self.start_playback();
            if !self.playing {
                self.pending_monitor = None;
                self.error = Some("Cannot start the transport for audio recording".into());
                return;
            }
        }
        let open_started = Instant::now();
        let input = input.filter(|_| probe_rate == Some(self.sample_rate))
            .map_or_else(|| CaptureInput::open(
                &source, channels, self.sample_rate, Arc::clone(&monitor)
            ), Ok);
        let recording = input.and_then(|mut input| {
            input.wait_ready()?;
            input.prepare_take(&path, self.sample_rate, channels)
        });
        match recording {
            Ok(parts) => {
                self.pending_monitor = None;
                if let Some(track) = self.project.track_mut(track_id) {
                    // Resolve Auto once a take actually opens. Future takes on
                    // this track then refuse an accidental silent substitution.
                    track.record_input = Some(input_name.clone());
                    track.record_input_id = Some(source.id.id().to_owned());
                    track.record_input_channels = channels;
                    self.dirty = true;
                }
                let arm_frame = self.midi_transport_frame();
                let armed_ns = arm_capture(&parts.clock);
                let output_delay_frames = self.driver.as_ref().map_or(0, |driver| {
                    let micros = driver
                        .telemetry()
                        .predicted_playback_delay_us
                        .load(Ordering::Relaxed);
                    ns_to_frames(micros.saturating_mul(1_000), self.sample_rate)
                });
                // Provisional start; the true first-buffer offset is resolved
                // when the callback delivers its first accepted buffer.
                let start_frame = aligned_start_frame(arm_frame, 0, output_delay_frames);
                let alignment = RecordingAlignment {
                    open_ms: open_started.elapsed().as_millis() as u64,
                    first_buffer_frames: 0,
                    output_delay_frames,
                    arm_frame,
                };
                self.recorder = Some(ActiveRecording {
                    input_stream: Some(parts.stream), stop: parts.stopped,
                    worker: Some(parts.writer), dropped: parts.dropped,
                    path: path.clone(), track_id, start_frame, arm_frame,
                    armed_ns,
                    clock: parts.clock,
                    output_delay_frames, monitor,
                    preview: parts.preview, alignment,
                    progress_frames: 0, progress_at: std::time::Instant::now(),
                });
                self.status = format!("Recording → {input_name} (click REC or Stop to finish)");
                self.error = None;
                // The output-delay estimate comes from the
                // output stream's reported delay, never an acoustic loopback.
                eprintln!(
                    "Rusty recording start: input={input_name}; rate={}Hz; open_ms={}; arm_frame={}; predicted_output_delay_ms={:.2}; provisional_clip_start_frame={}",
                    self.sample_rate,
                    alignment.open_ms,
                    alignment.arm_frame,
                    alignment.output_delay_ms(self.sample_rate),
                    start_frame,
                );
            }
            Err(error) => {
                self.pending_monitor = None;
                if started_transport {
                    self.pause_playback();
                }
                self.error = Some(error);
            }
        }
    }

    fn finish_recording(&mut self) {
        let Some(mut recording) = self.recorder.take() else { return };
        self.error = None;
        // Drop the capture stream *before* stopping the worker, guaranteeing
        // no more writes into the queue after the reader drains it.
        recording.input_stream.take();
        recording.stop.store(true, Ordering::Release);
        let result = recording.worker.take().and_then(|worker| worker.join().ok())
            .unwrap_or_else(|| Err("Recording writer thread crashed".into()));
        let dropped = recording.dropped.load(Ordering::Relaxed);
        let partial = partial_path(&recording.path);
        match result {
            Ok(frames) if frames > 0 => {
                let path = recording.path.clone();
                let first_capture = recording.clock.first_capture.load(Ordering::Acquire);
                let first_buffer_frames = if first_capture == u64::MAX {
                    0
                } else {
                    ns_to_frames(
                        first_capture.saturating_sub(recording.armed_ns),
                        self.sample_rate,
                    )
                };
                let start_frame = aligned_start_frame(
                    recording.arm_frame,
                    first_buffer_frames,
                    recording.output_delay_frames,
                );
                let first_callback = recording.clock.first_callback.load(Ordering::Relaxed);
                let mut alignment = recording.alignment;
                alignment.first_buffer_frames = first_buffer_frames;
                // A finalized WAV only becomes a project take after a clean
                // rename. A failed rename/finalize leaves the .partial file in
                // place and never exposes a plausible complete clip.
                if let Err(error) = finalize_take(&partial, &path) {
                    self.error = Some(error);
                    self.status = "Recording not added to the project".into();
                    return;
                }
                if self.clip_count() >= MAX_CLIPS {
                    self.error = Some(format!("Arrangement clip limit reached. Free a slot and import the saved take at {}", path.display()));
                    self.status = format!("WAV take saved • {dropped} dropped input frames");
                    return;
                }
                let decoded = self.decode_cached(&path);
                if let Err(error) = decoded { self.error = Some(error); return; }
                let tempo = self.project.tempo_map.tempo_at(MusicalTime::ZERO)
                    .map_or(120.0, |t| t.bpm());
                let start_ticks = frames_to_ticks(start_frame, self.sample_rate, tempo);
                let ticks = frames_to_ticks(frames, self.sample_rate, tempo).max(1);
                let id = self.next_id;
                self.next_id = self.next_id.saturating_add(1);
                if let Some(track) = self.project.track_mut(recording.track_id) {
                    track.clips.push(Clip {
                        id, start: MusicalTime::from_ticks(start_ticks),
                        duration: MusicalTime::from_ticks(ticks),
                        kind: ClipKind::Audio(AudioClip { source: path.clone(), warped: false, source_offset_frames: 0,
                gain: 1.0, fade_in_ms: 0, fade_out_ms: 0 }),
                        scale_override: None,
                    });
                    self.selected_track = Some(recording.track_id);
                    self.selected_clip = Some(id);
                    self.detail = Detail::Clip;
                    self.dirty = true;
                    self.status = format!(
                        "Saved WAV take ({:.2}s, {} frames) to {} • {} dropped input frames",
                        frames as f64 / self.sample_rate as f64, frames, path.display(), dropped);
                    if dropped > 0 {
                        self.error = Some(format!(
                            "Recording buffer overran: {dropped} frames were lost. The WAV was preserved, but later audio may be shifted earlier on the timeline; review the take before using it."
                        ));
                    }
                    eprintln!(
                        "Rusty recording finished: frames={frames}; dropped={dropped}; rate={}Hz; start_frame={}; arm_frame={}; first_capture_after_arm_ms={:.2}; first_callback_after_arm_ms={:.2}; predicted_output_delay_ms={:.2}",
                        self.sample_rate,
                        start_frame,
                        alignment.arm_frame,
                        alignment.first_buffer_ms(self.sample_rate),
                        first_callback.saturating_sub(recording.armed_ns) as f64 / 1_000_000.0,
                        alignment.output_delay_ms(self.sample_rate),
                    );
                    if self.playing { self.restart_for_edit(); }
                }
            }
            Ok(_) => {
                let _ = fs::remove_file(&partial);
                self.error = Some("Recording had no input samples; nothing was added".into());
                self.status = "Empty recording discarded".into();
            }
            Err(e) => {
                // Keep whatever reached disk, but name it honestly and never
                // present an unfinalized/truncated take as a complete clip.
                let detail = if partial.exists() {
                    format!("Unfinalized audio remains at {}.", partial.display())
                } else {
                    "No take file was created.".into()
                };
                self.error = Some(format!("Recording writer failed: {e}. {detail}"));
                eprintln!("Rusty recording writer failed: captured_frames={}; dropped={dropped}; rate={}Hz; {e}",
                    recording.clock.accepted_frames.load(Ordering::Acquire), self.sample_rate);
                self.status = "Recording failed; partial take not imported".into();
            }
        }
    }

    /// Detect a microphone that stopped delivering frames mid-take. The take
    /// is finalized rather than discarded: audio already written is valid and
    /// the user gets an explicit "partial take saved" error instead of a
    /// recording that silently looks alive.
    fn poll_recording_input(&mut self) {
        let Some(recording) = self.recorder.as_ref() else {
            return;
        };
        // An active writer exits only on error. Stop the input immediately
        // instead of letting the UI show a recording into a dead queue.
        if recording.worker.as_ref().is_some_and(|writer| writer.is_finished()) {
            self.finish_recording();
            return;
        }
        if recording.clock.failed.load(Ordering::Acquire) {
            self.driver = None;
            self.playing = false;
            self.finish_recording();
            let outcome = self.error.take().unwrap_or_else(|| self.status.clone());
            self.error = Some(format!("Capture device stopped delivering audio. {outcome}"));
            self.status = "Recording stopped: capture device failed".into();
            return;
        }
        let rate = self.sample_rate;
        let stalled_frames = {
            let Some(recording) = self.recorder.as_mut() else {
                return;
            };
            let frames = recording.clock.accepted_frames.load(Ordering::Acquire);
            if frames != recording.progress_frames {
                recording.progress_frames = frames;
                recording.progress_at = std::time::Instant::now();
                let first = recording.clock.first_capture.load(Ordering::Acquire);
                if first != u64::MAX {
                    let offset =
                        ns_to_frames(first.saturating_sub(recording.armed_ns), rate);
                    if offset != recording.alignment.first_buffer_frames {
                        recording.alignment.first_buffer_frames = offset;
                        recording.start_frame = aligned_start_frame(
                            recording.arm_frame,
                            offset,
                            recording.output_delay_frames,
                        );
                    }
                }
                return;
            }
            if recording.progress_at.elapsed() < CAPTURE_STALL_TIMEOUT {
                return;
            }
            frames
        };
        let seconds = stalled_frames as f64 / self.sample_rate as f64;
        self.driver = None;
        self.playing = false;
        self.finish_recording();
        let outcome = self.error.take().unwrap_or_else(|| self.status.clone());
        self.error = Some(format!(
            "Recording input stopped delivering frames at {seconds:.2}s. {outcome}"
        ));
        self.status = "Recording stopped: input timed out".into();
    }

    fn toggle_recording(&mut self) {        if self.midi_take.is_some() {
            self.finish_midi_take();
            if self.playing { self.restart_for_edit(); }
        } else if self.recorder.is_some() {
            self.finish_recording();
        } else if self.selected_track.is_some_and(|id| self.project.tracks.iter()
            .any(|track| track.id == id && track.kind == TrackKind::Midi)) {
            self.start_midi_take();
        } else {
            self.start_recording();
        }
    }
}

#[cfg(test)]
mod recorder_tests {
    use super::*;

    #[test]
    fn session_view_recording_checks_the_arrangement_clip_limit_before_arming() {
        let creation = eframe::CreationContext::_new_kittest(egui::Context::default());
        let mut studio = Studio::new(&creation, None);
        studio.view = View::Session;
        let track = &mut studio.project.tracks[0];
        let template = track.clips[0].clone();
        track.clips = (100..)
            .take(MAX_CLIPS - 2)
            .map(|id| Clip { id, ..template.clone() })
            .collect();
        assert_eq!(studio.clip_count(), MAX_CLIPS);
        let before = studio.project.clone();

        studio.start_recording();

        assert_eq!(studio.project, before);
        assert!(studio.recorder.is_none());
        assert!(studio.error.as_deref().unwrap().contains("Free an Arrangement clip slot"));
    }

    #[test]
    fn pre_roll_buffers_are_discarded_before_recording_start() {
        assert!(!capture_time_is_fresh(800_000_000, 1_000_000_000));
        assert!(capture_time_is_fresh(1_200_000_000, 1_000_000_000));
        assert!(!capture_time_is_fresh(1_200_000_000, u64::MAX));
    }

    #[test]
    fn input_selector_rejects_output_monitor_loopbacks() {
        assert!(is_physical_capture_source("alsa_input.usb-air", "AIR 192 6 Analog Stereo"));
        assert!(!is_physical_capture_source("alsa_output.test.monitor", "Custom label"));
        assert!(!is_physical_capture_source("other", "Monitor of AIR 192 6 Analog Stereo"));
    }

    #[test]
    fn explicit_missing_input_errors_even_when_another_device_has_the_same_label() {
        let source = |id| CaptureSource {
            id: cpal::DeviceId::new(cpal::HostId::PulseAudio, id),
            label: "AIR 192 6 Analog Stereo".into(),
            channels: 2,
        };
        let sources = [source("test_a"), source("test_b")];
        assert_eq!(select_capture_source(None, &sources).unwrap().id, sources[0].id);
        assert_eq!(
            select_capture_source(Some(&sources[1].id), &sources).unwrap().id,
            sources[1].id
        );
        assert!(select_capture_source(Some(&sources[1].id), &sources[..1]).is_err());
        let label = sources[1].label.as_str();
        assert!(select_track_capture_source(None, Some(label), &sources).is_err());
        assert_eq!(select_track_capture_source(None, Some(label), &sources[..1]).unwrap().id, sources[0].id);
        assert_eq!(select_track_capture_source(Some("test_b"), Some(label), &sources).unwrap().id, sources[1].id);
        assert!(select_track_capture_source(Some("test_b"), Some(label), &sources[..1]).is_err());
    }

    #[test]
    fn alignment_accounts_for_first_buffer_and_output_delay() {
        assert_eq!(ns_to_frames(6_000_000, 44_100), 264);
        assert_eq!(aligned_start_frame(1_000, 264, 441), 823);
        // Never underflow before the project start.
        assert_eq!(aligned_start_frame(100, 0, 441), 0);
        assert_eq!(frames_to_ticks(44_100, 44_100, 120.0), 2 * PPQ);
        assert_eq!(frames_to_ticks(0, 44_100, 120.0), 0);
    }

    #[test]
    fn full_ring_counts_overruns_and_preserves_accepted_frames() {
        let (mut producer, mut consumer) = rtrb::RingBuffer::<StereoFrame>::new(2);
        let dropped = AtomicU64::new(0);
        for value in [0.1_f32, 0.2, 0.3, 0.4] {
            queue_capture_frame(
                &mut producer,
                &dropped,
                StereoFrame {
                    left: value,
                    right: -value,
                },
            );
        }
        // Two accepted, two explicitly counted as dropped.
        assert_eq!(dropped.load(Ordering::Relaxed), 2);
        assert_eq!(consumer.pop().unwrap().left, 0.1);
        assert_eq!(consumer.pop().unwrap().left, 0.2);
        assert!(consumer.pop().is_err());
    }

    #[test]
    fn selected_channel_meters_before_arm_and_keeps_raw_samples_in_wav() {
        let lanes = [0.125_f32, -0.375, -0.0625, 0.25];
        let bytes = lanes.into_iter().flat_map(f32::to_le_bytes).collect::<Vec<_>>();
        for (channels, expected_peak, expected_samples) in [
            (RecordInputChannels::Mono1, 0.125, vec![lanes[0], lanes[2]]),
            (RecordInputChannels::Mono2, 0.375, vec![lanes[1], lanes[3]]),
            (RecordInputChannels::Stereo, 0.375, lanes.to_vec()),
        ] {
            let (producer, mut consumer) = rtrb::RingBuffer::<StereoFrame>::new(4);
            let (done, _callback_done) = std::sync::mpsc::sync_channel(1);
            let clock = Arc::new(CaptureClock::default());
            let monitor = Arc::new(RecordingMonitor::new());
            let mut sink = CaptureSink {
                physical_channels: 2,
                selected_channels: channels,
                producer,
                dropped: Arc::new(AtomicU64::new(0)),
                monitor: Arc::clone(&monitor),
                clock: Arc::clone(&clock),
                done,
            };
            pulseaudio::RecordSink::write(&mut sink, &bytes);
            assert_eq!(f32::from_bits(clock.meter_peak.load(Ordering::Relaxed)), expected_peak);
            assert_ne!(clock.last_buffer_ns.load(Ordering::Acquire), u64::MAX);
            assert_eq!(monitor.next_index(), 0);
            assert_eq!(clock.accepted_frames.load(Ordering::Acquire), 0);
            assert!(consumer.pop().is_err());

            clock.input_age_ns.store(u64::MAX, Ordering::Relaxed);
            let armed_ns = arm_capture(&clock);
            pulseaudio::RecordSink::write(&mut sink, &bytes);
            assert!(consumer.pop().is_err(), "stale pre-roll must stay out of the take");
            assert_eq!(clock.first_capture.load(Ordering::Acquire), u64::MAX);

            clock.input_age_ns.store(0, Ordering::Relaxed);
            pulseaudio::RecordSink::write(&mut sink, &bytes);
            assert!(clock.first_capture.load(Ordering::Acquire) >= armed_ns);
            assert_eq!(clock.accepted_frames.load(Ordering::Acquire), 2);
            let mut cursor = 0;
            assert_eq!(
                monitor.take(&mut cursor),
                match channels {
                    RecordInputChannels::Mono1 => StereoFrame { left: lanes[0], right: lanes[0] },
                    RecordInputChannels::Mono2 => StereoFrame { left: lanes[1], right: lanes[1] },
                    RecordInputChannels::Stereo => StereoFrame { left: lanes[0], right: lanes[1] },
                },
            );
            drop(sink);

            let path = std::env::temp_dir().join(format!(
                "rusty-capture-channel-{}-{channels:?}.wav",
                std::process::id(),
            ));
            let wav_channels = if channels == RecordInputChannels::Stereo { 2 } else { 1 };
            let writer = spawn_wav_writer(
                path.clone(),
                consumer,
                Arc::new(AtomicBool::new(true)),
                48_000,
                wav_channels,
                Arc::new(CapturePreview::new()),
            ).unwrap();
            assert_eq!(writer.join().unwrap().unwrap(), 2);
            let mut wav = hound::WavReader::open(&path).unwrap();
            assert_eq!(wav.spec().channels, wav_channels);
            let samples = wav.samples::<f32>().collect::<Result<Vec<_>, _>>().unwrap();
            assert_eq!(samples, expected_samples);
            fs::remove_file(path).unwrap();
        }
    }

    #[test]
    fn growing_preview_keeps_full_timeline_in_bounded_bins() {
        let preview = CapturePreview::new();
        let (stride, initial) = preview.snapshot();
        assert!(initial.is_empty());
        {
            let mut bins = preview.0.lock().unwrap();
            for index in 0..PREVIEW_BINS {
                bins.push(match index {
                    1 => 0.8,
                    n if n == PREVIEW_BINS - 1 => 0.7,
                    _ => 0.25,
                });
            }
        }
        let (stride_after_compaction, peaks) = preview.snapshot();
        assert_eq!(stride_after_compaction, stride * 2);
        assert_eq!(peaks.len(), PREVIEW_BINS / 2);
        assert_eq!(peaks.first(), Some(&0.8));
        assert_eq!(peaks.last(), Some(&0.7));
        assert_eq!(peaks.len() as u64 * stride_after_compaction, PREVIEW_BINS as u64 * stride);
    }

    #[test]
    #[ignore = "requires isolated PulseAudio, ALSA default routed to its sink, and a disposable HOME"]
    fn private_pulse_switches_inputs_and_recovers_take_after_disconnect() {
        fn pactl(args: &[&str]) -> String {
            let result = std::process::Command::new("pactl").args(args).output().unwrap();
            assert!(result.status.success(), "{:?}: {}", args, String::from_utf8_lossy(&result.stderr));
            String::from_utf8(result.stdout).unwrap()
        }

        fn wait_for(mut condition: impl FnMut() -> bool) {
            let deadline = Instant::now() + Duration::from_secs(4);
            while !condition() {
                assert!(Instant::now() < deadline, "input lifecycle step timed out");
                std::thread::sleep(Duration::from_millis(10));
            }
        }

        let server = std::env::var("PULSE_SERVER").unwrap();
        assert!(server.starts_with("unix:/tmp/rusty-capture-lifecycle."));
        let home = std::env::var("HOME").unwrap();
        assert!(home.starts_with("/tmp/rusty-capture-lifecycle."));
        let modules = pactl(&["list", "short", "modules"]);
        let alternate_module = modules.lines().find(|line| {
            line.contains("source_name=qa_alt_input")
        }).unwrap().split_whitespace().next().unwrap();
        let input_count = || {
            pactl(&["list", "short", "source-outputs"]).lines().filter(|line| {
                line.split_whitespace().nth(2).is_some_and(|client| client != "-")
            }).count()
        };
        let source_id = |index| if index % 2 == 0 { "qa_synthetic_input" } else { "qa_alt_input" };

        let mut studio = Studio::with_sample_rate(44_100);
        studio.backend = Backend::Alsa;
        studio.add_audio_track();
        let track_id = studio.selected_track.unwrap();
        let ctx = egui::Context::default();
        for index in 0..75 {
            let track = studio.project.track_mut(track_id).unwrap();
            track.record_input_id = Some(source_id(index).into());
            track.record_input_channels = [
                RecordInputChannels::Mono1, RecordInputChannels::Stereo, RecordInputChannels::Mono2,
            ][index % 3];
            let _ = ctx.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| studio.draw_hardware_controls(ui));
            });
            wait_for(|| {
                studio.poll_track_input(&ctx);
                matches!(studio.track_input_status(track_id), TrackInputStatus::Ready { .. })
            });
            assert_eq!(input_count(), 1);
        }

        let track = studio.project.track_mut(track_id).unwrap();
        track.record_input_id = Some("qa_alt_input".into());
        track.record_input_channels = RecordInputChannels::Mono2;
        wait_for(|| {
            studio.poll_track_input(&ctx);
            matches!(studio.track_input_status(track_id), TrackInputStatus::Ready { .. })
        });
        studio.start_recording();
        assert!(studio.recorder.is_some(), "{:?}", studio.error);
        assert_eq!(input_count(), 1, "recording must reuse the meter stream");
        wait_for(|| {
            studio.poll_recording_input();
            studio.recorder.as_ref().is_some_and(|recording| recording.progress_frames >= 4_410)
        });

        pactl(&["unload-module", alternate_module]);
        wait_for(|| {
            studio.poll_recording_input();
            studio.recorder.is_none()
        });
        assert!(studio.error.as_deref().unwrap().contains("stopped delivering"));
        let track = studio.project.tracks.iter().find(|track| track.id == track_id).unwrap();
        assert_eq!(track.clips.len(), 1);
        let ClipKind::Audio(audio) = &track.clips[0].kind else {
            panic!("the recoverable recording must be an audio clip")
        };
        let wav = hound::WavReader::open(&audio.source).unwrap();
        assert!(wav.duration() >= 4_410);
        assert_eq!(wav.spec().channels, 1);
        wait_for(|| input_count() == 0);

        studio.capture_devices = None;
        studio.poll_track_input(&ctx);
        assert!(matches!(studio.track_input_status(track_id), TrackInputStatus::Unavailable(_)));
        studio.project.track_mut(track_id).unwrap().record_input_id = Some("qa_synthetic_input".into());
        wait_for(|| {
            studio.poll_track_input(&ctx);
            matches!(studio.track_input_status(track_id), TrackInputStatus::Ready { .. })
        });
        assert_eq!(input_count(), 1);
        drop(studio);
        wait_for(|| input_count() == 0);
    }

    #[test]
    fn partial_suffix_is_explicit_and_failed_writer_leaves_no_take() {
        let base = std::env::temp_dir().join(format!(
            "rustie-record-partial-test-{}",
            std::process::id()
        ));
        let take = base.join("Take.wav");
        let partial = partial_path(&take);
        assert_eq!(partial.file_name().unwrap(), "Take.wav.partial");
        // A missing parent directory makes WavWriter::create fail inside the
        // worker; the returned error must surface on join and nothing may be
        // published under the final take name.
        let (_producer, consumer) = rtrb::RingBuffer::<StereoFrame>::new(8);
        let stopped = Arc::new(AtomicBool::new(false));
        let writer = spawn_wav_writer(partial.clone(), consumer, stopped, 48_000, 2, Arc::new(CapturePreview::new())).unwrap();
        assert!(writer.join().unwrap().is_err());
        assert!(!take.exists());
        assert!(!partial.exists());
    }

    #[test]
    fn writer_drains_published_tail_when_capture_stops() {
        let take = std::env::temp_dir().join(format!(
            "rustie-record-drain-test-{}.wav.partial",
            std::process::id()
        ));
        let (mut producer, consumer) = rtrb::RingBuffer::<StereoFrame>::new(16);
        let stopped = Arc::new(AtomicBool::new(false));
        let writer = spawn_wav_writer(take.clone(), consumer, Arc::clone(&stopped), 48_000, 2, Arc::new(CapturePreview::new()))
            .unwrap();
        for index in 0..128 {
            while producer.push(StereoFrame::ZERO).is_err() {
                assert!(!writer.is_finished(), "writer failed before frame {index}");
                std::thread::yield_now();
            }
        }
        drop(producer);
        stopped.store(true, Ordering::Release);
        let written = writer.join().unwrap().unwrap();
        let wav = hound::WavReader::open(&take).unwrap();
        let _ = fs::remove_file(&take);
        assert_eq!(written, 128);
        assert_eq!(wav.duration(), 128);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn full_disk_writer_exits_without_waiting_for_capture_stop() {
        let (mut producer, consumer) = rtrb::RingBuffer::<StereoFrame>::new(2048);
        for _ in 0..2048 {
            producer.push(StereoFrame::ZERO).unwrap();
        }
        let stopped = Arc::new(AtomicBool::new(false));
        let writer = spawn_wav_writer(PathBuf::from("/dev/full"), consumer, Arc::clone(&stopped), 48_000, 2, Arc::new(CapturePreview::new()))
            .unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(1);
        while !writer.is_finished() && std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(2));
        }
        let exited_while_capturing = writer.is_finished();
        stopped.store(true, Ordering::Release);
        assert!(writer.join().unwrap().is_err());
        assert!(exited_while_capturing);
    }

    #[test]
    fn wav_writer_round_trips_stereo_samples() {
        let root = std::env::temp_dir();
        let file = root.join(format!("rustie-stereo-record-test-{}-{}.wav",
            std::process::id(), std::thread::current().name().unwrap_or("writer")));
        let (mut producer, consumer) = rtrb::RingBuffer::<StereoFrame>::new(512);
        let done = Arc::new(AtomicBool::new(false));
        let writer = spawn_wav_writer(file.clone(), consumer, Arc::clone(&done), 48_000, 2, Arc::new(CapturePreview::new())).unwrap();
        for i in 0..160 {
            producer.push(StereoFrame { left: i as f32 / 320.0,
                right: -(i as f32 / 320.0) }).unwrap();
        }
        done.store(true, Ordering::Release);
        assert_eq!(writer.join().unwrap().unwrap(), 160);
        let mut source = hound::WavReader::open(&file).unwrap();
        assert_eq!(source.spec().channels, 2);
        assert_eq!(source.spec().sample_rate, 48_000);
        let samples: Vec<f32> = source.samples::<f32>().map(Result::unwrap).collect();
        assert_eq!(samples.len(), 320);
        assert!((samples[318] - 159.0 / 320.0).abs() < 1e-6);
        assert!((samples[319] + 159.0 / 320.0).abs() < 1e-6);
        let _ = fs::remove_file(file);
    }

    /// End-to-end synthetic take through the *same* bounded ring, writer,
    /// finalize/rename and WAV decoder used by microphone capture. The click
    /// must survive exactly at 2.000 s and the saved project must reopen with
    /// a real waveform and the click at the expected project frame.
    #[test]
    fn ten_second_synthetic_take_saves_reopens_and_keeps_transient_position() {
        let rate = 44_100_u32;
        let frames = u64::from(rate) * 10;
        let click_at = u64::from(rate) * 2;
        let base = std::env::temp_dir().join(format!(
            "rustie-record-take-test-{}",
            std::process::id()
        ));
        let _ = fs::create_dir_all(&base);
        let take = base.join("Take-10s.wav");
        let partial = partial_path(&take);

        let (mut producer, consumer) = rtrb::RingBuffer::<StereoFrame>::new(
            usize::try_from(frames).unwrap() + 1024,
        );
        let stopped = Arc::new(AtomicBool::new(false));
        let writer = spawn_wav_writer(partial.clone(), consumer, Arc::clone(&stopped), rate, 2, Arc::new(CapturePreview::new()))
            .unwrap();
        let mut phase = 0.0_f32;
        let step = std::f32::consts::TAU * 220.0 / rate as f32;
        for index in 0..frames {
            let sample = if index >= click_at && index < click_at + 64 {
                0.9 * if index == click_at { 1.0 } else { -1.0 }
            } else {
                0.05 * phase.sin()
            };
            phase += step;
            producer
                .push(StereoFrame {
                    left: sample,
                    right: -sample,
                })
                .unwrap();
        }
        stopped.store(true, Ordering::Release);
        assert_eq!(writer.join().unwrap().unwrap(), frames);
        finalize_take(&partial, &take).unwrap();

        // Header and sample-count evidence from the actual file.
        let mut wav = hound::WavReader::open(&take).unwrap();
        let spec = wav.spec();
        assert_eq!(spec.channels, 2);
        assert_eq!(spec.sample_rate, rate);
        assert_eq!(spec.bits_per_sample, 32);
        assert_eq!(spec.sample_format, hound::SampleFormat::Float);
        let samples = wav.samples::<f32>().collect::<Result<Vec<_>, _>>().unwrap();
        assert_eq!(samples.len() as u64, frames * 2);
        let actual_click = samples
            .as_chunks::<2>().0.iter()
            .position(|frame| frame[0].abs() > 0.5)
            .unwrap() as u64;
        assert_eq!(actual_click, click_at);

        // A take armed at 5.000 s with a 10 ms output delay and a 3 ms first
        // buffer lands at 4.993 s; the click is then at 6.993 s in the project.
        let arm_frame = u64::from(rate) * 5;
        let start_frame = aligned_start_frame(
            arm_frame,
            ns_to_frames(3_000_000, rate),
            ns_to_frames(10_000_000, rate),
        );
        let start_ticks = frames_to_ticks(start_frame, rate, 120.0);
        let mut project = Project::new("Take round-trip", Tempo::default());
        let mut track = Track::new(1, "Vocals", TrackKind::Audio);
        track.clips.push(Clip {
            id: 2,
            start: MusicalTime::from_ticks(start_ticks),
            duration: MusicalTime::from_ticks(frames_to_ticks(frames, rate, 120.0)),
            kind: ClipKind::Audio(AudioClip {
                source: take.clone(),
                warped: false,
                source_offset_frames: 0,
                gain: 1.0,
                fade_in_ms: 0,
                fade_out_ms: 0,
            }),
            scale_override: None,
        });
        project.tracks.push(track);

        // Project JSON round-trip, then decode through the reopen path.
        let bytes = serde_json::to_vec_pretty(&project).unwrap();
        let reopened: Project = serde_json::from_slice(&bytes).unwrap();
        let ClipKind::Audio(audio) = &reopened.tracks[0].clips[0].kind else {
            panic!("expected audio clip");
        };
        let Source::Samples {
            frames: decoded, ..
        } = decode_to_source(&audio.source, rate, 0).unwrap()
        else {
            panic!("decoded take is not sample data");
        };
        assert_eq!(decoded.len(), frames as usize);
        // Non-silent waveform peaks (the UI derives its waveform from these).
        let step = (decoded.len() / 1200).max(1);
        let peaks = decoded
            .chunks(step)
            .map(|block| {
                block
                    .iter()
                    .fold(0.0_f32, |peak, frame| {
                        peak.max(frame.left.abs()).max(frame.right.abs())
                    })
            })
            .collect::<Vec<_>>();
        assert!(peaks.iter().any(|peak| *peak > 0.5));
        let project_click = reopened
            .tempo_map
            .sample_frame_at(reopened.tracks[0].clips[0].start, rate)
            .unwrap()
            + click_at;
        // Tick quantisation is 1/960 of a beat (~23 frames at 120 BPM), so
        // compare within one tick rather than demanding sample-exact ticks.
        let expected_click = u64::from(rate) * 6_993_000 / 1_000_000;
        assert!(
            project_click.abs_diff(expected_click) <= 23,
            "project click {project_click} vs expected {expected_click}"
        );

        // The same reopened take must survive the production mixdown path as
        // real non-silent audio at the expected project position.
        let clip_frame = reopened
            .tempo_map
            .sample_frame_at(reopened.tracks[0].clips[0].start, rate)
            .unwrap();
        let mut builder = GraphBuilder::new(rate);
        builder.add_node(NodeSpec::audio(
            1,
            Source::TrimmedSamples {
                frames: Arc::clone(&decoded),
                start_frame: clip_frame,
                end_frame: clip_frame + frames,
                source_offset_frames: 0,
                gain: 1.0,
                fade_in_frames: 0,
                fade_out_frames: 0,
            },
        ));
        builder.add_node(NodeSpec::master(MASTER_ID));
        builder.route(1, MASTER_ID, 1.0);
        let export = base.join("export.wav");
        let export_frames = clip_frame + frames;
        let peak = render_mix_to_wav(builder.build().unwrap(), &[], export_frames, &export, rate)
            .unwrap();
        assert!(peak > 0.5, "exported take peak {peak}");
        let mut exported = hound::WavReader::open(&export).unwrap();
        assert_eq!(exported.spec().channels, 2);
        assert_eq!(exported.spec().sample_rate, rate);
        assert_eq!(exported.len() as u64, export_frames * 2);
        let exported_samples = exported
            .samples::<f32>()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        let exported_click = exported_samples
            .as_chunks::<2>().0.iter()
            .position(|frame| frame[0].abs() > 0.5)
            .unwrap() as u64;
        assert_eq!(exported_click, clip_frame + click_at);
        let _ = fs::remove_file(&export);

        let _ = fs::remove_file(&take);
        let _ = fs::remove_dir(&base);
    }

    #[test]
    fn empty_take_is_invalid_not_a_completed_clip() {
        let file = std::env::temp_dir().join(format!(
            "rustie-empty-take-test-{}.wav",
            std::process::id()
        ));
        let partial = partial_path(&file);
        let (_producer, consumer) = rtrb::RingBuffer::<StereoFrame>::new(8);
        let stopped = Arc::new(AtomicBool::new(true));
        let writer = spawn_wav_writer(partial.clone(), consumer, stopped, 44_100, 2, Arc::new(CapturePreview::new())).unwrap();
        assert_eq!(writer.join().unwrap().unwrap(), 0);
        let _ = fs::remove_file(&partial);
        assert!(!file.exists());
    }
}

/// Best-effort output latency prediction for probe reporting only. Starts a
/// short silent stream so CPAL can report callback-to-playback delay; this is
/// never used as an acoustic round-trip measurement.
fn probe_output_delay_us(rate: u32) -> u64 {
    let mut builder = GraphBuilder::new(rate);
    builder.add_node(NodeSpec::master(MASTER_ID));
    let Ok(graph) = builder.build() else {
        return 0;
    };
    let config = DriverConfig {
        sample_rate: rate,
        backend_preference: Backend::PulseAudio,
        promote_realtime: false,
        ..DriverConfig::default()
    };
    match AudioDriver::start(graph, &config) {
        Ok(driver) => {
            // The USB interface can be slow to restart its output callback
            // right after a capture stream closes; poll briefly for the
            // first real timestamp instead of reporting a false zero.
            let telemetry = driver.telemetry();
            let mut delay = 0;
            for _ in 0..60 {
                delay = telemetry.predicted_playback_delay_us.load(Ordering::Relaxed);
                if delay > 0 && telemetry.callbacks.load(Ordering::Relaxed) >= 1 {
                    break;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            delay
        }
        Err(error) => {
            eprintln!("Output delay prediction unavailable: {error}");
            0
        }
    }
}

pub fn probe_live_capture() -> Result<(), String> {
    let rate = Studio::negotiate_sample_rate();
    let sources = Studio::available_input_devices()?;
    let source = select_capture_source(None, &sources)?;
    let path = std::env::temp_dir().join(format!(
        "rustie-input-probe-{}-{}.wav", std::process::id(),
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
            .map_err(|e|e.to_string())?.as_nanos()));
    let partial = partial_path(&path);
    let result = (|| -> Result<(),String> {
        let open_at=std::time::Instant::now();
        let monitor = Arc::new(RecordingMonitor::new());
        let channels = RecordInputChannels::default().for_source(source.channels);
        let mut input = CaptureInput::open(source, channels, rate, Arc::clone(&monitor))?;
        input.wait_ready()?;
        let parts = input.prepare_take(&path, rate, channels)?;
        let armed_ns = arm_capture(&parts.clock);
        let open_ms=open_at.elapsed().as_millis();
        let started_at=std::time::Instant::now();
        std::thread::sleep(Duration::from_millis(350));
        drop(parts.stream); // stop hardware callback before draining and finalizing WAV
        let capture_elapsed_ms=started_at.elapsed().as_millis();
        parts.stopped.store(true,Ordering::Release);
        let frames=parts.writer.join().map_err(|_|"WAV writer panicked".to_string())??;
        finalize_take(&partial, &path)?;
        let wav=hound::WavReader::open(&path).map_err(|e|e.to_string())?;
        if frames<rate as u64 / 10 || wav.spec().sample_rate!=rate {
            return Err(format!("Short or incorrectly clocked recording: {frames} frames at {rate}Hz, last backend-reported input age {:.0}ms", parts.clock.input_age_ns.load(Ordering::Relaxed) as f64/1_000_000.0));
        }
        let first_capture = parts.clock.first_capture.load(Ordering::Acquire);
        let first_buffer_frames = if first_capture == u64::MAX {
            0
        } else {
            ns_to_frames(first_capture.saturating_sub(armed_ns), rate)
        };
        let output_delay_us = probe_output_delay_us(rate);
        let output_delay_frames = ns_to_frames(output_delay_us.saturating_mul(1_000), rate);
        println!("Live capture probe PASSED: source={}, rate={}Hz, frames={}, dropped={}, monitor_frames={}, opened={}ms, captured={}ms, wav_duration={:.1}ms, first_buffer={:.2}ms, output_delay_prediction={:.2}ms, net_alignment_offset={:+.2}ms, wav_channels={}, wav_bits={} (temporary WAV removed)",
            source.label,rate,frames,parts.dropped.load(Ordering::Relaxed),monitor.next_index(),
            open_ms,capture_elapsed_ms,frames as f64*1000.0/rate as f64,
            first_buffer_frames as f64*1000.0/rate as f64,
            output_delay_frames as f64*1000.0/rate as f64,
            (first_buffer_frames as i64 - output_delay_frames as i64) as f64*1000.0/rate as f64,
            wav.spec().channels,wav.spec().bits_per_sample);
        Ok(())
    })();
    let _=fs::remove_file(&path); // discard captured audio on success or failure
    let _=fs::remove_file(&partial);
    result
}

/// Device-free reproduction of the exact take pipeline: ring buffer, WAV
/// writer, finalize/rename and header validation. A 220 Hz bed plus a click
/// at exactly 2.000 s lets the probe report expected versus actual transient
/// position. This is synthetic evidence, not a microphone or loopback test.
pub fn probe_synthetic_recording() -> Result<(), String> {
    let rate = Studio::negotiate_sample_rate();
    let frames = rate as u64 * 10;
    let transient_at = rate as u64 * 2;
    let path = std::env::temp_dir().join(format!(
        "rustie-synthetic-recording-probe-{}-{}.wav", std::process::id(),
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
            .map_err(|e|e.to_string())?.as_nanos()));
    let partial = partial_path(&path);
    let result = (|| -> Result<(), String> {
        let (mut producer, consumer) = rtrb::RingBuffer::<StereoFrame>::new(
            usize::try_from(frames).unwrap_or(usize::MAX).saturating_add(1024));
        let stopped = Arc::new(AtomicBool::new(false));
        let writer = spawn_wav_writer(partial.clone(), consumer, Arc::clone(&stopped), rate, 2, Arc::new(CapturePreview::new()))?;
        let mut phase = 0.0_f32;
        let step = std::f32::consts::TAU * 220.0 / rate as f32;
        for index in 0..frames {
            let click = index >= transient_at && index < transient_at + 64;
            let sample = if click {
                0.9 * if index == transient_at { 1.0 } else { -1.0 }
            } else {
                0.05 * phase.sin()
            };
            phase += step;
            producer
                .push(StereoFrame { left: sample, right: -sample })
                .map_err(|_| "Synthetic capture ring unexpectedly full".to_string())?;
        }
        stopped.store(true, Ordering::Release);
        let written = writer.join().map_err(|_| "WAV writer panicked".to_string())??;
        finalize_take(&partial, &path)?;
        let mut wav = hound::WavReader::open(&path).map_err(|e| e.to_string())?;
        let spec = wav.spec();
        let samples = wav.samples::<f32>().collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        if written != frames || samples.len() as u64 != frames * 2 {
            return Err(format!(
                "Synthetic take length mismatch: written={written}, samples={}, expected_frames={frames}",
                samples.len()));
        }
        let actual = samples
            .as_chunks::<2>().0.iter()
            .position(|frame| frame[0].abs() > 0.5)
            .map(|index| index as u64)
            .ok_or("Synthetic click was not found in the WAV")?;
        if actual != transient_at {
            return Err(format!("Synthetic transient found at frame {actual}, expected {transient_at}"));
        }
        if spec.channels != 2 || spec.sample_rate != rate || spec.bits_per_sample != 32
            || spec.sample_format != hound::SampleFormat::Float {
            return Err(format!("Synthetic WAV header wrong: {spec:?}"));
        }
        println!(
            "Synthetic 10s recording probe PASSED: rate={rate}Hz, frames={written}, duration={:.3}s, channels={}, bits={}, transient_expected_frame={transient_at}, transient_actual_frame={actual}, delta_frames={}, peak_window={:.2}",
            written as f64 / rate as f64,
            spec.channels,
            spec.bits_per_sample,
            actual as i64 - transient_at as i64,
            samples.iter().fold(0.0_f32, |peak, sample| peak.max(sample.abs())),
        );
        Ok(())
    })();
    let _ = fs::remove_file(&path);
    let _ = fs::remove_file(&partial);
    result
}
