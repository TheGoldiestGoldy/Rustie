// Experimental realtime CLAP audio insert, isolated from the DAW's callback.
// Audio thread only exchanges fixed-size blocks over lock-free SPSC rings.
// Blocking child-process pipes are exclusively owned by this worker thread.
use std::io::{self, Read};
use std::os::fd::AsRawFd;
use std::process::Stdio;

/// An unresponsive child can stop reading halfway through a block. POLLOUT
/// alone does not guarantee a whole write fits; keep the pipe nonblocking and
/// enforce one absolute deadline for the full packet, including MIDI.
fn write_live_block(input: &mut std::process::ChildStdin, bytes: &[u8], timeout: Duration) -> bool {
    let started = std::time::Instant::now();
    let mut sent = 0;
    while sent < bytes.len() {
        let Some(remaining) = timeout.checked_sub(started.elapsed()) else {
            return false;
        };
        let mut descriptor = libc::pollfd {
            fd: input.as_raw_fd(), events: libc::POLLOUT, revents: 0,
        };
        let millis = remaining.as_millis().clamp(1, i32::MAX as u128) as i32;
        // SAFETY: only polls our owned child stdin file descriptor.
        let ready = unsafe { libc::poll(&mut descriptor, 1, millis) };
        if ready < 0 {
            if io::Error::last_os_error().kind() == io::ErrorKind::Interrupted { continue; }
            return false;
        }
        if ready == 0 || descriptor.revents & libc::POLLOUT == 0 {
            return false;
        }
        match std::io::Write::write(input, &bytes[sent..]) {
            Ok(0) => return false,
            Ok(count) => sent += count,
            Err(e) if matches!(e.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted) => continue,
            Err(_) => return false,
        }
    }
    true
}

/// Read an entire audio block with one absolute deadline. A plugin might
/// write only some bytes and freeze: poll+read_exact would wait forever.
/// The stdout pipe must be nonblocking before calling this function.
fn read_live_block(
    output: &mut std::process::ChildStdout,
    bytes: &mut [u8],
    timeout: Duration,
) -> bool {
    let started = std::time::Instant::now();
    let mut filled = 0usize;
    while filled < bytes.len() {
        let Some(remaining) = timeout.checked_sub(started.elapsed()) else {
            return false;
        };
        let millis = remaining.as_millis().clamp(1, i32::MAX as u128) as i32;
        let mut descriptor = libc::pollfd {
            fd: output.as_raw_fd(), events: libc::POLLIN, revents: 0,
        };
        // SAFETY: poll only observes the owned child stdout fd; no memory is transferred.
        let ready = unsafe { libc::poll(&mut descriptor, 1, millis) };
        if ready < 0 {
            if io::Error::last_os_error().kind() == io::ErrorKind::Interrupted {
                continue;
            }
            return false;
        }
        if ready == 0 || (descriptor.revents & libc::POLLIN) == 0 {
            return false;
        }
        match output.read(&mut bytes[filled..]) {
            Ok(0) => return false,
            Ok(count) => filled += count,
            Err(e) if e.kind() == io::ErrorKind::Interrupted
                || e.kind() == io::ErrorKind::WouldBlock => continue,
            Err(_) => return false,
        }
    }
    true
}

use daw_core::model::ClapLiveInsert;
use daw_core::plugin_state::{FORMAT_CLAP, FORMAT_VST3};
use daw_engine::live_plugin::{
    LIVE_BLOCK, LIVE_MIDI_EVENT_BYTES, LIVE_MIDI_PACKET_BYTES, LiveMidiMessage, LivePluginEffect, live_plugin_pair,
};

static LIVE_STATE_FILE_COUNTER: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);

/// Write a saved state blob to a private temporary file for the child to load
/// at startup. The child reads it before announcing readiness, so the parent
/// removes the file immediately afterwards. Never called from the audio thread.
fn write_live_state_file(bytes: &[u8]) -> Result<PathBuf, String> {
    use std::os::unix::fs::OpenOptionsExt;
    let counter = LIVE_STATE_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "rustie-plugin-state-{}-{counter}.bin",
        std::process::id()
    ));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)
        .map_err(|e| format!("Create plugin state tempfile {}: {e}", path.display()))?;
    let result = file
        .write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|e| format!("Write plugin state tempfile: {e}"));
    drop(file);
    if let Err(err) = result {
        let _ = fs::remove_file(&path);
        return Err(err);
    }
    Ok(path)
}

