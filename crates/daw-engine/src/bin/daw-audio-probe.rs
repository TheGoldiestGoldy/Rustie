//! Non-invasive Linux audio diagnostic. Silent by default.
//! Usage:
//!   cargo run -p daw-engine --bin daw-audio-probe
//!   cargo run -p daw-engine --bin daw-audio-probe -- --silent 44100
//!   cargo run -p daw-engine --bin daw-audio-probe -- --tone 48000 pulse
use std::{env, sync::atomic::Ordering, thread, time::Duration};

use daw_engine::{
    AudioDriver, DriverConfig, EngineCommand, GraphBuilder, NodeSpec, Source, driver::Backend,
    driver::enumerate_device_names,
};

fn main() {
    let args = env::args().collect::<Vec<_>>();
    if args.len() == 1 {
        println!("Available outputs (CPAL):");
        for (backend, name) in enumerate_device_names() {
            println!("  {backend}: {name}");
        }
        println!(
            "Device enumeration only; use --silent [sample-rate] to validate an actual stream."
        );
        return;
    }
    let tone = match args[1].as_str() {
        "--silent" => false,
        "--tone" => true,
        other => {
            eprintln!("Unknown mode {other}. Use --silent or --tone.");
            std::process::exit(2);
        }
    };
    let sample_rate = args.get(2).map_or(Ok(44_100), |rate| rate.parse::<u32>());
    let sample_rate = sample_rate.unwrap_or_else(|_| {
        eprintln!("Invalid sample rate");
        std::process::exit(2);
    });
    let backend_preference = match args.get(3).map(String::as_str).unwrap_or("alsa") {
        "pulse" | "pulseaudio" => Backend::PulseAudio,
        "alsa" => Backend::Alsa,
        "jack" => Backend::Jack,
        other => {
            eprintln!("Unknown backend {other}; use pulse, alsa or jack.");
            std::process::exit(2);
        }
    };
    let mut builder = GraphBuilder::new(sample_rate);
    if tone {
        builder
            .add_node(NodeSpec::audio(
                1,
                Source::Tone {
                    frequency_hz: 440.0,
                    phase: 0.0,
                },
            ))
            .route(1, 999, 0.3);
    }
    builder.add_node(NodeSpec::master(999));
    let graph = builder.build().expect("valid probe graph");
    let config = DriverConfig {
        sample_rate,
        backend_preference,
        ..DriverConfig::default()
    };
    let mut driver = AudioDriver::start(graph, &config).unwrap_or_else(|err| {
        eprintln!("Unable to open a stream: {err}");
        std::process::exit(1);
    });
    println!("Selected backend: {}", driver.info().backend);
    println!("Selected device: {}", driver.info().device);
    if driver
        .info()
        .device
        .to_ascii_lowercase()
        .contains("digital stereo")
        || driver.info().device.to_ascii_lowercase().contains("iec958")
    {
        eprintln!(
            "ROUTING WARNING: Current output is digital S/PDIF (IEC958), not analog headphones/monitors."
        );
        eprintln!(
            "If listening through the AIR 192|6 headphone/analog outputs, select its Analog Stereo Duplex profile in your sound settings first."
        );
        eprintln!(
            "The DAW will not change your system profile while other audio apps may be using it."
        );
    }
    println!("Sample rate: {} Hz", driver.info().sample_rate);
    println!("Channels: {}", driver.info().channels);
    println!("RT scheduling: {}", driver.info().rt_scheduling);
    println!(
        "Fixed buffer request: {:?}",
        driver.info().requested_fixed_buffer_frames
    );
    let telemetry = driver.controller().telemetry().clone();
    driver
        .controller()
        .immediate(EngineCommand::Play)
        .expect("queue command");
    thread::sleep(Duration::from_secs(2));
    driver.stop();
    let callbacks = telemetry.callbacks.load(Ordering::Relaxed);
    let rendered = telemetry.rendered_frames.load(Ordering::Relaxed);
    let peak = f32::from_bits(telemetry.output_peak_bits.load(Ordering::Relaxed));
    println!(
        "Callbacks: {callbacks}, rendered output frames: {rendered}, nominal buffer time: {:.2}s",
        rendered as f64 / sample_rate as f64
    );
    println!(
        "Peak generated output: {:.6} ({:.1} dBFS)",
        peak,
        if peak > 0.0 {
            20.0 * peak.log10()
        } else {
            f32::NEG_INFINITY
        }
    );
    println!("Output buffer time can exceed wall-clock time due to PulseAudio prebuffering.");
    println!("XRuns: not instrumented; callback count alone does not prove glitch-free audio.");
    if callbacks == 0 || rendered == 0 {
        eprintln!("FAIL: backend never invoked our render callback");
        std::process::exit(1);
    }
    if tone && peak <= 0.0 {
        eprintln!("FAIL: callback ran but no nonzero tone samples were generated");
        std::process::exit(1);
    }
    if tone {
        println!("PASS: tone samples generated; audible speaker routing is not confirmed");
    } else {
        println!("PASS: native silent stream callback ran successfully");
    }
}
