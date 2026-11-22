//! Fails immediately if render code allocates *or frees* inside the guarded RT section.
//! Tests the engine callback body, not the implementation inside ALSA/JACK/CPAL.
use assert_no_alloc::{AllocDisabler, assert_no_alloc};
use daw_engine::{
    EngineCommand, GraphBuilder, NodeSpec, ScheduledCommand, Source, StereoFrame, create_engine,
};

#[global_allocator]
static ALLOCATOR: AllocDisabler = AllocDisabler;

#[test]
fn all_graph_render_and_automation_operations_are_allocation_free() {
    let mut builder = GraphBuilder::new(48_000);
    let mut slow = NodeSpec::audio(
        1,
        Source::Tone {
            frequency_hz: 440.0,
            phase: 0.0,
        },
    );
    slow.latency_samples = 64;
    builder
        .add_node(slow)
        .add_node(NodeSpec::midi(2))
        .add_node(NodeSpec::returns(3))
        .add_node(NodeSpec::master(99));
    builder
        .route(1, 3, 0.2)
        .route(1, 99, 0.8)
        .route(2, 3, 0.3)
        .route(2, 99, 0.8)
        .route(3, 99, 0.5);
    let (mut controller, mut engine) = create_engine(builder.build().unwrap());
    controller.immediate(EngineCommand::Play).unwrap();
    controller
        .immediate(EngineCommand::NoteOn {
            node: 2,
            note: 60,
            velocity: 100,
        })
        .unwrap();
    controller
        .schedule(ScheduledCommand {
            at_engine_frame: 100,
            command: EngineCommand::SetGain { node: 1, gain: 0.5 },
        })
        .unwrap();
    controller
        .schedule(ScheduledCommand {
            at_engine_frame: 200,
            command: EngineCommand::NoteOff { node: 2, note: 60 },
        })
        .unwrap();
    controller
        .schedule(ScheduledCommand {
            at_engine_frame: 300,
            command: EngineCommand::Seek { project_frame: 0 },
        })
        .unwrap();
    controller
        .schedule(ScheduledCommand {
            at_engine_frame: 400,
            command: EngineCommand::Stop,
        })
        .unwrap();

    let mut output = [0.0_f32; 256];
    assert_no_alloc(|| {
        for _ in 0..100 {
            engine.process_interleaved(&mut output, 2);
        }
    });
}

#[test]
fn loaded_buffers_are_read_without_allocation() {
    let mut builder = GraphBuilder::new(48_000);
    builder
        .add_node(NodeSpec::audio(
            1,
            Source::Samples {
                frames: vec![
                    StereoFrame {
                        left: 0.25,
                        right: -0.25
                    };
                    512
                ]
                .into(),
                start_frame: 0,
            },
        ))
        .add_node(NodeSpec::master(2))
        .route(1, 2, 1.0);
    let (mut controller, mut engine) = create_engine(builder.build().unwrap());
    controller.immediate(EngineCommand::Play).unwrap();
    let mut output = [0.0_f32; 256];
    assert_no_alloc(|| engine.process_interleaved(&mut output, 2));
    assert_eq!(output[0], 0.25);
}

#[test]
fn live_input_monitor_and_metronome_render_without_allocations() {
    use daw_engine::monitor::RecordingMonitor;
    use std::sync::Arc;

    let monitor = Arc::new(RecordingMonitor::new());
    for _ in 0..1024 {
        monitor.write(StereoFrame {
            left: 0.1,
            right: -0.1,
        });
    }
    let mut builder = GraphBuilder::new(48_000);
    builder
        .add_node(NodeSpec::audio(
            1,
            Source::LiveInput {
                monitor: Arc::clone(&monitor),
                cursor: 0,
            },
        ))
        .add_node(NodeSpec::audio(
            2,
            Source::Metronome {
                bpm: 120.0,
                beats_per_bar: 4,
                beat_denominator: 4,
            },
        ))
        .add_node(NodeSpec::master(3));
    builder.route(1, 3, 1.0).route(2, 3, 0.5);
    let (mut controller, mut engine) = create_engine(builder.build().unwrap());
    controller.immediate(EngineCommand::Play).unwrap();
    let mut output = [0.0_f32; 256];
    let mut peak = 0.0_f32;
    assert_no_alloc(|| {
        for _ in 0..200 {
            engine.process_interleaved(&mut output, 2);
            for sample in &output {
                peak = peak.max(sample.abs());
            }
        }
    });
    assert!(peak > 0.001);
}