struct LivePluginRuntime {
    bridge: std::thread::JoinHandle<()>,
    missed_blocks: Arc<std::sync::atomic::AtomicU64>,
    class_id: String,
}

fn prepare_live_clap(
    insert: &ClapLiveInsert,
    rate: u32,
) -> Result<(LivePluginEffect, LivePluginControl, LivePluginRuntime), String> {
    prepare_live_plugin(insert, rate, false)
}

fn prepare_live_instrument(
    insert: &ClapLiveInsert,
    rate: u32,
) -> Result<(LivePluginEffect, LivePluginControl, LivePluginRuntime), String> {
    prepare_live_plugin(insert, rate, true)
}

/// Validate the *identity* as well as the encoding before handing untrusted
/// native state to a third-party worker. Old state without a UID is accepted
/// for backwards compatibility; current saves always include their class ID.
fn decode_state_for_insert(
    insert: &ClapLiveInsert,
    format: &str,
) -> Result<Option<Vec<u8>>, String> {
    let Some(state) = insert.state.as_ref() else {
        return Ok(None);
    };
    let same_class = if format == FORMAT_VST3 {
        state.plugin_uid.eq_ignore_ascii_case(&insert.class_id)
    } else {
        state.plugin_uid == insert.class_id
    };
    if !state.plugin_uid.is_empty() && !same_class {
        return Err(format!(
            "Saved state belongs to plugin '{}' rather than '{}'",
            state.plugin_uid, insert.class_id
        ));
    }
    let bytes = state.decode(format)?;
    Ok((!bytes.is_empty()).then_some(bytes))
}

