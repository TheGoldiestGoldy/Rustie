//! Isolated realtime VST3 audio-effect/instrument processor.
//! Same fixed 128-frame interleaved stereo-f32 IPC as the CLAP worker, plus the
//! shared opcode/state extension used for native project-state recall.
//! Third-party binaries load only in this child process.
use daw_engine::live_plugin::{LIVE_MIDI_PACKET_BYTES, LiveMidiEvent, LiveMidiMessage};
use vst3_host::midi::{MidiChannel, MidiEvent};

use std::io::{self, Read, Write};
use std::path::Path;
use vst3_host::{Vst3Host, audio::AudioBuffers};

const BLOCK: usize = 128;
const BYTES: usize = BLOCK * 2 * 4;
const READY_BYTES: usize = 9;
const STATE_SAVED: u8 = 1;
const STATE_FAILED: u8 = 2;

pub fn run(
    plugin_path: &Path,
    rate: u32,
    state_path: Option<&Path>,
    class_id: Option<&str>,
) -> Result<(), String> {
    if !(8_000..=192_000).contains(&rate) {
        return Err(format!("Unsupported VST3 sample rate: {rate}"));
    }
    let mut host = Vst3Host::builder()
        .sample_rate(f64::from(rate))
        .block_size(BLOCK)
        .input_channels(2)
        .output_channels(2)
        .build()
        .map_err(|e| format!("Construct VST3 host: {e}"))?;
    let mut plugin = load_live_plugin(&mut host, plugin_path, class_id)?;
    restore_state(&mut plugin, state_path)?;
    plugin
        .start_processing()
        .map_err(|e| format!("Start live VST3: {e}"))?;

    let mut buffers = AudioBuffers::new(2, 2, BLOCK, f64::from(rate));
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut input = stdin.lock();
    let mut output = stdout.lock();
    let mut encoded = [0u8; BYTES];
    let (latency, tail) = (plugin.latency_samples(), plugin.tail_samples());
    write_ready(&mut output, latency, tail, &plugin.info().uid)?;
    loop {
        let mut opcode = [0u8; 1];
        match input.read(&mut opcode) {
            Ok(0) => break,
            Ok(_) => {}
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(format!("Read live VST3 opcode: {e}")),
        }
        let mut read_bytes = 0;
        while read_bytes < BYTES {
            match input.read(&mut encoded[read_bytes..]) {
                Ok(0) if read_bytes == 0 => break,
                Ok(0) => return Err("Truncated VST3 live audio block".into()),
                Ok(n) => read_bytes += n,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(format!("Read live VST3 audio: {e}")),
            }
        }
        if read_bytes == 0 {
            break;
        }
        buffers.clear();
        for (channel, samples) in buffers.inputs.iter_mut().enumerate() {
            for (i, dest) in samples.iter_mut().enumerate().take(BLOCK) {
                let start = (i * 2 + channel) * 4;
                let sample =
                    f32::from_le_bytes(encoded[start..start + 4].try_into().expect("float sample"));
                *dest = if sample.is_finite() { sample } else { 0.0 };
            }
        }
        plugin
            .process_audio(&mut buffers)
            .map_err(|e| format!("Process live VST3 audio: {e}"))?;
        for i in 0..BLOCK {
            for channel in 0..2 {
                let sample = buffers
                    .outputs
                    .get(channel)
                    .and_then(|ch| ch.get(i))
                    .copied()
                    .unwrap_or(0.0);
                let safe = if sample.is_finite() {
                    sample.clamp(-4.0, 4.0)
                } else {
                    0.0
                };
                let start = (i * 2 + channel) * 4;
                encoded[start..start + 4].copy_from_slice(&safe.to_le_bytes());
            }
        }

        let mut status = 0_u8;
        let mut saved_state = Vec::new();
        if opcode[0] & 1 != 0 {
            match plugin.save_state() {
                Ok(state) if state.len() <= daw_engine::live_plugin::MAX_PLUGIN_STATE_BYTES => {
                    saved_state = state;
                    status |= STATE_SAVED;
                }
                Ok(_) | Err(_) => status |= STATE_FAILED,
            }
        }

        output
            .write_all(&[status])
            .map_err(|e| format!("Write live VST3 status: {e}"))?;
        output
            .write_all(&encoded)
            .map_err(|e| format!("Write live VST3 audio: {e}"))?;
        if status & STATE_SAVED != 0 {
            output
                .write_all(&(saved_state.len() as u32).to_le_bytes())
                .map_err(|e| format!("Write live VST3 state length: {e}"))?;
            output
                .write_all(&saved_state)
                .map_err(|e| format!("Write live VST3 state: {e}"))?;
        }
        output
            .flush()
            .map_err(|e| format!("Flush VST3 audio: {e}"))?;
    }
    plugin.stop_processing().map_err(|e| e.to_string())?;
    Ok(())
}

