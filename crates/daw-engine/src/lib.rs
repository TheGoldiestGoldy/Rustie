//! Phase 2 native Linux audio engine. No WINE or proprietary plugin code.
//! Graph is compiled off-thread; one RT thread exclusively owns the render state.
pub mod audio_file;
pub mod driver;
pub mod effects;
pub mod engine;
pub mod graph;
pub mod instruments;
pub mod live_plugin;
pub mod lnf1;
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
    Instrument, SF16_PARTS, Sf16Instrument, Sf16Listen, Sf16PartState, Sf16Settings, SoundFontBank,
    SoundFontHandle, SoundFontInstrument, SoundFontTone, apply_tone, build_synth,
    build_synth_with_tone, load_sound_font,
};
pub use lnf1::{
    LNF1_INSTRUMENT_BANK, Lnf1Instrument, Lnf1Page, Lnf1ParamHandle, Lnf1ParamId, Lnf1ParamSpec,
    Lnf1ParamStore, Lnf1Params, PARAM_SPECS,
};

[executed on device: Johnnysstation (1200896c-45a0-4cb2-8dfa-479e25d803a4)]