fn prepare_live_plugin(
    insert: &ClapLiveInsert,
    rate: u32,
    instrument: bool,
) -> Result<(LivePluginEffect, LivePluginControl, LivePluginRuntime), String> {
    if !insert.enabled {
        return Err("Inactive live CLAP insert".into());
    }
    let is_clap = insert.bundle.extension().is_some_and(|ext| ext == "clap");
    let is_vst3 = insert.bundle.extension().is_some_and(|ext| ext == "vst3");
    if !insert.bundle.exists() || !(is_clap || is_vst3) {
        return Err(format!(
            "Live CLAP/VST3 bundle unavailable: {}",
            insert.bundle.display()
        ));
    }
    if is_clap && insert.class_id.trim().is_empty() {
        return Err("Select a CLAP effect class before enabling live processing".into());
    }
    let host = crate::plugin_worker_path()?;
    if !host.is_file() {
        return Err(format!("Live plug-in helper missing: {}", host.display()));
    }

    // A malformed, oversized, wrong-format or wrong-class native state blob
    // is never passed to another plugin. The insert still loads with defaults.
    let state_format = if is_vst3 { FORMAT_VST3 } else { FORMAT_CLAP };
    let state_path = match decode_state_for_insert(insert, state_format) {
        Ok(Some(bytes)) => match write_live_state_file(&bytes) {
            Ok(path) => Some(path),
            Err(err) => {
                eprintln!("Rusty skipped saved plugin state: {err}");
                None
            }
        },
        Ok(None) => None,
        Err(err) => {
            eprintln!("Rusty ignored invalid saved plugin state: {err}");
            None
        }
    };

    let (effect, mut worker) = live_plugin_pair();
    let control = worker.control();
    let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel::<String>(1);
    let path = insert.bundle.clone();
    let class_id = insert.class_id.clone();
    let child_state_path = state_path.clone();
    let bridge = std::thread::Builder::new()
        .name("rustie-live-clap-ipc".into())
        .spawn(move || {
            let mut command = Command::new(host);
            if is_vst3 {
                command
                    .arg(if instrument {
                        "stream-vst3-instrument"
                    } else {
                        "stream-vst3-effect"
                    })
                    .arg(&path)
                    .arg(rate.to_string());
            } else {
                command
                    .arg(if instrument {
                        "stream-clap-instrument"
                    } else {
                        "stream-clap-effect"
                    })
                    .arg(&path)
                    .arg(&class_id)
                    .arg(rate.to_string());
            }
            if is_vst3 {
                command.arg(child_state_path.as_deref().unwrap_or_else(|| Path::new("")));
                command.arg(&class_id);
            } else if let Some(state) = child_state_path.as_ref() {
                command.arg(state);
            }
            let spawn = command
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn();
            let Ok(mut child) = spawn else { return; };
            let (Some(mut input), Some(mut output)) =
                (child.stdin.take(), child.stdout.take())
            else {
                let _ = child.kill();
                let _ = child.wait();
                return;
            };
            // SAFETY: fcntl touches only our owned child stdout descriptor.
            // O_NONBLOCK prevents the final read from blocking after a partial
            // plugin response, even if the plugin stops mid-frame.
            let flags = unsafe { libc::fcntl(output.as_raw_fd(), libc::F_GETFL) };
            let input_flags = unsafe { libc::fcntl(input.as_raw_fd(), libc::F_GETFL) };
            if flags < 0
                || input_flags < 0
                || unsafe {
                    libc::fcntl(output.as_raw_fd(), libc::F_SETFL, flags | libc::O_NONBLOCK)
                } < 0
                || unsafe {
                    libc::fcntl(input.as_raw_fd(), libc::F_SETFL, input_flags | libc::O_NONBLOCK)
                } < 0
            {
                let _ = child.kill();
                let _ = child.wait();
                return;
            }
            const AUDIO_BYTES: usize = LIVE_BLOCK * 2 * 4;
            let mut encoded = [0_u8; 1 + AUDIO_BYTES + LIVE_MIDI_PACKET_BYTES];
            // Every mode now waits for readiness. It proves the plugin loaded,
            // and carries plugin-reported latency into the parent's PDC sum.
            let mut ready = [0_u8; 9];
            if !read_live_block(&mut output, &mut ready, Duration::from_millis(2_500))
                || ready[0] != b'R'
            {
                eprintln!("Live CLAP/VST3 worker did not report successful activation");
                let _ = child.kill();
                let _ = child.wait();
                return;
            }
            let latency = u32::from_le_bytes(ready[1..5].try_into().unwrap());
            let tail = u32::from_le_bytes(ready[5..9].try_into().unwrap());
            worker.control().set_reported_latency(latency, tail);
            let resolved_class = if is_vst3 {
                let mut uid = [0_u8; 32];
                if !read_live_block(&mut output, &mut uid, Duration::from_millis(150))
                    || !uid.iter().all(u8::is_ascii_hexdigit)
                    || (!class_id.is_empty() && !uid.eq_ignore_ascii_case(class_id.as_bytes()))
                {
                    let _ = child.kill();
                    let _ = child.wait();
                    return;
                }
                String::from_utf8(uid.to_vec()).expect("hex VST3 class ID")
            } else {
                class_id
            };
            let _ = ready_tx.send(resolved_class);
            let mut first_block = true;
            let mut last_child_check = std::time::Instant::now();
            while !worker.stopped() {
                let Some(block) = worker.recv() else {
                    if last_child_check.elapsed() >= Duration::from_millis(20) {
                        if !matches!(child.try_wait(), Ok(None)) {
                            break;
                        }
                        last_child_check = std::time::Instant::now();
                    }
                    std::thread::sleep(Duration::from_micros(350));
                    continue;
                };
                let want_state = worker.control().take_state_request();
                encoded[0] = u8::from(want_state);
                for (i, frame) in block.frames.iter().enumerate() {
                    let at = 1 + i * 8;
                    encoded[at..at + 4].copy_from_slice(&frame.left.to_le_bytes());
                    encoded[at + 4..at + 8].copy_from_slice(&frame.right.to_le_bytes());
                }
                let send_size = if instrument {
                    let notes_at = 1 + AUDIO_BYTES;
                    encoded[notes_at] = block.midi_count;
                    for (index, event) in block.midi.iter().enumerate() {
                        let offset = notes_at + 1 + index * LIVE_MIDI_EVENT_BYTES;
                        encoded[offset..offset + LIVE_MIDI_EVENT_BYTES]
                            .copy_from_slice(&event.to_wire());
                    }
                    encoded.len()
                } else {
                    1 + AUDIO_BYTES
                };
                // Do not trust a single POLLOUT to cover all 1,282 bytes.
                // A hung child can leave only a few bytes of pipe capacity.
                if !write_live_block(
                    &mut input,
                    &encoded[..send_size],
                    Duration::from_millis(150),
                ) {
                    eprintln!("Live plugin child stopped consuming audio; using dry fallback");
                    if want_state {
                        worker
                            .control()
                            .deliver_state(Err("Plugin worker stopped accepting audio".into()));
                    }
                    break;
                }
                let timeout = if first_block {
                    Duration::from_millis(2_500)
                } else {
                    Duration::from_millis(150)
                };
                let mut status = [0_u8; 1];
                if !read_live_block(&mut output, &mut status, timeout) {
                    eprintln!("Live plugin worker missed IPC deadline or exited; using dry fallback");
                    if want_state {
                        worker.control().deliver_state(Err(
                            "Plugin worker missed the state deadline".into(),
                        ));
                    }
                    break;
                }
                if !read_live_block(&mut output, &mut encoded[..AUDIO_BYTES], timeout) {
                    eprintln!("Live plugin worker returned a truncated audio block; using dry fallback");
                    if want_state {
                        worker
                            .control()
                            .deliver_state(Err("Plugin worker returned truncated audio".into()));
                    }
                    break;
                }
                first_block = false;
                if want_state {
                    if status[0] & 0x01 != 0 {
                        let mut length = [0_u8; 4];
                        if !read_live_block(
                            &mut output,
                            &mut length,
                            Duration::from_millis(1_000),
                        ) {
                            worker
                                .control()
                                .deliver_state(Err("Plugin state length was truncated".into()));
                            break;
                        }
                        let length = u32::from_le_bytes(length) as usize;
                        if length > daw_engine::live_plugin::MAX_PLUGIN_STATE_BYTES {
                            worker.control().deliver_state(Err(format!(
                                "Plugin state is {length} bytes; limit is {}",
                                daw_engine::live_plugin::MAX_PLUGIN_STATE_BYTES
                            )));
                            break;
                        }
                        let mut state = vec![0_u8; length];
                        if !read_live_block(
                            &mut output,
                            &mut state,
                            Duration::from_millis(1_000),
                        ) {
                            worker
                                .control()
                                .deliver_state(Err("Plugin state payload was truncated".into()));
                            break;
                        }
                        worker.control().deliver_state(Ok(state));
                    } else {
                        worker
                            .control()
                            .deliver_state(Err("Plugin worker could not save native state".into()));
                    }
                }
                let mut processed = block;
                for (i, frame) in processed.frames.iter_mut().enumerate() {
                    let at = i * 8;
                    frame.left = f32::from_le_bytes(encoded[at..at + 4].try_into().unwrap());
                    frame.right = f32::from_le_bytes(encoded[at + 4..at + 8].try_into().unwrap());
                    if !frame.left.is_finite() {
                        frame.left = 0.0;
                    }
                    if !frame.right.is_finite() {
                        frame.right = 0.0;
                    }
                }
                let _ = worker.send(processed);
            }
            drop(input);
            drop(output);
            let _ = child.kill();
            let _ = child.wait();
        })
        .map_err(|e| format!("Start isolated CLAP audio bridge: {e}"))?;
    let ready = ready_rx.recv_timeout(Duration::from_secs(3)).map_err(|_| {
        format!(
            "CLAP/VST3 could not be activated: {} ({})",
            insert.bundle.display(),
            insert.class_id
        )
    });
    if let Some(path) = state_path.as_ref() {
        let _ = fs::remove_file(path);
    }
    let runtime = LivePluginRuntime {
        class_id: ready?,
        bridge,
        missed_blocks: effect.missed_counter(),
    };
    Ok((effect, control, runtime))
}