/// Isolated VST3 MIDI instrument with a fixed 128-frame note/audio IPC protocol.
/// The parent waits for the readiness packet before this synth replaces the
/// built-in sine fallback. The VST3 code never runs in the DAW realtime callback.
pub fn run_instrument(
    plugin_path: &Path,
    rate: u32,
    state_path: Option<&Path>,
    class_id: Option<&str>,
) -> Result<(), String> {
    if !(8_000..=192_000).contains(&rate) {
        return Err(format!("Unsupported VST3 instrument rate: {rate}"));
    }
    let mut host = Vst3Host::builder()
        .sample_rate(f64::from(rate))
        .block_size(BLOCK)
        .input_channels(0)
        .output_channels(2)
        .build()
        .map_err(|e| format!("Create VST3 instrument host: {e}"))?;
    let mut plugin = load_live_plugin(&mut host, plugin_path, class_id)?;
    restore_state(&mut plugin, state_path)?;
    plugin
        .start_processing()
        .map_err(|e| format!("Start VST3 instrument: {e}"))?;

    let mut buffers = AudioBuffers::new(0, 2, BLOCK, f64::from(rate));
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut input = stdin.lock();
    let mut output = stdout.lock();
    let mut audio = [0u8; BYTES];
    let mut notes = [0u8; LIVE_MIDI_PACKET_BYTES];
    let mut held = [[0_u8; 128]; 16];
    let mut sustained = [false; 16];
    let (latency, tail) = (plugin.latency_samples(), plugin.tail_samples());
    write_ready(&mut output, latency, tail, &plugin.info().uid)?;
    loop {
        let mut opcode = [0u8; 1];
        match input.read(&mut opcode) {
            Ok(0) => break,
            Ok(_) => {}
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(format!("Read VST3 instrument opcode: {e}")),
        }
        input
            .read_exact(&mut audio)
            .map_err(|e| format!("Truncated VST3 instrument audio: {e}"))?;
        input
            .read_exact(&mut notes)
            .map_err(|e| format!("Truncated VST3 instrument MIDI: {e}"))?;
        buffers.clear();
        for event in LiveMidiEvent::decode_packet(&notes)? {
            let event = event?;
            deliver_midi(event, &mut held, &mut sustained, |message, offset| {
                plugin
                    .send_midi_event_at(message, offset)
                    .map_err(|e| format!("VST3 instrument MIDI delivery: {e}"))
            })?;
        }
        plugin
            .process_audio(&mut buffers)
            .map_err(|e| format!("VST3 instrument process: {e}"))?;
        for i in 0..BLOCK {
            for channel in 0..2 {
                let value = buffers
                    .outputs
                    .get(channel)
                    .and_then(|ch| ch.get(i))
                    .copied()
                    .unwrap_or_default();
                let safe = if value.is_finite() {
                    value.clamp(-4.0, 4.0)
                } else {
                    0.0
                };
                let offset = (i * 2 + channel) * 4;
                audio[offset..offset + 4].copy_from_slice(&safe.to_le_bytes());
            }
        }

        let mut status = 0_u8;
        let mut saved_state = Vec::new();
        if opcode[0] & 1 != 0 {
            match plugin.save_state() {
                Ok(state) if state.len() <= daw_engine::live_plugin::MAX_PLUGIN_STATE_BYTES => {
                    saved_state = state;
                    status |= STATE_SAVED;
                }
                Ok(_) | Err(_) => status |= STATE_FAILED,
            }
        }

        output
            .write_all(&[status])
            .map_err(|e| format!("Write VST3 instrument status: {e}"))?;
        output
            .write_all(&audio)
            .map_err(|e| format!("Write VST3 instrument audio: {e}"))?;
        if status & STATE_SAVED != 0 {
            output
                .write_all(&(saved_state.len() as u32).to_le_bytes())
                .map_err(|e| format!("Write VST3 instrument state length: {e}"))?;
            output
                .write_all(&saved_state)
                .map_err(|e| format!("Write VST3 instrument state: {e}"))?;
        }
        output
            .flush()
            .map_err(|e| format!("Flush VST3 instrument audio: {e}"))?;
    }
    plugin.stop_processing().map_err(|e| e.to_string())?;
    Ok(())
}

