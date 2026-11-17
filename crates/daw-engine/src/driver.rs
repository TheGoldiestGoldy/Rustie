//! CPAL native Linux audio layer: PulseAudio, JACK via PipeWire, and ALSA.
//! Default preference is PulseAudio on hosts where it owns physical playback.
//!
//! The render callback does not allocate, lock, log, call D-Bus, or touch files.
//! Device enumeration, negotiation, and stream creation happen off-thread.

use std::{
    fmt,
    sync::{
        Arc,
        atomic::{AtomicI32, Ordering},
    },
    thread,
    time::Duration,
};

use cpal::{
    Device, SampleFormat, Stream, StreamConfig, SupportedBufferSize,
    traits::{DeviceTrait, HostTrait, StreamTrait},
};

use crate::{
    engine::{Controller, create_engine},
    graph::AudioGraph,
    realtime::{self, PriorityStatus},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    Jack,
    Alsa,
    PulseAudio,
}

#[derive(Debug, Clone)]
pub struct DriverConfig {
    pub backend_preference: Backend,
    pub device_name_contains: Option<String>,
    pub sample_rate: u32,
    /// JACK's quantum is server-controlled and cannot be overridden by CPAL.
    pub buffer_frames: u32,
    /// Ask for real-time scheduling for ALSA callback via RLIMIT_RTPRIO or RTKit.
    pub promote_realtime: bool,
}

impl Default for DriverConfig {
    fn default() -> Self {
        Self {
            backend_preference: Backend::Alsa,
            device_name_contains: None,
            sample_rate: 48_000,
            buffer_frames: 128,
            promote_realtime: true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct DriverInfo {
    pub backend: String,
    pub device: String,
    pub sample_rate: u32,
    pub channels: u16,
    /// None = server/driver selects quantum.
    pub requested_fixed_buffer_frames: Option<u32>,
    pub rt_scheduling: PriorityStatus,
}

#[derive(Debug)]
pub enum DriverError {
    InvalidConfig(String),
    NoDevice(String),
    UnsupportedConfig(String),
    Cpal(String),
}

impl fmt::Display for DriverError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidConfig(s)
            | Self::NoDevice(s)
            | Self::UnsupportedConfig(s)
            | Self::Cpal(s) => f.write_str(s),
        }
    }
}
impl std::error::Error for DriverError {}

pub struct AudioDriver {
    // Dropped on the owning control thread when AudioDriver is dropped.
    _stream: Stream,
    controller: Controller,
    info: DriverInfo,
}

