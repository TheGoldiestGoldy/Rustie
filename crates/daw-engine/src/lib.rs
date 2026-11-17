//! Phase 2 native Linux audio engine. No WINE or proprietary plugin code.
//! Graph is compiled off-thread; one RT thread exclusively owns the render state.
pub mod audio_file;
pub mod driver;
pub mod engine;
pub mod graph;
pub mod realtime;

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
