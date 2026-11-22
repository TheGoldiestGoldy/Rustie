//! Native CLAP format offline effect worker. Third-party .clap code runs
//! only in daw-plugin-host's child process (not the DAW UI or audio callback).
use std::path::Path;

use clack_host::events::event_types::{NoteOffEvent, NoteOnEvent};
use clack_host::factory::plugin::PluginFactory;
use clack_host::prelude::*;

use crate::{BLOCK, MAX_RENDER_FRAMES, RATE};

fn load_entry(path: &Path) -> Result<PluginEntry, String> {
    // SAFETY: arbitrary plugin code is loaded only inside the independently
    // supervised, timeout-bound worker process. This does not make the binary
    // intrinsically safe; malformed plugins can still crash this child process.
    unsafe { PluginEntry::load(path.as_os_str()) }
        .map_err(|e| format!("Load CLAP bundle {}: {e}", path.display()))
}

pub fn scan(path: &Path) -> Result<(), String> {
    let entry = load_entry(path)?;
    let factory = entry
        .get_factory::<PluginFactory>()
        .ok_or("CLAP bundle does not expose a plugin factory")?;
    let mut found = 0;
    for descriptor in factory.plugin_descriptors() {
        let id = descriptor
            .id()
            .map_or("<invalid>".into(), |v| v.to_string_lossy().into_owned());
        let name = descriptor
            .name()
            .map_or("<unnamed>".into(), |v| v.to_string_lossy().into_owned());
        println!("{id}\t{name}");
        found += 1;
    }
    if found == 0 {
        return Err("No CLAP plugin classes in bundle".into());
    }
    Ok(())
}

