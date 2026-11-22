//! Offline native VST2/VST3 rendering worker. The entire plugin runs OUTSIDE the GUI/audio
//! engine process: a buggy plugin may abort this worker, but not Rustie.
//! This is deliberately not advertised as live in-chain VST3 hosting.
use serde::Deserialize;
use std::{fs, path::Path, process::ExitCode};
use vst3_host::{
    Vst3Host,
    audio::AudioBuffers,
    midi::{MidiChannel, MidiEvent},
};

mod clap_render;
mod clap_stream;
mod vst2;
mod vst3_stream;

const RATE: u32 = 48_000;
const BLOCK: usize = 512;
const MAX_RENDER_FRAMES: u64 = 48_000 * 60 * 10;

#[derive(Debug, Clone, Copy, Deserialize)]
struct MidiRenderEvent {
    frame: u64,
    note: u8,
    velocity: u8,
    on: bool,
}

#[derive(Debug, Deserialize)]
struct MidiRenderRequest {
    length_frames: u64,
    bpm: f64,
    events: Vec<MidiRenderEvent>,
}

fn write_stereo<W: std::io::Write + std::io::Seek>(
    wav: &mut hound::WavWriter<W>,
    buffers: &AudioBuffers,
    n: usize,
) -> Result<f32, String> {
    let mut peak = 0.0_f32;
    for i in 0..n {
        let left = buffers
            .outputs
            .first()
            .and_then(|c| c.get(i))
            .copied()
            .unwrap_or(0.0);
        let right = buffers
            .outputs
            .get(1)
            .and_then(|c| c.get(i))
            .copied()
            .unwrap_or(left);
        peak = peak.max(left.abs()).max(right.abs());
        wav.write_sample(left.clamp(-1.0, 1.0))
            .map_err(|e| format!("Write audio: {e}"))?;
        wav.write_sample(right.clamp(-1.0, 1.0))
            .map_err(|e| format!("Write audio: {e}"))?;
    }
    Ok(peak)
}

fn render_midi(plugin_path: &Path, request_path: &Path, out: &Path) -> Result<(), String> {
    let bytes = fs::read(request_path).map_err(|e| e.to_string())?;
    let mut request: MidiRenderRequest =
        serde_json::from_slice(&bytes).map_err(|e| format!("Invalid note-event JSON: {e}"))?;
    if request.length_frames == 0
        || request.length_frames > MAX_RENDER_FRAMES
        || request.events.len() > 50_000
        || !(20.0..=300.0).contains(&request.bpm)
    {
        return Err("Out-of-range VST3 MIDI render length, note count or tempo".into());
    }
    request.events.sort_by_key(|event| (event.frame, event.on));
    let mut host = Vst3Host::builder()
        .sample_rate(RATE as f64)
        .block_size(BLOCK)
        .input_channels(0)
        .output_channels(2)
        .tempo(request.bpm)
        .build()
        .map_err(|e| e.to_string())?;
    let mut plugin = host
        .load_plugin(plugin_path)
        .map_err(|e| format!("Load VST3: {e}"))?;
    plugin
        .start_processing()
        .map_err(|e| format!("Start VST3: {e}"))?;
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: RATE,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut wav = hound::WavWriter::create(out, spec).map_err(|e| e.to_string())?;
    let mut buffers = AudioBuffers::new(0, 2, BLOCK, RATE as f64);
    let mut next = 0_usize;
    let mut frame = 0_u64;
    let mut peak = 0.0_f32;
    // One second for reverb/release tails, capped independently from note length.
    let end = request.length_frames.saturating_add(RATE as u64);
    while frame < end {
        buffers.clear();
        let block_end = frame + BLOCK as u64;
        while let Some(event) = request.events.get(next) {
            if event.frame >= block_end {
                break;
            }
            next += 1;
            if event.frame < frame {
                continue;
            }
            let event_type = if event.on {
                MidiEvent::NoteOn {
                    channel: MidiChannel::Ch1,
                    note: event.note,
                    velocity: event.velocity,
                }
            } else {
                MidiEvent::NoteOff {
                    channel: MidiChannel::Ch1,
                    note: event.note,
                    velocity: 0,
                }
            };
            plugin
                .send_midi_event_at(event_type, (event.frame - frame) as i32)
                .map_err(|e| format!("VST3 note event: {e}"))?;
        }
        plugin
            .process_audio(&mut buffers)
            .map_err(|e| format!("VST3 audio processing: {e}"))?;
        let count = (end - frame).min(BLOCK as u64) as usize;
        peak = peak.max(write_stereo(&mut wav, &buffers, count)?);
        frame += count as u64;
    }
    plugin.stop_processing().map_err(|e| e.to_string())?;
    wav.finalize().map_err(|e| e.to_string())?;
    println!("Rendered {} samples at {} Hz; peak {:.5}", end, RATE, peak);
    if peak < 0.000_001 {
        return Err(
            "VST3 rendered silence: check whether this plugin accepts MIDI or needs a preset"
                .into(),
        );
    }
    Ok(())
}

