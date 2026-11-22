mod studio;

/// This string is embedded at compile time, not inferred from a later source checkout.
pub(crate) const RUSTY_DISPLAY_VERSION: &str = concat!("v", env!("CARGO_PKG_VERSION"), "-dev");
pub(crate) const RUSTY_BUILD_SHA: &str = env!("RUSTY_BUILD_SHA");

pub(crate) fn rusty_full_version() -> String {
    format!("Rusty {} ({})", RUSTY_DISPLAY_VERSION, RUSTY_BUILD_SHA)
}

/// Installed builds use the Rusty-branded worker; developer workspaces retain
/// the historical Cargo binary name so existing test commands keep working.
pub(crate) fn plugin_worker_path() -> Result<std::path::PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| format!("Locate plugin worker: {e}"))?;
    let installed = exe.with_file_name("rusty-plugin-host");
    if installed.is_file() {
        Ok(installed)
    } else {
        Ok(exe.with_file_name("daw-plugin-host"))
    }
}

/// Headless preflight: validates the model and decodes its referenced audio.
/// Does not connect to any sound device.
fn check_project_file(path: &std::path::Path) -> Result<(), String> {
    use daw_core::{ClipKind, Project};
    use std::collections::HashSet;
    let bytes = std::fs::read(path).map_err(|e| format!("Cannot read {}: {e}", path.display()))?;
    let mut project: Project =
        serde_json::from_slice(&bytes).map_err(|e| format!("Invalid project JSON: {e}"))?;
    project
        .validate_groups()
        .map_err(|e| format!("Invalid project group routing: {e}"))?;
    project.prepare_session()?;
    let mut ids = HashSet::new();
    let arrangement_clips: usize = project.tracks.iter().map(|t| t.clips.len()).sum();
    let session_clips: usize = project.tracks.iter().map(|t| t.session_slots.len()).sum();
    let mut audio_files = HashSet::new();
    for track in &project.tracks {
        if !ids.insert(track.id) {
            return Err(format!("Repeated track ID {}", track.id));
        }
        for clip in track.all_clips() {
            if !ids.insert(clip.id) {
                return Err(format!("Repeated clip ID {}", clip.id));
            }
            if clip.duration.ticks() <= 0 {
                return Err(format!("Clip {} has a nonpositive duration", clip.id));
            }
            if let ClipKind::Audio(audio) = &clip.kind {
                audio_files.insert(audio.source.clone());
            }
        }
    }
    if project.tracks.is_empty()
        || arrangement_clips + session_clips == 0
        || project.tracks.len() > 48
    {
        return Err("Project has no playable clips or exceeds the prototype's limits".into());
    }
    for path in &audio_files {
        daw_engine::decode_to_source(path, 48_000, 0)
            .map_err(|e| format!("Review audio {}: {e}", path.display()))?;
    }
    println!(
        "Review project valid: {} tracks, {} Arrangement clips, {} Session slots, {} decoded audio files",
        project.tracks.len(),
        arrangement_clips,
        session_clips,
        audio_files.len()
    );
    Ok(())
}

