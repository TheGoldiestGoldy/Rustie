//! Phase 2 native Linux audio engine. No WINE or proprietary plugin code.
//! Graph is compiled off-thread; one RT thread exclusively owns the render state.
pub mod audio_file;
pub mod driver;
pub mod effects;
pub mod engine;
pub mod graph;
pub mod instruments;
pub mod live_plugin;
pub mod monitor;
pub mod realtime;
pub mod session;

pub use audio_file::{AudioLoadError, decode_to_source};
pub use daw_core::time::{MusicalTime, TempoMap};
pub use driver::{AudioDriver, DriverConfig, DriverError, DriverInfo};
pub use engine::{
    Controller, EngineCommand, EngineTelemetry, QueueError, RenderEngine, ScheduledCommand,
    create_engine,
};
pub use graph::{
    AudioGraph, GraphBuilder, GraphError, NodeKind, NodeSpec, Route, Source, StereoFrame,
};
pub use instruments::{
    Instrument, SoundFontBank, SoundFontHandle, SoundFontInstrument, SoundFontTone, apply_tone,
    build_synth, build_synth_with_tone, load_sound_font,
};