#[cfg(test)]
mod live_clap_tests {
    use super::*;
    #[test]
    fn rejects_missing_clap_without_starting_child() {
        let config = ClapLiveInsert {
            bundle: PathBuf::from("/no/such/rustie-live-plugin.clap"),
            class_id: "example".into(),
            enabled: true,
            state: None,
        };
        assert!(prepare_live_clap(&config, 44_100).is_err());
    }

    #[test]
    fn native_state_rejects_another_plugins_identity_before_worker_spawn() {
        use daw_core::plugin_state::{FORMAT_CLAP, FORMAT_VST3, PluginStateBlob};
        let mut insert = ClapLiveInsert {
            bundle: PathBuf::from("/not/a/real/plugin.clap"),
            class_id: "our.class".to_owned(),
            enabled: true,
            state: Some(
                PluginStateBlob::from_bytes(FORMAT_CLAP, "foreign.class", 44_100, b"state")
                    .unwrap(),
            ),
        };
        assert!(decode_state_for_insert(&insert, FORMAT_CLAP)
            .unwrap_err()
            .contains("foreign.class"));
        assert!(decode_state_for_insert(&insert, FORMAT_VST3).is_err());

        insert.state.as_mut().unwrap().plugin_uid = "our.class".to_owned();
        assert_eq!(
            decode_state_for_insert(&insert, FORMAT_CLAP).unwrap(),
            Some(b"state".to_vec())
        );

        // Old blobs without a UID remain accepted when the bytes are valid.
        insert.state.as_mut().unwrap().plugin_uid.clear();
        assert_eq!(
            decode_state_for_insert(&insert, FORMAT_CLAP).unwrap(),
            Some(b"state".to_vec())
        );
        insert.state.as_mut().unwrap().data_hex = "definitely-not-hex".into();
        assert!(decode_state_for_insert(&insert, FORMAT_CLAP).is_err());

        insert.state = None;
        assert!(decode_state_for_insert(&insert, FORMAT_CLAP).unwrap().is_none());
    }
}