/// Machine-readable class metadata for explicit class selection and an honest
/// compatibility matrix. Installed binaries remain test inputs, never bundled.
pub fn inspect(path: &Path) -> Result<(), String> {
    let entry = load_entry(path)?;
    let factory = entry
        .get_factory::<PluginFactory>()
        .ok_or("CLAP bundle does not expose a plugin factory")?;
    let mut found = 0;
    for descriptor in factory.plugin_descriptors() {
        let field = |value: Option<&std::ffi::CStr>| {
            value
                .map(|v| v.to_string_lossy().into_owned())
                .unwrap_or_default()
        };
        let id = field(descriptor.id());
        let name = field(descriptor.name());
        let vendor = field(descriptor.vendor());
        let version = field(descriptor.version());
        let features = descriptor
            .features()
            .map(|feature| feature.to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join(",");
        println!(
            "format=clap\tid={id}\tname={name}\tvendor={vendor}\tversion={version}\tfeatures={features}"
        );
        found += 1;
    }
    if found == 0 {
        return Err("No CLAP plugin classes in bundle".into());
    }
    Ok(())
}

pub fn render_effect(
    plugin_path: &Path,
    input: &Path,
    out: &Path,
    plugin_id: Option<&str>,
) -> Result<(), String> {
    let source = daw_engine::decode_to_source(input, RATE, 0)
        .map_err(|e| format!("Decode CLAP input: {e}"))?;
    let daw_engine::Source::Samples { frames, .. } = source else {
        return Err("CLAP requires a decoded stereo WAV input".into());
    };
    if frames.is_empty() || frames.len() as u64 > MAX_RENDER_FRAMES {
        return Err("CLAP render is limited to ten minutes".into());
    }
    let entry = load_entry(plugin_path)?;
    let factory = entry
        .get_factory::<PluginFactory>()
        .ok_or("CLAP bundle lacks a plugin factory")?;
    let descriptor = factory
        .plugin_descriptors()
        .find(|descriptor| {
            plugin_id.is_none_or(|name| {
                descriptor
                    .id()
                    .is_some_and(|id| id.to_bytes() == name.as_bytes())
            })
        })
        .ok_or_else(|| {
            format!(
                "CLAP plugin ID not found: {}",
                plugin_id.unwrap_or("(first)")
            )
        })?;
    let name = descriptor
        .name()
        .map_or("<unnamed>".into(), |n| n.to_string_lossy().into_owned());
    let plugin_id = descriptor.id().ok_or("CLAP class has no ID")?;
    let info = HostInfo::new(
        "Slate Studio",
        "Rustie",
        "https://github.com/TheGoldiestGoldy/Rustie",
        "0.1",
    )
    .map_err(|e| format!("Host info: {e}"))?;
    let mut plugin = PluginInstance::<()>::new(|_| (), |_| (), &entry, plugin_id, &info)
        .map_err(|e| format!("Create CLAP class {name}: {e}"))?;
    let config = PluginAudioConfiguration {
        sample_rate: RATE as f64,
        min_frames_count: BLOCK as u32,
        max_frames_count: BLOCK as u32,
    };
    let processor = plugin
        .activate(|_, _| (), config)
        .map_err(|e| format!("Activate CLAP: {e}"))?;
    let mut processor = processor
        .start_processing()
        .map_err(|e| format!("Start CLAP: {e}"))?;
    let mut in_ports = AudioPorts::with_capacity(2, 1);
    let mut out_ports = AudioPorts::with_capacity(2, 1);
    let mut input_buf = [[0.0_f32; BLOCK]; 2];
    let mut output_buf = [[0.0_f32; BLOCK]; 2];
    let in_events = EventBuffer::new();
    let mut out_events = EventBuffer::new();
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: RATE,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut wav =
        hound::WavWriter::create(out, spec).map_err(|e| format!("Create CLAP render WAV: {e}"))?;
    let mut peak = 0.0_f32;
    let total = frames.len().saturating_add(RATE as usize); // 1s effect tail
    let mut cursor = 0;
    while cursor < total {
        for (i, sample) in input_buf[0].iter_mut().enumerate() {
            *sample = frames.get(cursor + i).map_or(0.0, |frame| frame.left);
        }
        for (i, sample) in input_buf[1].iter_mut().enumerate() {
            *sample = frames.get(cursor + i).map_or(0.0, |frame| frame.right);
        }
        output_buf[0].fill(0.0);
        output_buf[1].fill(0.0);
        out_events.clear();
        {
            let ins = in_ports.with_input_buffers([AudioPortBuffer {
                latency: 0,
                channels: AudioPortBufferType::f32_input_only(
                    input_buf.iter_mut().map(InputChannel::variable),
                ),
            }]);
            let mut outs = out_ports.with_output_buffers([AudioPortBuffer {
                latency: 0,
                channels: AudioPortBufferType::f32_output_only(
                    output_buf.iter_mut().map(|b| b.as_mut_slice()),
                ),
            }]);
            processor
                .process(
                    &ins,
                    &mut outs,
                    &InputEvents::from_buffer(&in_events),
                    &mut OutputEvents::from_buffer(&mut out_events),
                    None,
                    None,
                )
                .map_err(|e| format!("CLAP audio block {}: {e}", cursor / BLOCK))?;
        }
        let n = BLOCK.min(total - cursor);
        for i in 0..n {
            let left = output_buf[0][i];
            let right = output_buf[1][i];
            if !left.is_finite() || !right.is_finite() {
                return Err("CLAP output contains nonfinite samples".into());
            }
            peak = peak.max(left.abs()).max(right.abs());
            wav.write_sample(left.clamp(-1.0, 1.0))
                .map_err(|e| e.to_string())?;
            wav.write_sample(right.clamp(-1.0, 1.0))
                .map_err(|e| e.to_string())?;
        }
        cursor += n;
    }
    let stopped = processor.stop_processing();
    plugin.deactivate(stopped);
    wav.finalize().map_err(|e| e.to_string())?;
    println!("CLAP {name}: {total} samples @ {RATE} Hz, peak {peak:.5}");
    if peak < 0.000_001 {
        return Err("CLAP processed silence; file not imported into Rustie".into());
    }
    Ok(())
}
// Offline CLAP instrument render; events are sample-offset within each
// processing block. Plugin instance remains isolated in the worker process.
#[derive(Debug, serde::Deserialize)]
struct MidiNoteEvent {
    frame: u64,
    note: u8,
    velocity: u8,
    on: bool,
}
#[derive(Debug, serde::Deserialize)]
struct MidiRequest {
    length_frames: u64,
    bpm: f64,
    events: Vec<MidiNoteEvent>,
}
pub fn render_midi(
    plugin_path: &Path,
    request_path: &Path,
    out: &Path,
    plugin_id: Option<&str>,
) -> Result<(), String> {
    let raw = std::fs::read(request_path).map_err(|e| e.to_string())?;
    let mut request: MidiRequest =
        serde_json::from_slice(&raw).map_err(|e| format!("CLAP MIDI request: {e}"))?;
    if request.length_frames == 0
        || request.length_frames > MAX_RENDER_FRAMES
        || request.events.len() > 50_000
        || !(20.0..=300.0).contains(&request.bpm)
    {
        return Err("Invalid MIDI length, note count or tempo for CLAP".into());
    }
    request.events.sort_by_key(|e| (e.frame, e.on));
    let entry = load_entry(plugin_path)?;
    let factory = entry
        .get_factory::<PluginFactory>()
        .ok_or("No CLAP plugin factory")?;
    let descriptor = factory
        .plugin_descriptors()
        .find(|descriptor| {
            plugin_id.is_none_or(|name| {
                descriptor
                    .id()
                    .is_some_and(|id| id.to_bytes() == name.as_bytes())
            })
        })
        .ok_or_else(|| format!("No CLAP class: {}", plugin_id.unwrap_or("(first)")))?;
    let id = descriptor.id().ok_or("CLAP class has no ID")?;
    let name = descriptor.name().map_or("<unnamed>".into(), |name| {
        name.to_string_lossy().into_owned()
    });
    let info = HostInfo::new(
        "Slate Studio",
        "Rustie",
        "https://github.com/TheGoldiestGoldy/Rustie",
        "0.1",
    )
    .map_err(|e| e.to_string())?;
    let mut plugin = PluginInstance::<()>::new(|_| (), |_| (), &entry, id, &info)
        .map_err(|e| format!("CLAP instrument instantiate: {e}"))?;
    let config = PluginAudioConfiguration {
        sample_rate: RATE as f64,
        min_frames_count: BLOCK as u32,
        max_frames_count: BLOCK as u32,
    };
    let processor = plugin
        .activate(|_, _| (), config)
        .map_err(|e| format!("Activate CLAP instrument: {e}"))?;
    let mut processor = processor
        .start_processing()
        .map_err(|e| format!("Start CLAP instrument: {e}"))?;
    let mut in_ports = AudioPorts::with_capacity(0, 0);
    let mut out_ports = AudioPorts::with_capacity(2, 1);
    let mut output_buf = [[0.0_f32; BLOCK]; 2];
    let mut in_events = EventBuffer::with_capacity(128);
    let mut out_events = EventBuffer::new();
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: RATE,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut wav =
        hound::WavWriter::create(out, spec).map_err(|e| format!("Create CLAP MIDI WAV: {e}"))?;
    let total = request.length_frames.saturating_add(RATE as u64);
    let mut next = 0;
    let mut frame = 0;
    let mut peak = 0.0_f32;
    while frame < total {
        in_events.clear();
        out_events.clear();
        while let Some(event) = request.events.get(next) {
            if event.frame >= frame + BLOCK as u64 {
                break;
            }
            next += 1;
            if event.frame < frame || event.note > 127 || event.velocity > 127 {
                continue;
            }
            let at = (event.frame - frame) as u32;
            let target = Pckn::new(0u16, 0u16, event.note as u16, 0u32);
            if event.on {
                in_events.push(&NoteOnEvent::new(at, target, event.velocity as f64 / 127.0));
            } else {
                in_events.push(&NoteOffEvent::new(at, target, 0.0));
            }
        }
        output_buf[0].fill(0.0);
        output_buf[1].fill(0.0);
        {
            let ins = in_ports.with_input_buffers(std::iter::empty::<
                AudioPortBuffer<
                    std::iter::Empty<InputChannel<'_, f32>>,
                    std::iter::Empty<InputChannel<'_, f64>>,
                >,
            >());
            let mut outs = out_ports.with_output_buffers([AudioPortBuffer {
                latency: 0,
                channels: AudioPortBufferType::f32_output_only(
                    output_buf.iter_mut().map(|buf| buf.as_mut_slice()),
                ),
            }]);
            processor
                .process(
                    &ins,
                    &mut outs,
                    &InputEvents::from_buffer(&in_events),
                    &mut OutputEvents::from_buffer(&mut out_events),
                    None,
                    None,
                )
                .map_err(|e| format!("CLAP MIDI audio block: {e}"))?;
        }
        let n = (total - frame).min(BLOCK as u64) as usize;
        for i in 0..n {
            let left = output_buf[0][i];
            let right = output_buf[1][i];
            if !left.is_finite() || !right.is_finite() {
                return Err("CLAP instrument output contains nonfinite values".into());
            }
            peak = peak.max(left.abs()).max(right.abs());
            wav.write_sample(left.clamp(-1.0, 1.0))
                .map_err(|e| e.to_string())?;
            wav.write_sample(right.clamp(-1.0, 1.0))
                .map_err(|e| e.to_string())?;
        }
        frame += n as u64;
    }
    let stopped = processor.stop_processing();
    plugin.deactivate(stopped);
    wav.finalize().map_err(|e| e.to_string())?;
    println!("CLAP MIDI {name}: {total} frames @ {RATE} Hz, peak {peak:.5}");
    if peak < 0.000001 {
        return Err(
            "CLAP instrument produced silence: check patch/preset/note compatibility".into(),
        );
    }
    Ok(())
}