/// Load a VST3 bundle once and report the class the host selected. This is the
/// same loader/integrity path used for live work; the report is descriptive,
/// not a compatibility promise.
pub fn inspect(plugin_path: &Path) -> Result<(), String> {
    let mut host = Vst3Host::builder()
        .sample_rate(48_000.0)
        .block_size(BLOCK)
        .input_channels(2)
        .output_channels(2)
        .build()
        .map_err(|e| format!("Construct VST3 host: {e}"))?;
    let plugin = host
        .load_plugin(plugin_path)
        .map_err(|e| format!("Load VST3: {e}"))?;
    let info = plugin.info();
    println!(
        "format=vst3\tpath={}\tname={}\tvendor={}\tversion={}\tcategory={}\tuid={}\taudio_in={}\taudio_out={}\tmidi_in={}\tmidi_out={}\tgui={}\tlatency_samples={}\ttail_samples={}",
        info.path.display(),
        info.name,
        info.vendor,
        info.version,
        info.category,
        info.uid,
        info.audio_inputs,
        info.audio_outputs,
        info.has_midi_input,
        info.has_midi_output,
        info.has_gui,
        plugin.latency_samples(),
        plugin.tail_samples(),
    );
    Ok(())
}

/// Capture the opaque native state of a live VST3 plugin.
pub fn save_state(
    plugin_path: &Path,
    rate: u32,
    state_in: Option<&Path>,
    state_out: &Path,
) -> Result<(), String> {
    if !(8_000..=192_000).contains(&rate) {
        return Err(format!("Unsupported VST3 sample rate: {rate}"));
    }
    let mut host = Vst3Host::builder()
        .sample_rate(f64::from(rate))
        .block_size(BLOCK)
        .input_channels(2)
        .output_channels(2)
        .build()
        .map_err(|e| format!("Construct VST3 host: {e}"))?;
    let mut plugin = host
        .load_plugin(plugin_path)
        .map_err(|e| format!("Load VST3: {e}"))?;
    restore_state(&mut plugin, state_in)?;
    let state = plugin
        .save_state()
        .map_err(|e| format!("Save VST3 state: {e}"))?;
    plugin.stop_processing().map_err(|e| e.to_string())?;
    std::fs::write(state_out, &state).map_err(|e| format!("Write VST3 state file: {e}"))?;
    println!(
        "Saved {} bytes of native VST3 state to {}",
        state.len(),
        state_out.display()
    );
    Ok(())
}

/// Capture the opaque native state of a live VST3 MIDI instrument.
pub fn save_state_instrument(
    plugin_path: &Path,
    rate: u32,
    state_in: Option<&Path>,
    state_out: &Path,
) -> Result<(), String> {
    if !(8_000..=192_000).contains(&rate) {
        return Err(format!("Unsupported VST3 instrument rate: {rate}"));
    }
    let mut host = Vst3Host::builder()
        .sample_rate(f64::from(rate))
        .block_size(BLOCK)
        .input_channels(0)
        .output_channels(2)
        .build()
        .map_err(|e| format!("Construct VST3 instrument host: {e}"))?;
    let mut plugin = host
        .load_plugin(plugin_path)
        .map_err(|e| format!("Load VST3 instrument: {e}"))?;
    if let Some(path) = state_in {
        let bytes =
            std::fs::read(path).map_err(|e| format!("Read VST3 state {}: {e}", path.display()))?;
        if bytes.len() > daw_engine::live_plugin::MAX_PLUGIN_STATE_BYTES {
            return Err(format!(
                "VST3 state {} exceeds the {}-byte limit",
                path.display(),
                daw_engine::live_plugin::MAX_PLUGIN_STATE_BYTES
            ));
        }
        plugin
            .load_state(&bytes)
            .map_err(|e| format!("Restore VST3 state: {e}"))?;
    }
    let state = plugin
        .save_state()
        .map_err(|e| format!("Save VST3 state: {e}"))?;
    plugin.stop_processing().map_err(|e| e.to_string())?;
    std::fs::write(state_out, &state).map_err(|e| format!("Write VST3 state file: {e}"))?;
    println!(
        "Saved {} bytes of native VST3 instrument state to {}",
        state.len(),
        state_out.display()
    );
    Ok(())
}