#[test]
fn built_in_eq_and_reverb_process_in_audio_callback_without_allocations() {
    use daw_core::model::{BuiltinEffect, BuiltinEffectKind};
    use daw_engine::effects::EffectRack;

    let mut builder = GraphBuilder::new(48_000);
    let mut audio = NodeSpec::audio(
        1,
        Source::Tone {
            frequency_hz: 220.0,
            phase: 0.0,
        },
    );
    audio.effects = EffectRack::prepare(
        &[
            BuiltinEffect::new(BuiltinEffectKind::Eq3),
            BuiltinEffect::new(BuiltinEffectKind::Eq8),
            BuiltinEffect::new(BuiltinEffectKind::VocalReverb),
            BuiltinEffect::new(BuiltinEffectKind::GuitarReverb),
        ],
        48_000,
    );
    builder.add_node(audio).add_node(NodeSpec::master(2));
    builder.route(1, 2, 1.0);
    let (mut controller, mut engine) = create_engine(builder.build().unwrap());
    controller.immediate(EngineCommand::Play).unwrap();
    let mut output = [0.0_f32; 256];
    assert_no_alloc(|| {
        for _ in 0..200 {
            engine.process_interleaved(&mut output, 2);
        }
    });
    assert!(output.iter().any(|sample| sample.abs() > 0.001));
}

#[test]
fn session_scene_queue_saturation_replacement_cancel_and_midi_without_ui_callbacks() {
    use daw_core::{Clip, ClipKind, MidiClip, MidiNote, MusicalTime, PPQ, TempoMap};
    use daw_engine::session::{SessionClock, SessionLaunch, SessionVoice, TrackLaunch};
    use std::sync::Arc;

    let rate = 48_000;
    let clock = Arc::new(SessionClock::new(&TempoMap::default(), rate).unwrap());
    let mut builder = GraphBuilder::new(rate);
    let mut meters = Vec::new();
    for (id, track) in [(1, 0), (2, 0), (3, 1), (4, 1)] {
        let mut node = NodeSpec::midi(id);
        let clip = Clip {
            id: id.into(),
            start: MusicalTime::ZERO,
            duration: MusicalTime::from_ticks(PPQ / 4),
            scale_override: None,
            kind: ClipKind::Midi(MidiClip {
                notes: vec![MidiNote {
                    pitch: 69,
                    start: MusicalTime::ZERO,
                    duration: MusicalTime::from_ticks(PPQ),
                    velocity: 96,
                    muted: false,
                }],
            }),
        };
        let voice = SessionVoice::new(clock.clone(), track, &clip, None);
        meters.push(voice.meter());
        node.session = Some(voice);
        builder.add_node(node).route(id, 99, 1.0);
    }
    builder.add_node(NodeSpec::master(99));
    let (mut ui, mut engine) = create_engine(builder.build().unwrap());
    ui.immediate(EngineCommand::Play).unwrap();
    let mut first = SessionLaunch::new(PPQ / 4);
    first.tracks[0] = TrackLaunch::Clip(1);
    first.tracks[1] = TrackLaunch::Clip(3);
    let mut second = first;
    second.tracks[0] = TrackLaunch::Clip(2);
    second.tracks[1] = TrackLaunch::Clip(4);
    for _ in 0..16 {
        ui.launch_session(first).unwrap();
    }
    assert!(ui.launch_session(second).is_err());
    let mut block = vec![0.0; 6000];
    assert_no_alloc(|| engine.process_interleaved(&mut block, 1));
    assert!(block.iter().all(|x| *x == 0.0));
    assert_eq!(meters[0].state(), (false, true, 0));
    assert_eq!(meters[1].state(), (false, false, 0));
    assert_no_alloc(|| engine.process_interleaved(&mut block, 1));
    assert_eq!(meters[0].state(), (true, false, 6000));
    assert_eq!(meters[2].state(), (true, false, 6000));
    assert!(block.iter().any(|x| x.abs() > 0.01));

    // Reach the frame before the next grid, replace a scene, then cancel it.
    ui.launch_session(second).unwrap();
    let mut cancel = first;
    cancel.tracks.fill(TrackLaunch::Cancel);
    ui.launch_session(cancel).unwrap();
    assert_no_alloc(|| engine.process_interleaved(&mut block[..5999], 1));
    assert!(meters[0].state().0 && !meters[1].state().1);
    let mut superseded = first;
    superseded.tracks.fill(TrackLaunch::Stop);
    ui.launch_session(superseded).unwrap();
    ui.launch_session(second).unwrap();
    assert_no_alloc(|| engine.process_interleaved(&mut block[..1], 1));
    assert_eq!(meters[1].state(), (false, true, 0));
    assert_no_alloc(|| engine.process_interleaved(&mut block, 1));
    assert_eq!(meters[1].state(), (true, false, 18000));
    assert_eq!(meters[3].state(), (true, false, 18000));
    assert!(!meters[0].state().0 && !meters[2].state().0);
    for _ in 0..20 {
        assert_no_alloc(|| engine.process_interleaved(&mut block, 1));
        assert!(block.iter().any(|x| x.abs() > 0.01));
    }
    let mut stop = first;
    stop.tracks.fill(TrackLaunch::Stop);
    ui.launch_session(stop).unwrap();
    for _ in 0..3 {
        assert_no_alloc(|| engine.process_interleaved(&mut block, 1));
    }
    assert!(block.iter().all(|x| *x == 0.0));
    assert!(meters.iter().all(|m| !m.state().0));
}

