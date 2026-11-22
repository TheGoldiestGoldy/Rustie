//! Isolated low-latency CLAP audio-effect/instrument worker.
//!
//! Wire protocol on stdin/stdout (little-endian):
//!   parent -> child: `opcode:u8`, 128-frame interleaved stereo f32 audio,
//!                    then for instruments `count:u8` + 64 x (offset,data1,data2,channel/kind)
//!   child -> parent: `status:u8`, 128-frame interleaved stereo f32 audio,
//!                    then if status bit 0 is set `len:u32` + native state bytes
//!   startup: child writes b"R" + reported latency u32 + tail u32 before blocks.
//!
//! opcode bit 0 asks for a state snapshot on this block. The plugin still runs
//! only in this supervised child, never in the DAW audio callback. No stdout
//! logging is allowed: every byte after the readiness prefix is audio/state.
use clack_host::events::event_types::{MidiEvent, NoteChokeEvent, NoteOffEvent, NoteOnEvent};
use clack_host::extensions::prelude::*;
use clack_host::factory::plugin::PluginFactory;
use clack_host::prelude::*;
use clack_host::stream::{InputStream, OutputStream};
use clap_sys::{
    ext::{
        latency::{CLAP_EXT_LATENCY, clap_plugin_latency},
        note_ports::{CLAP_EXT_NOTE_PORTS, CLAP_NOTE_DIALECT_MIDI, clap_plugin_note_ports},
        state::{CLAP_EXT_STATE, clap_plugin_state},
    },
    plugin::clap_plugin,
};
use daw_engine::live_plugin::{LIVE_MIDI_PACKET_BYTES, LiveMidiEvent, LiveMidiMessage};
use std::{
    ffi::CStr,
    fs,
    io::{self, Cursor, Read, Write},
    path::{Path, PathBuf},
};

pub const STREAM_BLOCK: usize = 128;
const CHANNELS: usize = 2;
const BYTES: usize = STREAM_BLOCK * CHANNELS * 4;
const READY_BYTES: usize = 9;
const STATE_SAVED: u8 = 1;
const STATE_FAILED: u8 = 2;

/// Minimal host-side view of the CLAP state extension. Only the main-thread
/// worker calls it; the extension pointer belongs to the live plugin instance.
#[derive(Copy, Clone)]
struct PluginStateExt(RawExtension<PluginExtensionSide, clap_plugin_state>);

// SAFETY: CLAP_EXT_STATE is the CLAP state extension identifier and the raw
// pointer is ABI-compatible with `clap_plugin_state`.
unsafe impl Extension for PluginStateExt {
    const IDENTIFIERS: &[&CStr] = &[CLAP_EXT_STATE];
    type ExtensionSide = PluginExtensionSide;

    unsafe fn from_raw(raw: RawExtension<Self::ExtensionSide>) -> Self {
        Self(unsafe { raw.cast() })
    }
}

struct StateWriter<'a> {
    bytes: &'a mut Vec<u8>,
    overflowed: bool,
}

