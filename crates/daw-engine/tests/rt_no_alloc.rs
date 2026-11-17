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
