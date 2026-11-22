//! Legacy VST 2.4 offline render compatibility. Plugins load only INSIDE
//! daw-plugin-host, a separate subprocess, never the GUI or real-time engine.
//! The host library is an independently maintained MIT-licensed VST2 ABI.
use std::{
    fs,
    path::Path,
    sync::{Arc, Mutex},
};
use vst::{
    buffer::SendEventBuffer,
    event::MidiEvent,
    host::{Host, HostBuffer, PluginLoader},
    plugin::Plugin,
};

use crate::{BLOCK, MAX_RENDER_FRAMES, MidiRenderRequest, RATE};

struct RustieLegacyHost;
impl Host for RustieLegacyHost {}

fn new_vst2(path: &Path) -> Result<vst::host::PluginInstance, String> {
    if !path.exists() {
        return Err(format!("Missing legacy VST library: {}", path.display()));
    }
    let host = Arc::new(Mutex::new(RustieLegacyHost));
    let mut loader =
        PluginLoader::load(path, host).map_err(|e| format!("Load VST2 library: {e}"))?;
    let mut plugin = loader
        .instance()
        .map_err(|e| format!("Create VST2 plugin: {e}"))?;
    let info = plugin.get_info();
    if info.outputs < 1 || info.outputs > 32 || info.inputs < 0 || info.inputs > 32 {
        return Err(format!(
            "Unsupported legacy VST inputs/outputs for '{}': {} in / {} out",
            info.name, info.inputs, info.outputs
        ));
    }
    plugin.init();
    plugin.set_sample_rate(RATE as f32);
    plugin.set_block_size(BLOCK as i64);
    plugin.resume();
    Ok(plugin)
}

fn write_vst2_buffer<W: std::io::Write + std::io::Seek>(
    wav: &mut hound::WavWriter<W>,
    outputs: &[Vec<f32>],
    n: usize,
) -> Result<f32, String> {
    let mut peak = 0_f32;
    for i in 0..n {
        let l = outputs
            .first()
            .and_then(|b| b.get(i))
            .copied()
            .unwrap_or_default();
        let r = outputs.get(1).and_then(|b| b.get(i)).copied().unwrap_or(l);
        peak = peak.max(l.abs()).max(r.abs());
        wav.write_sample(l.clamp(-1., 1.))
            .map_err(|e| e.to_string())?;
        wav.write_sample(r.clamp(-1., 1.))
            .map_err(|e| e.to_string())?;
    }
    Ok(peak)
}

pub(crate) fn render_effect(plugin_path: &Path, input: &Path, out: &Path) -> Result<(), String> {
    let daw_engine::Source::Samples { frames, .. } =
        daw_engine::decode_to_source(input, RATE, 0)
            .map_err(|e| format!("Decode effect input: {e}"))?
    else {
        return Err("Unsupported effect audio source".into());
    };
    if frames.is_empty() || frames.len() as u64 > MAX_RENDER_FRAMES {
        return Err("Effect input must be 1 sample to 10 minutes".into());
    }
    let mut plugin = new_vst2(plugin_path)?;
    let info = plugin.get_info();
    let count_in = info.inputs as usize;
    let count_out = info.outputs as usize;
    let mut host_buffer = HostBuffer::<f32>::new(count_in, count_out);
    let mut inputs = vec![vec![0_f32; BLOCK]; count_in];
    let mut outputs = vec![vec![0_f32; BLOCK]; count_out];
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: RATE,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut wav = hound::WavWriter::create(out, spec).map_err(|e| e.to_string())?;
    let mut peak = 0_f32;
    let mut frame = 0_usize;
    while frame < frames.len() {
        let n = (frames.len() - frame).min(BLOCK);
        for ch in &mut inputs {
            ch.fill(0.0);
        }
        for ch in &mut outputs {
            ch.fill(0.0);
        }
        for i in 0..n {
            if count_in > 0 {
                inputs[0][i] = frames[frame + i].left;
            }
            if count_in > 1 {
                inputs[1][i] = frames[frame + i].right;
            }
        }
        plugin.process(&mut host_buffer.bind(&inputs, &mut outputs));
        peak = peak.max(write_vst2_buffer(&mut wav, &outputs, n)?);
        frame += n;
    }
    plugin.suspend();
    wav.finalize().map_err(|e| e.to_string())?;
    println!("VST2 effect rendered {frame} frames: peak {peak:.5}");
    if peak < 0.000_001 {
        return Err("Legacy VST output silent. Shell plugins may require a specific class".into());
    }
    Ok(())
}

pub(crate) fn render_midi(
    plugin_path: &Path,
    request_path: &Path,
    out: &Path,
) -> Result<(), String> {
    let bytes = fs::read(request_path).map_err(|e| e.to_string())?;
    let mut request: MidiRenderRequest =
        serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    if request.length_frames == 0
        || request.length_frames > MAX_RENDER_FRAMES
        || request.events.len() > 50_000
    {
        return Err("Invalid MIDI render length or event count".into());
    }
    request.events.sort_by_key(|e| (e.frame, e.on));
    let mut plugin = new_vst2(plugin_path)?;
    let info = plugin.get_info();
    let mut host_buffer = HostBuffer::<f32>::new(info.inputs as usize, info.outputs as usize);
    let inputs = vec![vec![0_f32; BLOCK]; info.inputs as usize];
    let mut outputs = vec![vec![0_f32; BLOCK]; info.outputs as usize];
    let mut send = SendEventBuffer::new(1024);
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: RATE,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut wav = hound::WavWriter::create(out, spec).map_err(|e| e.to_string())?;
    let end = request.length_frames + RATE as u64;
    let mut position = 0_u64;
    let mut next = 0_usize;
    let mut peak = 0_f32;
    while position < end {
        for ch in &mut outputs {
            ch.fill(0.0);
        }
        let mut midi = Vec::<MidiEvent>::new();
        while let Some(event) = request.events.get(next) {
            if event.frame >= position + BLOCK as u64 {
                break;
            }
            next += 1;
            if event.frame < position {
                continue;
            }
            midi.push(MidiEvent {
                data: [
                    if event.on { 0x90 } else { 0x80 },
                    event.note,
                    if event.on { event.velocity } else { 0 },
                ],
                delta_frames: (event.frame - position) as i32,
                live: false,
                note_length: None,
                note_offset: None,
                detune: 0,
                note_off_velocity: 0,
            });
        }
        if !midi.is_empty() {
            send.store_events(&midi);
            plugin.process_events(send.events());
        }
        plugin.process(&mut host_buffer.bind(&inputs, &mut outputs));
        let count = (end - position).min(BLOCK as u64) as usize;
        peak = peak.max(write_vst2_buffer(&mut wav, &outputs, count)?);
        position += count as u64;
    }
    plugin.suspend();
    wav.finalize().map_err(|e| e.to_string())?;
    println!("VST2 MIDI rendered {end} frames: peak {peak:.5}");
    if peak < 0.000_001 {
        return Err(
            "Legacy VST produced silence. It may be an effect rather than a MIDI instrument".into(),
        );
    }
    Ok(())
}