fn deliver_midi(
    event: LiveMidiEvent,
    held: &mut [[u8; 128]; 16],
    sustained: &mut [bool; 16],
    mut send: impl FnMut(MidiEvent, i32) -> Result<(), String>,
) -> Result<(), String> {
    let index = usize::from(event.channel);
    let channel = MidiChannel::from_index(event.channel).ok_or("Invalid VST3 MIDI channel")?;
    let mut send = |message| send(message, i32::from(event.offset));
    let message = match event.message() {
        LiveMidiMessage::NoteOn(note, velocity) => {
            let count = &mut held[index][usize::from(note)];
            *count = count.saturating_add(1);
            MidiEvent::NoteOn {
                channel,
                note,
                velocity,
            }
        }
        LiveMidiMessage::NoteOff(note) => {
            let count = &mut held[index][usize::from(note)];
            *count = count.saturating_sub(1);
            MidiEvent::NoteOff {
                channel,
                note,
                velocity: 0,
            }
        }
        LiveMidiMessage::Sustain(value) => {
            sustained[index] = value >= 64;
            MidiEvent::ControlChange {
                channel,
                controller: 64,
                value,
            }
        }
        LiveMidiMessage::PitchBend(value) => MidiEvent::PitchBend { channel, value },
        panic @ (LiveMidiMessage::Panic | LiveMidiMessage::ChannelPanic) => {
            for (i, notes) in held.iter_mut().enumerate() {
                if panic == LiveMidiMessage::ChannelPanic && i != index {
                    continue;
                }
                let channel = MidiChannel::from_index(i as u8).expect("16 MIDI channels");
                if std::mem::take(&mut sustained[i]) {
                    send(MidiEvent::ControlChange {
                        channel,
                        controller: 64,
                        value: 0,
                    })?;
                }
                for (note, count) in notes.iter_mut().enumerate() {
                    for _ in 0..std::mem::take(count) {
                        send(MidiEvent::NoteOff {
                            channel,
                            note: note as u8,
                            velocity: 0,
                        })?;
                    }
                }
            }
            return Ok(());
        }
    };
    send(message)
}

fn restore_state(plugin: &mut vst3_host::Plugin, state_path: Option<&Path>) -> Result<(), String> {
    let Some(path) = state_path else {
        return Ok(());
    };
    let bytes = crate::read_native_state(path)?;
    plugin
        .load_state(&bytes)
        .map_err(|e| format!("Restore live VST3 state: {e}"))
}

fn load_live_plugin(
    host: &mut Vst3Host,
    path: &Path,
    class_id: Option<&str>,
) -> Result<vst3_host::Plugin, String> {
    let plugin = match class_id {
        Some(class) => host.load_plugin_class(path, class),
        None => host.load_plugin(path),
    }
    .map_err(|e| format!("Load live VST3: {e}"))?;
    if let Some(class) = class_id.filter(|class| !plugin.info().uid.eq_ignore_ascii_case(class)) {
        return Err(format!(
            "VST3 requested class {class}, loaded {}",
            plugin.info().uid
        ));
    }
    Ok(plugin)
}

fn write_ready(output: &mut impl Write, latency: u32, tail: u32, uid: &str) -> Result<(), String> {
    if uid.len() != 32 || !uid.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(format!("Invalid VST3 class ID: {uid}"));
    }
    let mut ready = [0u8; READY_BYTES];
    ready[0] = b'R';
    ready[1..5].copy_from_slice(&latency.to_le_bytes());
    ready[5..9].copy_from_slice(&tail.to_le_bytes());
    output
        .write_all(&ready)
        .map_err(|e| format!("VST3 worker readiness: {e}"))?;
    output
        .write_all(uid.as_bytes())
        .map_err(|e| format!("VST3 worker class ID: {e}"))?;
    output
        .flush()
        .map_err(|e| format!("Flush VST3 worker readiness: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn forwards_vst3_expression_and_releases_sustain_on_panic() {
        let mut held = [[0; 128]; 16];
        let mut sustained = [false; 16];
        let mut sent = Vec::new();
        for (offset, note, velocity, kind) in [
            (1, 60, 80, 1),
            (3, 64, 127, 3),
            (16, 0, 65, 4),
            (80, 0, 0, 2),
        ] {
            deliver_midi(
                LiveMidiEvent {
                    offset,
                    note,
                    velocity,
                    kind,
                    channel: 2,
                },
                &mut held,
                &mut sustained,
                |event, time| {
                    sent.push((event, time));
                    Ok(())
                },
            )
            .unwrap();
        }
        let channel = MidiChannel::Ch3;
        assert_eq!(
            sent,
            [
                (
                    MidiEvent::NoteOn {
                        channel,
                        note: 60,
                        velocity: 80
                    },
                    1
                ),
                (
                    MidiEvent::ControlChange {
                        channel,
                        controller: 64,
                        value: 127
                    },
                    3
                ),
                (
                    MidiEvent::PitchBend {
                        channel,
                        value: 8320
                    },
                    16
                ),
                (
                    MidiEvent::ControlChange {
                        channel,
                        controller: 64,
                        value: 0
                    },
                    80
                ),
                (
                    MidiEvent::NoteOff {
                        channel,
                        note: 60,
                        velocity: 0
                    },
                    80
                ),
            ]
        );
    }

    #[test]
    fn pcm_protocol_matches_live_clap_bridge() {
        assert_eq!(BLOCK, 128);
        assert_eq!(BYTES, 1024);
        assert_eq!(READY_BYTES, 9);
        assert_eq!(STATE_SAVED & STATE_FAILED, 0);
    }
}