/// Safe device-free end-to-end performance probe of the actual isolated live
/// CLAP/VST3 worker + bounded graph bridge. This does NOT produce speaker sound.
pub(crate) fn probe_live_clap(path: &Path, class_id: &str, rate: u32) -> Result<(), String> {
    let binding = ClapLiveInsert {
        bundle: path.to_path_buf(),
        class_id: class_id.into(),
        enabled: true,
        state: None,
    };
    let (effect, control, _runtime) = prepare_live_clap(&binding, rate)?;
    let missed = effect.missed_counter();
    let reported_latency = control.latency_samples() as usize;
    let mut builder = GraphBuilder::new(rate);
    let mut generator = NodeSpec::audio(
        9,
        Source::Tone { frequency_hz: 440.0, phase: 0.0 }
    );
    generator.effects.add_live(effect);
    generator.latency_samples = daw_engine::live_plugin::LIVE_DELAY + reported_latency;
    builder.add_node(generator).add_node(NodeSpec::master(MASTER_ID));
    builder.route(9, MASTER_ID, 1.0);
    let mut graph = builder.build().map_err(|e| format!("Live CLAP graph: {e:?}"))?;
    let blocks = 400_u64;
    let frames_per_block = LIVE_BLOCK as u64;
    let block_time = Duration::from_secs_f64(frames_per_block as f64 / f64::from(rate));
    let mut peak = 0.0_f32;
    for block in 0..blocks {
        let started = std::time::Instant::now();
        for frame in block * frames_per_block..(block + 1) * frames_per_block {
            let output = graph.render_frame(frame);
            peak = peak.max(output.left.abs()).max(output.right.abs());
        }
        if let Some(rest) = block_time.checked_sub(started.elapsed()) {
            std::thread::sleep(rest);
        }
    }
    let deadline_misses = missed.load(Ordering::Relaxed);
    println!("Live plugin graph probe: {blocks} blocks, {rate} Hz, peak {peak:.5}, dry fallback blocks {deadline_misses}, bridge latency {} samples, plugin-reported latency {reported_latency} samples", daw_engine::live_plugin::LIVE_DELAY);
    if peak < 0.001 {
        return Err("Live plugin graph generated no nonzero samples".into());
    }
    if deadline_misses > blocks / 2 {
        return Err(format!("Too many late plugin effect blocks: {deadline_misses}/{blocks}; plugin not reliable enough for live use"));
    }
    Ok(())
}