impl Write for StateWriter<'_> {
    fn write(&mut self, input: &[u8]) -> io::Result<usize> {
        let limit = daw_engine::live_plugin::MAX_PLUGIN_STATE_BYTES;
        if input.len() > limit.saturating_sub(self.bytes.len()) {
            self.overflowed = true;
            return Err(io::Error::other(
                "CLAP state exceeds the native state limit",
            ));
        }
        self.bytes.extend_from_slice(input);
        Ok(input.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl PluginStateExt {
    fn save(self, plugin: *const clap_plugin, out: &mut Vec<u8>) -> Result<(), String> {
        // SAFETY: the extension pointer stays valid while this worker owns the
        // plugin instance, and state save is called from its main thread.
        let state = unsafe { self.0.as_ptr().as_ref() };
        let save = state
            .save
            .ok_or("CLAP state extension has no save callback")?;
        let mut writer = StateWriter {
            bytes: out,
            overflowed: false,
        };
        let mut stream = OutputStream::from_writer(&mut writer);
        // SAFETY: `plugin` is the matching live instance and `stream` is a
        // valid host-managed output stream for the duration of the call.
        if unsafe { save(plugin, stream.as_raw_mut()) } && !writer.overflowed {
            Ok(())
        } else {
            Err("CLAP plugin refused to save state".into())
        }
    }

    fn load(self, plugin: *const clap_plugin, bytes: &[u8]) -> Result<(), String> {
        // SAFETY: see `save`; the extension and instance are still alive.
        let state = unsafe { self.0.as_ptr().as_ref() };
        let load = state
            .load
            .ok_or("CLAP state extension has no load callback")?;
        let mut reader = Cursor::new(bytes);
        let mut stream = InputStream::from_reader(&mut reader);
        // SAFETY: `plugin` is the matching live instance and `stream` is a
        // valid host-managed input stream for the duration of the call.
        if unsafe { load(plugin, stream.as_raw_mut()) } {
            Ok(())
        } else {
            Err("CLAP plugin refused the saved state".into())
        }
    }
}

#[derive(Copy, Clone)]
struct PluginNotePorts(RawExtension<PluginExtensionSide, clap_plugin_note_ports>);

// SAFETY: this identifier always returns the CLAP note port ABI.
unsafe impl Extension for PluginNotePorts {
    const IDENTIFIERS: &[&CStr] = &[CLAP_EXT_NOTE_PORTS];
    type ExtensionSide = PluginExtensionSide;

    unsafe fn from_raw(raw: RawExtension<Self::ExtensionSide>) -> Self {
        Self(unsafe { raw.cast() })
    }
}

impl PluginNotePorts {
    fn supports_midi(self, plugin: *const clap_plugin) -> bool {
        // SAFETY: query the first input port while the matching plugin is live
        // on its main thread. The output is only read on a successful query.
        unsafe {
            let ports = self.0.as_ptr().as_ref();
            let Some(get) = ports.get else { return false };
            let mut info = std::mem::MaybeUninit::uninit();
            get(plugin, 0, true, info.as_mut_ptr())
                && info.assume_init().supported_dialects & CLAP_NOTE_DIALECT_MIDI != 0
        }
    }
}

fn deliver_midi(events: &mut EventBuffer, event: LiveMidiEvent, raw_midi: bool) {
    let time = u32::from(event.offset);
    let channel = event.channel;
    let message = event.message();
    if raw_midi {
        let data = match message {
            LiveMidiMessage::NoteOn(note, velocity) => [0x90 | channel, note, velocity],
            LiveMidiMessage::NoteOff(note) => [0x80 | channel, note, 0],
            LiveMidiMessage::Sustain(value) => [0xb0 | channel, 64, value],
            LiveMidiMessage::PitchBend(_) => [0xe0 | channel, event.note, event.velocity],
            LiveMidiMessage::Panic | LiveMidiMessage::ChannelPanic => {
                for target in 0..16 {
                    if message == LiveMidiMessage::Panic || target == channel {
                        for controller in [64, 123, 120] {
                            events.push(&MidiEvent::new(time, 0, [0xb0 | target, controller, 0]));
                        }
                    }
                }
                return;
            }
        };
        events.push(&MidiEvent::new(time, 0, data));
        return;
    }

    let target = Pckn::new(0u16, u16::from(channel), u16::from(event.note), 0u32);
    match message {
        LiveMidiMessage::NoteOn(_, velocity) => {
            events.push(&NoteOnEvent::new(time, target, f64::from(velocity) / 127.0));
        }
        LiveMidiMessage::NoteOff(_) => events.push(&NoteOffEvent::new(time, target, 0.0)),
        LiveMidiMessage::Panic | LiveMidiMessage::ChannelPanic => {
            let mut target = Pckn::match_all();
            target.port_index = 0u16.into();
            if message == LiveMidiMessage::ChannelPanic {
                target.channel = u16::from(channel).into();
            }
            events.push(&NoteChokeEvent::new(time, target));
        }
        LiveMidiMessage::Sustain(_) | LiveMidiMessage::PitchBend(_) => {}
    }
}

/// Minimal host-side view of CLAP's optional latency extension.
#[derive(Copy, Clone)]
struct PluginLatencyExt(RawExtension<PluginExtensionSide, clap_plugin_latency>);

// SAFETY: CLAP_EXT_LATENCY is the CLAP latency extension identifier and the raw
// pointer is ABI-compatible with `clap_plugin_latency`.
unsafe impl Extension for PluginLatencyExt {
    const IDENTIFIERS: &[&CStr] = &[CLAP_EXT_LATENCY];
    type ExtensionSide = PluginExtensionSide;

    unsafe fn from_raw(raw: RawExtension<Self::ExtensionSide>) -> Self {
        Self(unsafe { raw.cast() })
    }
}

impl PluginLatencyExt {
    fn get(self, plugin: *const clap_plugin) -> u32 {
        // SAFETY: extension and instance are alive for this worker iteration.
        let latency = unsafe { self.0.as_ptr().as_ref() };
        match latency.get {
            None => 0,
            // SAFETY: the function pointer comes from the matching extension.
            Some(get) => unsafe { get(plugin) },
        }
    }
}

pub fn run(
    path: &Path,
    class_id: &str,
    rate: u32,
    state_path: Option<&Path>,
) -> Result<(), String> {
    run_mode(path, class_id, rate, false, state_path)
}

/// CLAP instruments use the same isolated child/128-frame audio return, plus a
/// fixed-size note-event trailer on each stdin block.
pub fn run_instrument(
    path: &Path,
    class_id: &str,
    rate: u32,
    state_path: Option<&Path>,
) -> Result<(), String> {
    run_mode(path, class_id, rate, true, state_path)
}

fn run_mode(
    path: &Path,
    class_id: &str,
    rate: u32,
    instrument: bool,
    state_path: Option<&Path>,
) -> Result<(), String> {
    if !(8_000..=192_000).contains(&rate) {
        return Err(format!("Unsupported live CLAP rate: {rate}"));
    }
    // SAFETY: user-provided third-party plugin code is loaded ONLY inside the
    // supervised child executable, not the real-time DAW process.
    let entry = unsafe { PluginEntry::load(path.as_os_str()) }
        .map_err(|e| format!("Load live CLAP {}: {e}", path.display()))?;
    let factory = entry
        .get_factory::<PluginFactory>()
        .ok_or("Plugin bundle has no CLAP factory")?;
    let descriptor = factory
        .plugin_descriptors()
        .find(|desc| {
            desc.id()
                .is_some_and(|id| id.to_bytes() == class_id.as_bytes())
        })
        .ok_or_else(|| format!("Unknown CLAP class: {class_id}"))?;
    if instrument
        && !descriptor
            .features()
            .any(|feature| feature.to_bytes() == b"instrument")
    {
        return Err(format!(
            "CLAP class {class_id} does not declare instrument support"
        ));
    }
    if !instrument
        && !descriptor
            .features()
            .any(|feature| feature.to_bytes() == b"audio-effect")
    {
        return Err(format!(
            "CLAP class {class_id} does not declare audio-effect support"
        ));
    }
    let id = descriptor.id().ok_or("Missing CLAP class ID")?;
    let info = HostInfo::new(
        "Slate Studio",
        "Rustie",
        "https://github.com/TheGoldiestGoldy/Rustie",
        "0.1",
    )
    .map_err(|e| e.to_string())?;
    let mut plugin = PluginInstance::<()>::new(|_| (), |_| (), &entry, id, &info)
        .map_err(|e| format!("Create live CLAP: {e}"))?;

    // Extensions and state load must happen on the main thread before audio
    // activation. Missing extensions only disable that feature; they never
    // turn a working plugin into an error.
    let (state_ext, latency_ext) = {
        let shared = plugin.plugin_shared_handle();
        (
            shared.get_extension::<PluginStateExt>(),
            shared.get_extension::<PluginLatencyExt>(),
        )
    };
    if let Some(path) = state_path {
        let bytes = crate::read_native_state(path)?;
        let state_ext = state_ext.ok_or("CLAP plugin has no state extension to restore")?;
        state_ext
            .load(plugin.plugin_shared_handle().as_raw(), &bytes)
            .map_err(|e| format!("Restore live CLAP state: {e}"))?;
    }
    let reported_latency = latency_ext
        .map(|ext| ext.get(plugin.plugin_shared_handle().as_raw()))
        .unwrap_or(0);

    let shared = plugin.plugin_shared_handle();
    let raw_midi = instrument
        && shared
            .get_extension::<PluginNotePorts>()
            .is_some_and(|ports| ports.supports_midi(shared.as_raw()));
    let processor = plugin
        .activate(
            |_, _| (),
            PluginAudioConfiguration {
                sample_rate: f64::from(rate),
                min_frames_count: STREAM_BLOCK as u32,
                max_frames_count: STREAM_BLOCK as u32,
            },
        )
        .map_err(|e| format!("Activate live CLAP: {e}"))?;
    let mut processor = processor
        .start_processing()
        .map_err(|e| format!("Start live CLAP: {e}"))?;

    let mut in_ports = AudioPorts::with_capacity(2, 1);
    let mut out_ports = AudioPorts::with_capacity(2, 1);
    let mut input_buf = [[0.0_f32; STREAM_BLOCK]; CHANNELS];
    let mut output_buf = [[0.0_f32; STREAM_BLOCK]; CHANNELS];
    let mut in_events = EventBuffer::with_capacity(256);
    let mut out_events = EventBuffer::new();
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut input = stdin.lock();
    let mut output = stdout.lock();
    let mut encoded = [0_u8; BYTES];
    let mut midi_bytes = [0_u8; LIVE_MIDI_PACKET_BYTES];

    // Readiness carries the plugin's reported latency/tail so the parent can
    // count the bridge buffer honestly instead of assuming compensation.
    let mut ready = [0_u8; READY_BYTES];
    ready[0] = b'R';
    ready[1..5].copy_from_slice(&reported_latency.to_le_bytes());
    ready[5..9].copy_from_slice(&0_u32.to_le_bytes());
    output
        .write_all(&ready)
        .map_err(|e| format!("CLAP worker readiness: {e}"))?;
    output
        .flush()
        .map_err(|e| format!("Flush CLAP worker readiness: {e}"))?;

    loop {
        let mut opcode = [0_u8; 1];
        match input.read(&mut opcode) {
            Ok(0) => break,
            Ok(_) => {}
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(format!("Read live CLAP opcode: {e}")),
        }
        let mut filled = 0_usize;
        while filled < BYTES {
            match input.read(&mut encoded[filled..]) {
                Ok(0) if filled == 0 => break,
                Ok(0) => return Err("Truncated live CLAP block".into()),
                Ok(n) => filled += n,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(format!("Read live CLAP audio: {e}")),
            }
        }
        if filled == 0 {
            break;
        }
        for (ch, channel) in input_buf.iter_mut().enumerate() {
            for (i, sample_out) in channel.iter_mut().enumerate() {
                let offset = (i * CHANNELS + ch) * 4;
                let sample = f32::from_le_bytes(
                    encoded[offset..offset + 4]
                        .try_into()
                        .expect("slice has four bytes"),
                );
                *sample_out = if sample.is_finite() { sample } else { 0.0 };
            }
        }
        for channel in &mut output_buf {
            channel.fill(0.0);
        }
        out_events.clear();
        in_events.clear();
        if instrument {
            input
                .read_exact(&mut midi_bytes)
                .map_err(|e| format!("Read live instrument note packet: {e}"))?;
            for event in LiveMidiEvent::decode_packet(&midi_bytes)? {
                deliver_midi(&mut in_events, event?, raw_midi);
            }
        }
        if instrument {
            let ins = in_ports.with_input_buffers(std::iter::empty::<
                AudioPortBuffer<
                    std::iter::Empty<InputChannel<'_, f32>>,
                    std::iter::Empty<InputChannel<'_, f64>>,
                >,
            >());
            let mut outs = out_ports.with_output_buffers([AudioPortBuffer {
                latency: 0,
                channels: AudioPortBufferType::f32_output_only(
                    output_buf.iter_mut().map(|samples| samples.as_mut_slice()),
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
                .map_err(|e| format!("Live CLAP instrument process: {e}"))?;
        } else {
            let ins = in_ports.with_input_buffers([AudioPortBuffer {
                latency: 0,
                channels: AudioPortBufferType::f32_input_only(
                    input_buf.iter_mut().map(InputChannel::variable),
                ),
            }]);
            let mut outs = out_ports.with_output_buffers([AudioPortBuffer {
                latency: 0,
                channels: AudioPortBufferType::f32_output_only(
                    output_buf.iter_mut().map(|samples| samples.as_mut_slice()),
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
                .map_err(|e| format!("Live CLAP process: {e}"))?;
        }
        for (ch, channel) in output_buf.iter().enumerate() {
            for (i, sample) in channel.iter().enumerate() {
                let safe = if sample.is_finite() {
                    sample.clamp(-4.0, 4.0)
                } else {
                    0.0
                };
                let offset = (i * CHANNELS + ch) * 4;
                encoded[offset..offset + 4].copy_from_slice(&safe.to_le_bytes());
            }
        }

        let mut status = 0_u8;
        let mut saved_state = Vec::new();
        if opcode[0] & 1 != 0 {
            match state_ext {
                Some(ext) => {
                    match ext.save(processor.shared_plugin_handle().as_raw(), &mut saved_state) {
                        Ok(())
                            if saved_state.len()
                                <= daw_engine::live_plugin::MAX_PLUGIN_STATE_BYTES =>
                        {
                            status |= STATE_SAVED;
                        }
                        Ok(()) => status |= STATE_FAILED,
                        Err(_) => status |= STATE_FAILED,
                    }
                }
                None => status |= STATE_FAILED,
            }
        }

        output
            .write_all(&[status])
            .map_err(|e| format!("Write live CLAP status: {e}"))?;
        output
            .write_all(&encoded)
            .map_err(|e| format!("Write live CLAP audio: {e}"))?;
        if status & STATE_SAVED != 0 {
            output
                .write_all(&(saved_state.len() as u32).to_le_bytes())
                .map_err(|e| format!("Write live CLAP state length: {e}"))?;
            output
                .write_all(&saved_state)
                .map_err(|e| format!("Write live CLAP state: {e}"))?;
        }
        // StdoutLock is buffered when connected to a pipe. Without flushing
        // each fixed block the parent waits for 128 samples while this child
        // waits for more input: an IPC deadlock, not an audio DSP failure.
        output
            .flush()
            .map_err(|e| format!("Flush live CLAP block: {e}"))?;
    }
    let stopped = processor.stop_processing();
    plugin.deactivate(stopped);
    Ok(())
}

/// Read an optional command-line state file path.
pub fn parse_state_path(raw: Option<&std::ffi::OsStr>) -> Option<PathBuf> {
    raw.filter(|path| !path.is_empty()).map(PathBuf::from)
}

/// Load a live CLAP instance and capture (or accept) its opaque native state.
/// The plugin still runs only inside this supervised child process.
pub fn save_state(
    path: &Path,
    class_id: &str,
    rate: u32,
    state_in: Option<&Path>,
    state_out: &Path,
) -> Result<(), String> {
    if !(8_000..=192_000).contains(&rate) {
        return Err(format!("Unsupported live CLAP rate: {rate}"));
    }
    let entry = unsafe { PluginEntry::load(path.as_os_str()) }
        .map_err(|e| format!("Load live CLAP {}: {e}", path.display()))?;
    let factory = entry
        .get_factory::<PluginFactory>()
        .ok_or("Plugin bundle has no CLAP factory")?;
    let descriptor = factory
        .plugin_descriptors()
        .find(|desc| {
            desc.id()
                .is_some_and(|id| id.to_bytes() == class_id.as_bytes())
        })
        .ok_or_else(|| format!("Unknown CLAP class: {class_id}"))?;
    if !descriptor
        .features()
        .any(|feature| feature.to_bytes() == b"audio-effect")
    {
        return Err(format!(
            "CLAP class {class_id} does not declare audio-effect support"
        ));
    }
    let id = descriptor.id().ok_or("Missing CLAP class ID")?;
    let info = HostInfo::new(
        "Slate Studio",
        "Rustie",
        "https://github.com/TheGoldiestGoldy/Rustie",
        "0.1",
    )
    .map_err(|e| e.to_string())?;
    let mut plugin = PluginInstance::<()>::new(|_| (), |_| (), &entry, id, &info)
        .map_err(|e| format!("Create live CLAP: {e}"))?;
    let shared = plugin.plugin_shared_handle();
    let state_ext = shared.get_extension::<PluginStateExt>();
    if let Some(state) = state_in {
        let bytes = crate::read_native_state(state)?;
        state_ext
            .as_ref()
            .ok_or("CLAP plugin has no state extension to restore")?
            .load(shared.as_raw(), &bytes)
            .map_err(|e| format!("Restore live CLAP state: {e}"))?;
    }
    let processor = plugin
        .activate(
            |_, _| (),
            PluginAudioConfiguration {
                sample_rate: f64::from(rate),
                min_frames_count: STREAM_BLOCK as u32,
                max_frames_count: STREAM_BLOCK as u32,
            },
        )
        .map_err(|e| format!("Activate live CLAP: {e}"))?;
    let processor = processor
        .start_processing()
        .map_err(|e| format!("Start live CLAP: {e}"))?;
    let mut state = Vec::new();
    state_ext
        .as_ref()
        .ok_or("CLAP plugin has no state extension to save")?
        .save(processor.shared_plugin_handle().as_raw(), &mut state)
        .map_err(|e| format!("Save live CLAP state: {e}"))?;
    let stopped = processor.stop_processing();
    plugin.deactivate(stopped);
    fs::write(state_out, &state).map_err(|e| format!("Write CLAP state file: {e}"))?;
    println!(
        "Saved {} bytes of native CLAP state to {}",
        state.len(),
        state_out.display()
    );
    Ok(())
}

/// Capture the opaque native state of a live CLAP MIDI instrument.
pub fn save_state_instrument(
    path: &Path,
    class_id: &str,
    rate: u32,
    state_in: Option<&Path>,
    state_out: &Path,
) -> Result<(), String> {
    if !(8_000..=192_000).contains(&rate) {
        return Err(format!("Unsupported live CLAP instrument rate: {rate}"));
    }
    let entry = unsafe { PluginEntry::load(path.as_os_str()) }
        .map_err(|e| format!("Load live CLAP {}: {e}", path.display()))?;
    let factory = entry
        .get_factory::<PluginFactory>()
        .ok_or("Plugin bundle has no CLAP factory")?;
    let descriptor = factory
        .plugin_descriptors()
        .find(|desc| {
            desc.id()
                .is_some_and(|id| id.to_bytes() == class_id.as_bytes())
        })
        .ok_or_else(|| format!("Unknown CLAP class: {class_id}"))?;
    if !descriptor
        .features()
        .any(|feature| feature.to_bytes() == b"instrument")
    {
        return Err(format!(
            "CLAP class {class_id} does not declare instrument support"
        ));
    }
    let id = descriptor.id().ok_or("Missing CLAP class ID")?;
    let info = HostInfo::new(
        "Slate Studio",
        "Rustie",
        "https://github.com/TheGoldiestGoldy/Rustie",
        "0.1",
    )
    .map_err(|e| e.to_string())?;
    let mut plugin = PluginInstance::<()>::new(|_| (), |_| (), &entry, id, &info)
        .map_err(|e| format!("Create live CLAP: {e}"))?;
    let shared = plugin.plugin_shared_handle();
    let state_ext = shared.get_extension::<PluginStateExt>();
    if let Some(state) = state_in {
        let bytes = crate::read_native_state(state)?;
        state_ext
            .as_ref()
            .ok_or("CLAP plugin has no state extension to restore")?
            .load(shared.as_raw(), &bytes)
            .map_err(|e| format!("Restore live CLAP state: {e}"))?;
    }
    let processor = plugin
        .activate(
            |_, _| (),
            PluginAudioConfiguration {
                sample_rate: f64::from(rate),
                min_frames_count: STREAM_BLOCK as u32,
                max_frames_count: STREAM_BLOCK as u32,
            },
        )
        .map_err(|e| format!("Activate live CLAP: {e}"))?;
    let processor = processor
        .start_processing()
        .map_err(|e| format!("Start live CLAP: {e}"))?;
    let mut state = Vec::new();
    state_ext
        .as_ref()
        .ok_or("CLAP plugin has no state extension to save")?
        .save(processor.shared_plugin_handle().as_raw(), &mut state)
        .map_err(|e| format!("Save live CLAP state: {e}"))?;
    let stopped = processor.stop_processing();
    plugin.deactivate(stopped);
    fs::write(state_out, &state).map_err(|e| format!("Write CLAP state file: {e}"))?;
    println!(
        "Saved {} bytes of native CLAP instrument state to {}",
        state.len(),
        state_out.display()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stream_block_is_stereo_float_and_power_of_two() {
        assert_eq!(STREAM_BLOCK, 128);
        assert_eq!(BYTES, 1024);
        assert!(STREAM_BLOCK.is_power_of_two());
        assert_eq!(READY_BYTES, 9);
        assert_eq!(STATE_SAVED & STATE_FAILED, 0);
    }
}

#[cfg(test)]
mod midi_tests {
    use super::*;

    #[test]
    fn state_writer_refuses_to_grow_past_the_project_limit() {
        let limit = daw_engine::live_plugin::MAX_PLUGIN_STATE_BYTES;
        let mut bytes = vec![0; limit - 1];
        let mut writer = StateWriter {
            bytes: &mut bytes,
            overflowed: false,
        };
        writer.write_all(b"a").unwrap();
        assert!(writer.write_all(b"b").is_err());
        assert!(writer.overflowed);
        assert_eq!(bytes.len(), limit);
    }

    #[test]
    fn sends_clap_midi_expression_with_channel_and_frame_offset() {
        let mut buffer = EventBuffer::new();
        for (kind, note, velocity) in [(0, 71, 0), (3, 64, 127), (4, 1, 65), (5, 0, 0)] {
            deliver_midi(
                &mut buffer,
                LiveMidiEvent {
                    offset: 17,
                    channel: 5,
                    kind,
                    note,
                    velocity,
                },
                true,
            );
        }
        let messages = buffer
            .iter()
            .map(|event| {
                let midi = event.as_event::<MidiEvent>().unwrap();
                (event.header().time(), midi.data())
            })
            .collect::<Vec<_>>();
        assert_eq!(
            messages,
            [
                (17, [0x85, 71, 0]),
                (17, [0xb5, 64, 127]),
                (17, [0xe5, 1, 65]),
                (17, [0xb5, 64, 0]),
                (17, [0xb5, 123, 0]),
                (17, [0xb5, 120, 0]),
            ]
        );
    }
}