/// Wait for a command to appear in render telemetry from the native sink.
fn wait_for_probe_transport(wait_ms: u64, mut observed: impl FnMut() -> bool) -> Option<()> {
    let timeout = std::time::Duration::from_millis(wait_ms);
    let started = std::time::Instant::now();
    loop {
        if observed() {
            return Some(());
        }
        if started.elapsed() >= timeout {
            return None;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}

fn main() -> eframe::Result {
    let args = std::env::args().collect::<Vec<_>>();
    if args.iter().any(|arg| arg == "--version" || arg == "-V") {
        println!("{}", rusty_full_version());
        return Ok(());
    }
    if let Some(index) = args.iter().position(|arg| arg == "--probe-live-clap") {
        let (Some(bundle), Some(class_id)) = (args.get(index + 1), args.get(index + 2)) else {
            eprintln!("Usage: daw-ui --probe-live-clap BUNDLE.clap CLASS_ID");
            std::process::exit(2);
        };
        if let Err(error) = studio::probe_live_clap(
            std::path::Path::new(bundle),
            class_id,
            studio::Studio::negotiate_sample_rate(),
        ) {
            eprintln!("Live CLAP graph probe failed: {error}");
            std::process::exit(2);
        }
        return Ok(());
    }
    if let Some(index) = args
        .iter()
        .position(|arg| arg == "--probe-live-clap-instrument")
    {
        let (Some(bundle), Some(class_id)) = (args.get(index + 1), args.get(index + 2)) else {
            eprintln!("Usage: daw-ui --probe-live-clap-instrument BUNDLE.clap CLASS_ID");
            std::process::exit(2);
        };
        if let Err(error) = studio::probe_live_clap_instrument(
            std::path::Path::new(bundle),
            class_id,
            studio::Studio::negotiate_sample_rate(),
        ) {
            eprintln!("Live CLAP instrument graph probe failed: {error}");
            std::process::exit(2);
        }
        return Ok(());
    }
    if let Some(index) = args
        .iter()
        .position(|arg| arg == "--probe-live-vst3-instrument")
    {
        let Some(bundle) = args.get(index + 1) else {
            eprintln!("Usage: daw-ui --probe-live-vst3-instrument BUNDLE.vst3");
            std::process::exit(2);
        };
        if let Err(error) = studio::probe_live_clap_instrument(
            std::path::Path::new(bundle),
            "",
            studio::Studio::negotiate_sample_rate(),
        ) {
            eprintln!("Live VST3 instrument graph probe failed: {error}");
            std::process::exit(2);
        }
        return Ok(());
    }
    if let Some(index) = args.iter().position(|arg| arg == "--probe-live-vst3") {
        let Some(bundle) = args.get(index + 1) else {
            eprintln!("Usage: daw-ui --probe-live-vst3 BUNDLE.vst3");
            std::process::exit(2);
        };
        if let Err(error) = studio::probe_live_clap(
            std::path::Path::new(bundle),
            "",
            studio::Studio::negotiate_sample_rate(),
        ) {
            eprintln!("Live VST3 graph probe failed: {error}");
            std::process::exit(2);
        }
        return Ok(());
    }
    if std::env::args().any(|arg| arg == "--probe-ui-rate") {
        println!(
            "Rusty chosen project sample rate: {} Hz",
            studio::Studio::negotiate_sample_rate()
        );
        return Ok(());
    }
    if std::env::args().any(|arg| arg == "--probe-recording") {
        if let Err(error) = studio::probe_live_capture() {
            eprintln!("Live capture probe failed: {error}");
            std::process::exit(2);
        }
        return Ok(());
    }
    if std::env::args().any(|arg| arg == "--probe-recording-writer") {
        if let Err(error) = studio::probe_synthetic_recording() {
            eprintln!("Synthetic recording probe failed: {error}");
            std::process::exit(2);
        }
        return Ok(());
    }
    if let Some(position) = std::env::args().position(|arg| arg == "--probe-midi") {
        let query = std::env::args().nth(position + 1);
        let input = midir::MidiInput::new("Rusty MIDI connection probe").unwrap_or_else(|e| {
            eprintln!("MIDI backend unavailable: {e}");
            std::process::exit(2);
        });
        let ports = input.ports();
        let mut selected = None;
        for (index, port) in ports.iter().enumerate() {
            if let Ok(name) = input.port_name(port) {
                println!("MIDI input [{index}]: {name}");
                if query.as_ref().is_some_and(|needle| name.contains(needle)) {
                    selected = Some(index);
                }
            }
        }
        if ports.is_empty() {
            eprintln!("No MIDI input ports available");
            std::process::exit(2);
        }
        if let Some(query) = query {
            let Some(index) = selected else {
                eprintln!("No MIDI input port matched: {query}");
                std::process::exit(2);
            };
            // Open and close the actual input port without saving MIDI traffic.
            let conn = input
                .connect(&ports[index], "Rusty MIDI probe", |_, _, _| {}, ())
                .unwrap_or_else(|e| {
                    eprintln!("MIDI input connection failed: {e}");
                    std::process::exit(2);
                });
            println!("MIDI input connection OK (no notes were captured)");
            drop(conn);
        }
        return Ok(());
    }
    if std::env::args().any(|arg| arg == "--probe-input") {
        use cpal::traits::{DeviceTrait, HostTrait};
        let rate = studio::Studio::negotiate_sample_rate();
        let host = cpal::host_from_id(cpal::HostId::PulseAudio).unwrap_or_else(|e| {
            eprintln!("PulseAudio input host: {e}");
            std::process::exit(2)
        });
        let devices = host.input_devices().unwrap_or_else(|e| {
            eprintln!("Microphone devices: {e}");
            std::process::exit(2)
        });
        let mut valid = 0;
        for device in devices {
            let name = device.to_string();
            match device.supported_input_configs() {
                Ok(configs) => {
                    for format in configs.filter(|c| {
                        c.min_sample_rate() <= rate
                            && c.max_sample_rate() >= rate
                            && matches!(
                                c.sample_format(),
                                cpal::SampleFormat::F32 | cpal::SampleFormat::I16
                            )
                    }) {
                        valid += 1;
                        println!(
                            "Record-capable: {name} • {} ch • {:?} @ {rate} Hz",
                            format.channels(),
                            format.sample_format()
                        );
                    }
                }
                Err(err) => eprintln!("Cannot inspect {name}: {err}"),
            }
        }
        if valid == 0 {
            eprintln!("No supported PulseAudio recording input at {rate} Hz");
            std::process::exit(2);
        }
        println!(
            "Input probe found {valid} compatible configurations (no microphone audio captured)"
        );
        return Ok(());
    }
    if std::env::args().any(|arg| arg == "--probe-transport") {
        use daw_engine::{
            AudioDriver, DriverConfig, EngineCommand, GraphBuilder, NodeSpec, driver::Backend,
        };
        use std::sync::atomic::Ordering;
        let sample_rate = std::env::var("RUSTIE_PROBE_RATE")
            .ok()
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(48_000);
        let mut builder = GraphBuilder::new(sample_rate);
        builder.add_node(NodeSpec::master(900_000));
        let graph = builder.build().expect("silent master graph");
        let args = std::env::args().collect::<Vec<_>>();
        let requested = args
            .iter()
            .position(|arg| arg == "--probe-transport")
            .and_then(|index| args.get(index + 1))
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(128);
        let wait_ms = args
            .iter()
            .position(|arg| arg == "--probe-transport")
            .and_then(|i| args.get(i + 2))
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(1000)
            .clamp(200, 10_000);
        let config = DriverConfig {
            backend_preference: Backend::PulseAudio,
            buffer_frames: requested,
            sample_rate,
            promote_realtime: false,
            ..DriverConfig::default()
        };
        let stream_open_started = std::time::Instant::now();
        let mut driver = match AudioDriver::start(graph, &config) {
            Ok(driver) => driver,
            Err(err) => {
                eprintln!("Silent transport probe failed: {err}");
                std::process::exit(2);
            }
        };
        let stream_open_ms = stream_open_started.elapsed().as_secs_f64() * 1000.0;
        let play_queued_at = std::time::Instant::now();
        let target = 4 * sample_rate as u64;
        driver
            .controller()
            .immediate(EngineCommand::Seek {
                project_frame: target,
            })
            .expect("Queue is initially empty");
        driver
            .controller()
            .immediate(EngineCommand::Play)
            .expect("Queue is initially empty");
        let first_play_ms = wait_for_probe_transport(wait_ms, || {
            driver.telemetry().project_frame.load(Ordering::Relaxed) > target
        })
        .map(|()| play_queued_at.elapsed().as_secs_f64() * 1000.0);
        std::thread::sleep(
            std::time::Duration::from_millis(wait_ms).saturating_sub(play_queued_at.elapsed()),
        );
        let t = driver.telemetry();
        let generated = t.project_frame.load(Ordering::Relaxed);
        let delay = t.predicted_playback_delay_us.load(Ordering::Relaxed);
        let callbacks = t.callbacks.load(Ordering::Relaxed);
        let rendered = t.rendered_frames.load(Ordering::Relaxed);
        println!(
            "Transport probe: device={}; callbacks={callbacks}; generated_frame={generated}; rendered_frames={rendered}; playback_delay_ms={:.2}; fixed_buffer={:?}",
            driver.info().device,
            delay as f64 / 1000.0,
            driver.info().requested_fixed_buffer_frames
        );
        println!(
            "Transport displayed frame after delay compensation: {}",
            generated.saturating_sub(delay.saturating_mul(sample_rate as u64) / 1_000_000)
        );
        if callbacks < 2 || generated < target + 1000 {
            eprintln!("Transport failed to advance after Play and Seek");
            std::process::exit(2);
        }
        let seek_target = u64::from(sample_rate);
        let seek_queued_at = std::time::Instant::now();
        driver
            .controller()
            .immediate(EngineCommand::Seek {
                project_frame: seek_target,
            })
            .expect("Single transport command fits in the queue");
        let seek_observed = wait_for_probe_transport(wait_ms, || {
            let frame = driver.telemetry().project_frame.load(Ordering::Relaxed);
            (seek_target..seek_target + u64::from(sample_rate)).contains(&frame)
        });
        let seek_ms = seek_queued_at.elapsed().as_secs_f64() * 1000.0;

        let stop_queued_at = std::time::Instant::now();
        let mut previous_frame = driver.telemetry().project_frame.load(Ordering::Relaxed);
        let mut previous_callbacks = driver.telemetry().callbacks.load(Ordering::Relaxed);
        driver
            .controller()
            .immediate(EngineCommand::Stop)
            .expect("Single transport command fits in the queue");
        let hold_observed = wait_for_probe_transport(wait_ms, || {
            let telemetry = driver.telemetry();
            let callbacks = telemetry.callbacks.load(Ordering::Relaxed);
            if callbacks == previous_callbacks {
                return false;
            }
            let frame = telemetry.project_frame.load(Ordering::Relaxed);
            let held = frame == previous_frame;
            previous_frame = frame;
            previous_callbacks = callbacks;
            held
        });
        let hold_ms = stop_queued_at.elapsed().as_secs_f64() * 1000.0;

        let resume_queued_at = std::time::Instant::now();
        driver
            .controller()
            .immediate(EngineCommand::Play)
            .expect("Single transport command fits in the queue");
        let resume_observed = wait_for_probe_transport(wait_ms, || {
            driver.telemetry().project_frame.load(Ordering::Relaxed) > previous_frame
        });
        let resume_ms = resume_queued_at.elapsed().as_secs_f64() * 1000.0;
        let (Some(play_ms), Some(()), Some(()), Some(())) =
            (first_play_ms, seek_observed, hold_observed, resume_observed)
        else {
            eprintln!(
                "Timed out waiting for a transport callback: play={first_play_ms:?}, seek={seek_observed:?}, hold={hold_observed:?}, resume={resume_observed:?}"
            );
            std::process::exit(2);
        };
        println!(
            "Transport callback timing: open_ms={stream_open_ms:.2}; play_ms={play_ms:.2}; seek_ms={seek_ms:.2}; stop_hold_ms={hold_ms:.2}; resume_ms={resume_ms:.2}"
        );
        let _ = driver.controller().immediate(EngineCommand::Stop);
        return Ok(());
    }
    if std::env::args().any(|arg| arg == "--probe-audio") {
        use daw_engine::{AudioDriver, DriverConfig, GraphBuilder, NodeSpec, driver::Backend};
        let mut builder = GraphBuilder::new(48_000);
        builder.add_node(NodeSpec::master(900_000));
        let graph = builder.build().expect("silent graph");
        let config = DriverConfig {
            backend_preference: Backend::PulseAudio,
            promote_realtime: false,
            ..DriverConfig::default()
        };
        match AudioDriver::start(graph, &config) {
            Ok(driver) => {
                std::thread::sleep(std::time::Duration::from_millis(180));
                let callbacks = driver
                    .telemetry()
                    .callbacks
                    .load(std::sync::atomic::Ordering::Relaxed);
                println!(
                    "Silent audio probe: {} / {}; callbacks={callbacks}; fixed_buffer={:?}; playback_delay_ms={:.1}",
                    driver.info().backend,
                    driver.info().device,
                    driver.info().requested_fixed_buffer_frames,
                    driver
                        .telemetry()
                        .predicted_playback_delay_us
                        .load(std::sync::atomic::Ordering::Relaxed) as f64
                        / 1000.0,
                );
                if callbacks == 0 {
                    std::process::exit(2);
                }
            }
            Err(err) => {
                eprintln!("Silent audio probe failed: {err}");
                std::process::exit(2);
            }
        }
        return Ok(());
    }
    if std::env::args().any(|arg| arg == "--smoke-audio") {
        studio::smoke_audio();
        return Ok(());
    }
    if let Some(index) = args.iter().position(|arg| arg == "--probe-soundfont") {
        let Some(path) = args.get(index + 1) else {
            eprintln!("Usage: daw-ui --probe-soundfont PATH.sf2");
            std::process::exit(2);
        };
        let rate = studio::Studio::negotiate_sample_rate();
        match studio::probe_soundfont(std::path::Path::new(path), rate) {
            Ok(peak) => println!("SoundFont probe OK: {} peak={peak:.6}", path),
            Err(error) => {
                eprintln!("SoundFont probe failed: {error}");
                std::process::exit(2);
            }
        }
        return Ok(());
    }
    // A review launcher can supply a saved project while the ordinary app
    // still opens with the built-in starter session. --session is independent.
    let args = std::env::args_os().collect::<Vec<_>>();
    let initial_project = args
        .windows(2)
        .find(|pair| pair[0] == "--project")
        .map(|pair| std::path::PathBuf::from(&pair[1]));
    if let Some(index) = args.iter().position(|arg| arg == "--check-project") {
        let Some(path) = args.get(index + 1) else {
            eprintln!("Usage: daw-ui --check-project PATH");
            std::process::exit(2);
        };
        if let Err(error) = check_project_file(std::path::Path::new(path)) {
            eprintln!("Review preflight failed: {error}");
            std::process::exit(2);
        }
        return Ok(());
    }
    let title = format!("{} | Native Linux DAW", rusty_full_version());
    let native = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title(&title)
            .with_inner_size([1540.0, 940.0])
            .with_min_inner_size([980.0, 640.0]),
        ..Default::default()
    };
    eframe::run_native(
        &title,
        native,
        Box::new(move |cc| Ok(Box::new(studio::Studio::new(cc, initial_project)))),
    )
}