/// Synthetic scheduled MIDI performance through the real isolated CLAP instrument.
/// Headless: verifies output from the MIDI graph without touching any sound device.
pub(crate) fn probe_live_clap_instrument(path: &Path, class_id: &str, rate: u32) -> Result<(), String> {
    let binding = ClapLiveInsert {
        bundle: path.to_path_buf(),
        class_id: class_id.into(),
        enabled: true,
        state: None,
    };
    let (instrument, control, _runtime) = prepare_live_instrument(&binding, rate)?;
    let misses = instrument.missed_counter();
    let reported_latency = control.latency_samples() as usize;
    let mut builder = GraphBuilder::new(rate);
    let mut spec = NodeSpec::midi(9);
    spec.source = Source::Silence;
    spec.effects.add_live(instrument);
    spec.latency_samples = daw_engine::live_plugin::LIVE_DELAY + reported_latency;
    builder.add_node(spec).add_node(NodeSpec::master(MASTER_ID));
    builder.route(9, MASTER_ID, 1.0);
    let mut graph = builder.build().map_err(|e| format!("CLAP instrument graph: {e:?}"))?;
    let mut performance = [
        (17, LiveMidiMessage::NoteOn(60, 105)),
        (rate as u64 / 4, LiveMidiMessage::Sustain(127)),
        (rate as u64 / 3, LiveMidiMessage::PitchBend(12_288)),
        (rate as u64 / 2, LiveMidiMessage::NoteOff(60)),
        (rate as u64 * 3 / 4, LiveMidiMessage::Sustain(0)),
        (rate as u64, LiveMidiMessage::Panic),
    ].into_iter().peekable();
    let blocks = 550_u64;
    let block_time = Duration::from_secs_f64(LIVE_BLOCK as f64 / f64::from(rate));
    let mut peak = 0.0_f32;
    for block in 0..blocks {
        let started = std::time::Instant::now();
        for offset in 0..LIVE_BLOCK {
            let frame = block * LIVE_BLOCK as u64 + offset as u64;
            if let Some((_, message)) = performance.next_if(|(at, _)| *at == frame) {
                graph.live_midi(9, 0, message);
            }
            let output = graph.render_frame(frame);
            peak = peak.max(output.left.abs()).max(output.right.abs());
        }
        if let Some(rest) = block_time.checked_sub(started.elapsed()) {
            std::thread::sleep(rest);
        }
    }
    let missed = misses.load(Ordering::Relaxed);
    println!("Live MIDI instrument graph: {blocks} blocks, {rate} Hz, peak {peak:.5}, missed blocks {missed}, bridge latency {} samples, plugin-reported latency {reported_latency} samples", daw_engine::live_plugin::LIVE_DELAY);
    if peak < 0.002 { return Err("Realtime CLAP MIDI instrument produced no sound".into()); }
    if missed > blocks / 2 {
        return Err(format!("Realtime CLAP instrument missed {missed}/{blocks} blocks"));
    }
    Ok(())
}


#[cfg(test)]
mod live_worker_pipe_safety_tests {
    use super::*;
    #[test]
    fn full_unread_child_pipe_times_out_without_blocking_thread() {
        let mut child = Command::new("sh").arg("-c").arg("sleep 3")
            .stdin(Stdio::piped()).spawn().expect("spawn unread pipe");
        let mut input = child.stdin.take().unwrap();
        let flags = unsafe { libc::fcntl(input.as_raw_fd(), libc::F_GETFL) };
        assert!(flags >= 0);
        assert!(unsafe { libc::fcntl(input.as_raw_fd(), libc::F_SETFL, flags | libc::O_NONBLOCK) } >= 0);
        let data = vec![0u8; 1024 * 1024];
        let start = std::time::Instant::now();
        assert!(!write_live_block(&mut input, &data, Duration::from_millis(100)));
        assert!(start.elapsed() < Duration::from_secs(2));
        let _ = child.kill();
        let _ = child.wait();
    }
}