impl AudioDriver {
    pub fn start(graph: AudioGraph, desired: &DriverConfig) -> Result<Self, DriverError> {
        if ![44_100, 48_000, 96_000].contains(&desired.sample_rate) {
            return Err(DriverError::InvalidConfig(
                "sample rate must be 44100, 48000, or 96000".into(),
            ));
        }
        if !(64..=2048).contains(&desired.buffer_frames) {
            return Err(DriverError::InvalidConfig(
                "buffer frames must be 64..=2048".into(),
            ));
        }
        if graph.sample_rate() != desired.sample_rate {
            return Err(DriverError::InvalidConfig(
                "graph sample rate must match device sample rate".into(),
            ));
        }

        // Backend fallback for supported-device discovery. The graph is consumed
        // when building a callback, so stream *startup* failures are returned.
        let order = match desired.backend_preference {
            Backend::Jack => [Backend::Jack, Backend::PulseAudio, Backend::Alsa],
            Backend::Alsa => [Backend::Alsa, Backend::PulseAudio, Backend::Jack],
            Backend::PulseAudio => [Backend::PulseAudio, Backend::Alsa, Backend::Jack],
        };
        let mut selected = None;
        let mut last_error = String::new();
        for backend in order {
            match select_device(backend, desired) {
                Ok(value) => {
                    selected = Some(value);
                    break;
                }
                Err(error) => last_error = error.to_string(),
            }
        }
        let (device, stream_config, mut info) = selected.ok_or_else(|| {
            DriverError::NoDevice(format!("no supported audio output: {last_error}"))
        })?;
        let channels = usize::from(info.channels);
        let (controller, mut processor) = create_engine(graph);
        let audio_telemetry = Arc::clone(controller.telemetry());
        let callback_tid = Arc::new(AtomicI32::new(0));
        let tid_in_callback = Arc::clone(&callback_tid);
        // Error handler runs outside the realtime render body.
        let stream = device
            .build_output_stream(
                stream_config,
                move |data: &mut [f32], info| {
                    let timestamp = info.timestamp();
                    let predicted = timestamp
                        .playback
                        .saturating_duration_since(timestamp.callback)
                        .as_micros()
                        .min(2_000_000) as u64;
                    audio_telemetry
                        .predicted_playback_delay_us
                        .store(predicted, Ordering::Relaxed);
                    // One bounded, nonblocking Linux gettid call per stream lifetime.
                    if tid_in_callback.load(Ordering::Relaxed) == 0 {
                        tid_in_callback.store(realtime::current_kernel_tid(), Ordering::Release);
                    }
                    processor.process_interleaved(data, channels);
                },
                move |err| eprintln!("Audio stream error: {err}"),
                None,
            )
            .map_err(|error| DriverError::Cpal(format!("build stream: {error}")))?;
        stream
            .play()
            .map_err(|error| DriverError::Cpal(format!("play stream: {error}")))?;
        // Control-thread operation only. Never performs D-Bus or priority syscalls
        // from the real-time audio callback.
        let mut tid = 0;
        for _ in 0..30 {
            tid = callback_tid.load(Ordering::Acquire);
            if tid > 0 {
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }
        info.rt_scheduling = if info.backend == "Jack" {
            // PipeWire/JACK manages its own real-time callback policy.
            realtime::query_thread_scheduler(tid)
        } else if desired.promote_realtime {
            realtime::request_realtime_for_tid(tid, realtime::DEFAULT_RT_PRIORITY)
        } else {
            realtime::query_thread_scheduler(tid)
        };
        Ok(Self {
            _stream: stream,
            controller,
            info,
        })
    }

    pub fn controller(&mut self) -> &mut Controller {
        &mut self.controller
    }

    pub fn info(&self) -> &DriverInfo {
        &self.info
    }

    pub fn telemetry(&self) -> &std::sync::Arc<crate::engine::EngineTelemetry> {
        self.controller.telemetry()
    }

    pub fn stop(self) {}
}

fn select_device(
    backend: Backend,
    desired: &DriverConfig,
) -> Result<(Device, StreamConfig, DriverInfo), DriverError> {
    let host_id = match backend {
        Backend::Jack => cpal::HostId::Jack,
        Backend::Alsa => cpal::HostId::Alsa,
        Backend::PulseAudio => cpal::HostId::PulseAudio,
    };
    if !cpal::available_hosts().contains(&host_id) {
        return Err(DriverError::NoDevice(format!(
            "{backend:?} backend is not available"
        )));
    }
    let host = cpal::host_from_id(host_id)
        .map_err(|e| DriverError::NoDevice(format!("{backend:?}: {e}")))?;
    let mut devices = Vec::new();
    // Default output first; then any others.
    if let Some(default) = host.default_output_device() {
        devices.push(default);
    }
    devices.extend(
        host.output_devices()
            .map_err(|e| DriverError::Cpal(format!("enumerate devices: {e}")))?,
    );

    let mut last_error = format!(
        "{backend:?}: no matching f32 output at {} Hz",
        desired.sample_rate
    );
    for device in devices {
        let name = device.to_string();
        if let Some(filter) = &desired.device_name_contains
            && !name.to_lowercase().contains(&filter.to_lowercase())
        {
            continue;
        }
        let ranges = match device.supported_output_configs() {
            Ok(ranges) => ranges,
            Err(e) => {
                last_error = format!("{backend:?}/{name}: {e}");
                continue;
            }
        };
        let mut compatible = ranges
            .filter(|config| {
                config.sample_format() == SampleFormat::F32
                    && config.min_sample_rate() <= desired.sample_rate
                    && config.max_sample_rate() >= desired.sample_rate
            })
            .collect::<Vec<_>>();
        // Prefer stereo. Multi-channel works too (channels > 2 are rendered silent).
        compatible.sort_by_key(|config| {
            let count = config.channels();
            if count == 2 { 0 } else { 1 + count.abs_diff(2) }
        });
        let Some(selected) = compatible.into_iter().next() else {
            continue;
        };
        let fixed = match (backend, selected.buffer_size()) {
            // JACK quantum is server controlled; PulseAudio can honor a fixed period.
            // Using Pulse's default buffer silently requests very large prebuffering
            // and produced ~2 seconds of audible delay on the originally tested Linux host.
            (Backend::Jack, _) => None,
            (_, SupportedBufferSize::Range { min, max })
                if desired.buffer_frames >= *min && desired.buffer_frames <= *max =>
            {
                Some(desired.buffer_frames)
            }
            _ => None,
        };
        let mut config = selected.with_sample_rate(desired.sample_rate).config();
        if let Some(frames) = fixed {
            config.buffer_size = cpal::BufferSize::Fixed(frames);
        }
        let info = DriverInfo {
            backend: format!("{backend:?}"),
            device: name,
            sample_rate: config.sample_rate,
            channels: config.channels,
            requested_fixed_buffer_frames: fixed,
            rt_scheduling: PriorityStatus::Unavailable,
        };
        return Ok((device, config, info));
    }
    Err(DriverError::UnsupportedConfig(last_error))
}

pub fn enumerate_device_names() -> Vec<(String, String)> {
    let mut outputs = Vec::new();
    for backend in [Backend::Jack, Backend::PulseAudio, Backend::Alsa] {
        let host_id = match backend {
            Backend::Jack => cpal::HostId::Jack,
            Backend::Alsa => cpal::HostId::Alsa,
            Backend::PulseAudio => cpal::HostId::PulseAudio,
        };
        if let Ok(host) = cpal::host_from_id(host_id)
            && let Ok(devices) = host.output_devices()
        {
            for device in devices {
                outputs.push((format!("{backend:?}"), device.to_string()));
            }
        }
    }
    outputs
}