#[test]
fn session_audio_loop_reanchors_at_absolute_ticks_across_tempo_change() {
    use daw_core::{AudioClip, Clip, ClipKind, MusicalTime, PPQ, Tempo, TempoMap};
    use daw_engine::session::{SessionClock, SessionVoice};
    use std::sync::Arc;

    let mut map = TempoMap::new(Tempo::from_bpm(137.125).unwrap());
    map.insert(
        MusicalTime::from_ticks(PPQ + PPQ / 3),
        Tempo::from_bpm(87.25).unwrap(),
    )
    .unwrap();
    let rate = 44_100;
    let clock = Arc::new(SessionClock::new(&map, rate).unwrap());
    let clip = Clip {
        id: 10,
        start: MusicalTime::ZERO,
        duration: MusicalTime::from_ticks(PPQ / 4),
        kind: ClipKind::Audio(AudioClip {
            source: "synthetic".into(),
            warped: false,
            source_offset_frames: 1,
            gain: 1.0,
            fade_in_ms: 0,
            fade_out_ms: 0,
        }),
        scale_override: None,
    };
    let mut node = NodeSpec::audio(
        10,
        Source::LoopedSamples {
            frames: vec![
                StereoFrame {
                    left: 0.5,
                    right: 0.5
                };
                201
            ]
            .into(),
            start_frame: 0,
            loop_frames: 1,
            source_offset_frames: 1,
            gain: 1.0,
            fade_in_frames: 32,
            fade_out_frames: 32,
        },
    );
    node.session = Some(SessionVoice::new(clock, 0, &clip, Some(0)));
    let mut builder = GraphBuilder::new(rate);
    builder
        .add_node(node)
        .add_node(NodeSpec::master(99))
        .route(10, 99, 1.0);
    let (mut ui, mut engine) = create_engine(builder.build().unwrap());
    ui.immediate(EngineCommand::Play).unwrap();
    let frame = |cycle| {
        map.sample_frame_at(MusicalTime::from_ticks(cycle * (PPQ / 4)), rate)
            .unwrap() as usize
    };
    let mut output = vec![0.0; frame(128)];
    assert_no_alloc(|| engine.process_interleaved(&mut output, 1));
    for cycle in 1..128 {
        let at = frame(cycle);
        assert_eq!(output[at - 1], 0.0);
        assert_eq!(output[at], 0.0);
        assert_eq!(output[at + 1], 0.5 / 32.0);
        assert_eq!(output[at + 32], 0.5);
        assert_eq!(output[at + 199], 0.5 / 32.0);
        assert_eq!(output[at + 200], 0.0);
    }
}