fn render_effect(plugin_path: &Path, input: &Path, out: &Path) -> Result<(), String> {
    let source = daw_engine::decode_to_source(input, RATE, 0)
        .map_err(|e| format!("Decode audio input: {e}"))?;
    let daw_engine::Source::Samples { frames, .. } = source else {
        return Err("Expected decoded stereo audio".into());
    };
    if frames.is_empty() || frames.len() as u64 > MAX_RENDER_FRAMES {
        return Err("VST3 effect input must be 1 frame to 10 minutes".into());
    }
    let mut host = Vst3Host::builder()
        .sample_rate(RATE as f64)
        .block_size(BLOCK)
        .input_channels(2)
        .output_channels(2)
        .build()
        .map_err(|e| e.to_string())?;
    let mut plugin = host
        .load_plugin(plugin_path)
        .map_err(|e| format!("Load VST3: {e}"))?;
    plugin
        .start_processing()
        .map_err(|e| format!("Start VST3: {e}"))?;
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: RATE,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut wav = hound::WavWriter::create(out, spec).map_err(|e| e.to_string())?;
    let mut buffers = AudioBuffers::new(2, 2, BLOCK, RATE as f64);
    let mut peak = 0.0_f32;
    let mut position = 0;
    while position < frames.len() {
        buffers.clear();
        let count = (frames.len() - position).min(BLOCK);
        for i in 0..count {
            buffers.inputs[0][i] = frames[position + i].left;
            buffers.inputs[1][i] = frames[position + i].right;
        }
        plugin
            .process_audio(&mut buffers)
            .map_err(|e| format!("VST3 process effect: {e}"))?;
        peak = peak.max(write_stereo(&mut wav, &buffers, count)?);
        position += count;
    }
    plugin.stop_processing().map_err(|e| e.to_string())?;
    wav.finalize().map_err(|e| e.to_string())?;
    println!(
        "Processed {} frames through VST3; peak {:.5}",
        position, peak
    );
    if peak < 0.000_001 {
        return Err("VST3 output was silent; file saved but not imported".into());
    }
    Ok(())
}

fn read_native_state(path: &Path) -> Result<Vec<u8>, String> {
    use daw_engine::live_plugin::MAX_PLUGIN_STATE_BYTES;
    use std::io::Read;

    let read = || -> std::io::Result<Vec<u8>> {
        let mut bytes = Vec::new();
        std::fs::File::open(path)?
            .take((MAX_PLUGIN_STATE_BYTES + 1) as u64)
            .read_to_end(&mut bytes)?;
        Ok(bytes)
    };
    let bytes = read().map_err(|e| format!("Read native plugin state {}: {e}", path.display()))?;
    if bytes.len() > MAX_PLUGIN_STATE_BYTES {
        return Err(format!(
            "Native plugin state exceeds the {MAX_PLUGIN_STATE_BYTES}-byte limit"
        ));
    }
    Ok(bytes)
}