#[test]
fn saturated_live_plugin_midi_bridge_is_allocation_free() {
    use daw_engine::live_plugin::{LIVE_BLOCK, LiveMidiMessage, live_plugin_pair};

    let (mut bridge, _worker) = live_plugin_pair();
    assert_no_alloc(|| {
        for _ in 0..128 {
            for note in 0..80 {
                bridge.midi(7, LiveMidiMessage::NoteOn(note, 100));
            }
            bridge.midi(7, LiveMidiMessage::Sustain(0));
            bridge.midi(7, LiveMidiMessage::PitchBend(8192));
            for _ in 0..LIVE_BLOCK {
                bridge.process(StereoFrame::ZERO);
            }
        }
    });
}

#[test]
fn soundfont_swaps_and_plays_a_new_note_without_realtime_heap_activity() {
    use daw_engine::{SoundFontInstrument, build_synth};
    use std::sync::Arc;

    let bytes = include_bytes!("fixtures/qa-two-programs.sf2");
    let bank = Arc::new(rustysynth::SoundFont::new(&mut bytes.as_slice()).unwrap());
    let (instrument, mut handle) = SoundFontInstrument::new(bank.clone(), 48_000, 0, 0).unwrap();
    let mut node = NodeSpec::midi(1);
    node.source = Source::SoundFont {
        instrument: Box::new(instrument),
    };
    let mut builder = GraphBuilder::new(48_000);
    builder.add_node(node).add_node(NodeSpec::master(2));
    builder.route(1, 2, 1.0);
    let (mut controller, mut engine) = create_engine(builder.build().unwrap());
    controller.immediate(EngineCommand::Play).unwrap();
    handle
        .replace(build_synth(&bank, 48_000, 0, 1).unwrap(), 0)
        .unwrap();
    controller
        .immediate(EngineCommand::NoteOn {
            node: 1,
            note: 69,
            velocity: 110,
        })
        .unwrap();

    let mut output = [0.0_f32; 1024];
    assert_no_alloc(|| engine.process_interleaved(&mut output, 2));
    assert!(output.iter().any(|sample| sample.abs() > 0.01));
    handle.reclaim_retired();
}

#[test]
fn arrangement_seek_barriers_and_rebased_midi_are_allocation_free() {
    let mut builder = GraphBuilder::new(48_000);
    builder
        .add_node(NodeSpec::midi(1))
        .add_node(NodeSpec::master(2))
        .route(1, 2, 1.0);
    let (mut controller, mut engine) = create_engine(builder.build().unwrap());
    controller.immediate(EngineCommand::Play).unwrap();
    let mut output = [0.0; 256];
    for target in [0, 40_000, 200, 80_000] {
        controller
            .schedule_arrangement(
                target + 1000,
                EngineCommand::NoteOn {
                    node: 1,
                    note: 60,
                    velocity: 100,
                },
            )
            .unwrap();
        assert_no_alloc(|| engine.process_interleaved(&mut output, 2));
        controller.seek_arrangement(target, target + 1).unwrap();
        controller
            .schedule_arrangement(
                target,
                EngineCommand::NoteOn {
                    node: 1,
                    note: 64,
                    velocity: 100,
                },
            )
            .unwrap();
        controller
            .schedule_arrangement(target + 20, EngineCommand::NoteOff { node: 1, note: 64 })
            .unwrap();
        assert_no_alloc(|| engine.process_interleaved(&mut output, 2));
    }
}