fn main() -> ExitCode {
    let args = std::env::args_os().collect::<Vec<_>>();
    let status = match args.get(1).and_then(|s| s.to_str()) {
        Some("scan-clap") if args.len() == 3 =>
            clap_render::scan(Path::new(&args[2])),
        Some("save-state-clap") if args.len() == 6 =>
            args[4].to_str().and_then(|raw| raw.parse::<u32>().ok())
                .ok_or_else(|| "Invalid live CLAP sample rate".to_string())
                .and_then(|rate| {
                    clap_stream::save_state(
                        Path::new(&args[2]),
                        args[3].to_str().unwrap_or(""),
                        rate,
                        None,
                        Path::new(&args[5]),
                    )
                }),
        Some("save-state-clap-instrument") if args.len() == 7 =>
            args[4].to_str().and_then(|raw| raw.parse::<u32>().ok())
                .ok_or_else(|| "Invalid live CLAP instrument rate".to_string())
                .and_then(|rate| {
                    clap_stream::save_state_instrument(
                        Path::new(&args[2]),
                        args[3].to_str().unwrap_or(""),
                        rate,
                        clap_stream::parse_state_path(args.get(5).map(|path| path.as_os_str()))
                            .as_deref(),
                        Path::new(&args[6]),
                    )
                }),
        Some("save-state-vst3") if args.len() == 5 =>
            args[3].to_str().and_then(|raw| raw.parse::<u32>().ok())
                .ok_or_else(|| "Invalid VST3 state sample rate".to_string())
                .and_then(|rate| vst3_stream::save_state(Path::new(&args[2]), rate, None, Path::new(&args[4]))),
        Some("save-state-vst3-instrument") if args.len() == 6 =>
            args[3].to_str().and_then(|raw| raw.parse::<u32>().ok())
                .ok_or_else(|| "Invalid VST3 instrument state sample rate".to_string())
                .and_then(|rate| vst3_stream::save_state_instrument(Path::new(&args[2]), rate, clap_stream::parse_state_path(args.get(4).map(|path| path.as_os_str())).as_deref(), Path::new(&args[5]))),
        Some("inspect-clap") if args.len() == 3 =>
            clap_render::inspect(Path::new(&args[2])),
        Some("inspect-vst3") if args.len() == 3 =>
            vst3_stream::inspect(Path::new(&args[2])),
        Some("stream-clap-effect") if args.len() == 5 || args.len() == 6 =>
            args[4].to_str().and_then(|raw| raw.parse::<u32>().ok())
                .ok_or_else(|| "Invalid live CLAP sample rate".to_string())
                .and_then(|rate| clap_stream::run(
                    Path::new(&args[2]),
                    args[3].to_str().unwrap_or(""),
                    rate,
                    clap_stream::parse_state_path(args.get(5).map(|path| path.as_os_str())).as_deref(),
                )),
        Some("stream-clap-instrument") if args.len() == 5 || args.len() == 6 =>
            args[4].to_str().and_then(|raw| raw.parse::<u32>().ok())
                .ok_or_else(|| "Invalid live CLAP instrument rate".to_string())
                .and_then(|rate| clap_stream::run_instrument(
                    Path::new(&args[2]),
                    args[3].to_str().unwrap_or(""),
                    rate,
                    clap_stream::parse_state_path(args.get(5).map(|path| path.as_os_str())).as_deref(),
                )),
        Some("stream-vst3-instrument") if (4..=6).contains(&args.len()) =>
            args[3].to_str().and_then(|raw| raw.parse::<u32>().ok())
                .ok_or_else(|| "Invalid live VST3 instrument rate".to_string())
                .and_then(|rate| vst3_stream::run_instrument(
                    Path::new(&args[2]),
                    rate,
                    clap_stream::parse_state_path(args.get(4).map(|path| path.as_os_str())).as_deref(),
                    args.get(5).and_then(|class| class.to_str()).filter(|class| !class.is_empty()),
                )),
        Some("stream-vst3-effect") if (4..=6).contains(&args.len()) =>
            args[3].to_str().and_then(|raw| raw.parse::<u32>().ok())
                .ok_or_else(|| "Invalid live VST3 sample rate".to_string())
                .and_then(|rate| vst3_stream::run(
                    Path::new(&args[2]),
                    rate,
                    clap_stream::parse_state_path(args.get(4).map(|path| path.as_os_str())).as_deref(),
                    args.get(5).and_then(|class| class.to_str()).filter(|class| !class.is_empty()),
                )),
        Some("render-clap-midi") if args.len() == 5 || args.len() == 6 => clap_render::render_midi(
            Path::new(&args[2]), Path::new(&args[3]), Path::new(&args[4]),
            args.get(5).and_then(|id| id.to_str())),
        Some("render-clap-effect") if args.len() == 5 || args.len() == 6 =>
            clap_render::render_effect(
                Path::new(&args[2]), Path::new(&args[3]), Path::new(&args[4]),
                args.get(5).and_then(|id| id.to_str())),
        Some("render-midi") if args.len() == 5 => render_midi(
            Path::new(&args[2]), Path::new(&args[3]), Path::new(&args[4])),
        Some("render-effect") if args.len() == 5 => render_effect(
            Path::new(&args[2]), Path::new(&args[3]), Path::new(&args[4])),
        Some("render-vst2-effect") if args.len() == 5 => vst2::render_effect(
            Path::new(&args[2]), Path::new(&args[3]), Path::new(&args[4])),
        Some("render-vst2-midi") if args.len() == 5 => vst2::render_midi(
            Path::new(&args[2]), Path::new(&args[3]), Path::new(&args[4])),
        _ => Err(
            "Usage: daw-plugin-host stream-clap-effect BUNDLE CLASS RATE | stream-clap-instrument BUNDLE CLASS RATE | stream-vst3-effect BUNDLE RATE | stream-vst3-instrument BUNDLE RATE | save-state-clap BUNDLE CLASS RATE STATE | save-state-clap-instrument BUNDLE CLASS RATE STATE-IN STATE-OUT | save-state-vst3 BUNDLE RATE STATE | save-state-vst3-instrument BUNDLE RATE STATE-IN STATE-OUT | render-midi PLUGIN.vst3 EVENTS.json OUT.wav | render-effect PLUGIN.vst3 AUDIO.wav OUT.wav"
                .into(),
        ),
    };
    match status {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("Plugin worker error: {err}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn midi_render_event_parses_and_sorts() {
        let input = br#"{"length_frames":4800,"bpm":120,"events":[{"frame":50,"note":60,"velocity":0,"on":false},{"frame":0,"note":60,"velocity":110,"on":true}]}"#;
        let mut req: MidiRenderRequest = serde_json::from_slice(input).unwrap();
        req.events.sort_by_key(|e| (e.frame, e.on));
        assert_eq!(req.events[0].frame, 0);
        assert!(req.events[0].on);
        assert!(!req.events[1].on);
    }
}
