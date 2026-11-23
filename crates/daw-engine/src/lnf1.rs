//! LNF-1: an original, real-time-safe piano-style instrument foundation.
//!
//! This module deliberately does not contain Lost N' Found Piano or mda sample
//! data. It provides the stable parameter and interaction contract needed by
//! the LNF-1 device UI, plus an original two-mode physical-style voice engine.
//! Sample-parity data may only be added after its provenance is approved.

use core::f32::consts::TAU;
use std::sync::{
    Arc,
    atomic::{AtomicU32, Ordering},
};

use crate::instruments::Instrument;

pub const PARAM_COUNT: usize = 20;
pub const MAX_VOICES: usize = 32;
/// Persisted `BuiltinInstrument::bank` marker for Rusty's native LNF-1 source.
/// It is not a file path and never triggers SoundFont discovery.
pub const LNF1_INSTRUMENT_BANK: &str = "lnf-1";

/// Stable IDs intentionally match the documented LNF import names.
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Lnf1ParamId {
    Instrument,
    FineTuning,
    RandomDetuning,
    StretchTuning,
    EnvDecay,
    EnvRelease,
    VelocitySensitivity,
    Hardness,
    VelocityToHardness,
    MufflingFilter,
    VelocityToMuffling,
    Modulation,
    LfoRate,
    TrebleBoost,
    Overdrive,
    StereoWidth,
    ReverbSize,
    ReverbDamp,
    ReverbMix,
    OutputLevel,
}

impl Lnf1ParamId {
    pub const ALL: [Self; PARAM_COUNT] = [
        Self::Instrument,
        Self::FineTuning,
        Self::RandomDetuning,
        Self::StretchTuning,
        Self::EnvDecay,
        Self::EnvRelease,
        Self::VelocitySensitivity,
        Self::Hardness,
        Self::VelocityToHardness,
        Self::MufflingFilter,
        Self::VelocityToMuffling,
        Self::Modulation,
        Self::LfoRate,
        Self::TrebleBoost,
        Self::Overdrive,
        Self::StereoWidth,
        Self::ReverbSize,
        Self::ReverbDamp,
        Self::ReverbMix,
        Self::OutputLevel,
    ];

    pub const fn index(self) -> usize {
        self as usize
    }

    pub const fn key(self) -> &'static str {
        match self {
            Self::Instrument => "instrument",
            Self::FineTuning => "fineTuning",
            Self::RandomDetuning => "randomDetuning",
            Self::StretchTuning => "stretchTuning",
            Self::EnvDecay => "envDecay",
            Self::EnvRelease => "envRelease",
            Self::VelocitySensitivity => "velocitySensitivity",
            Self::Hardness => "hardness",
            Self::VelocityToHardness => "velocityToHardness",
            Self::MufflingFilter => "mufflingFilter",
            Self::VelocityToMuffling => "velocityToMuffling",
            Self::Modulation => "modulation",
            Self::LfoRate => "lfoRate",
            Self::TrebleBoost => "trebleBoost",
            Self::Overdrive => "overdrive",
            Self::StereoWidth => "stereoWidth",
            Self::ReverbSize => "reverbSize",
            Self::ReverbDamp => "reverbDamp",
            Self::ReverbMix => "reverbMix",
            Self::OutputLevel => "outputLevel",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Lnf1ParamSpec {
    pub id: Lnf1ParamId,
    pub name: &'static str,
    pub min: f32,
    pub max: f32,
    pub step: f32,
    pub default: f32,
}

macro_rules! spec {
    ($id:ident, $name:literal, $min:expr, $max:expr, $step:expr, $default:expr) => {
        Lnf1ParamSpec {
            id: Lnf1ParamId::$id,
            name: $name,
            min: $min,
            max: $max,
            step: $step,
            default: $default,
        }
    };
}

pub const PARAM_SPECS: [Lnf1ParamSpec; PARAM_COUNT] = [
    spec!(Instrument, "instrument", 0.0, 1.0, 1.0, 0.0),
    spec!(FineTuning, "fine", -50.0, 50.0, 0.1, 0.0),
    spec!(RandomDetuning, "random", 0.0, 1.0, 0.001, 0.0),
    spec!(StretchTuning, "stretch", -50.0, 50.0, 0.1, 0.0),
    spec!(EnvDecay, "decay", 0.0, 100.0, 1.0, 50.0),
    spec!(EnvRelease, "release", 0.0, 100.0, 1.0, 50.0),
    spec!(VelocitySensitivity, "vel sens", 0.0, 100.0, 1.0, 33.0),
    spec!(Hardness, "hardness", -50.0, 50.0, 0.1, 0.0),
    spec!(VelocityToHardness, "vel→hard", 0.0, 100.0, 1.0, 50.0),
    spec!(MufflingFilter, "muffle", 0.0, 100.0, 1.0, 0.0),
    spec!(VelocityToMuffling, "vel→muffle", 0.0, 100.0, 1.0, 50.0),
    spec!(Modulation, "mod", 0.0, 1.0, 0.001, 0.5),
    spec!(LfoRate, "rate", 0.0, 1.0, 0.001, 0.0),
    spec!(TrebleBoost, "eq", -50.0, 50.0, 1.0, 0.0),
    spec!(Overdrive, "drive", 0.0, 100.0, 0.1, 0.0),
    spec!(StereoWidth, "width", 0.0, 200.0, 1.0, 100.0),
    spec!(ReverbSize, "size", 0.0, 10.0, 0.01, 7.0),
    spec!(ReverbDamp, "hf damp", 0.0, 100.0, 0.01, 70.0),
    spec!(ReverbMix, "mix", 0.0, 100.0, 0.01, 90.0),
    spec!(OutputLevel, "level", -24.0, 0.0, 0.01, -12.0),
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Lnf1Page {
    Tune,
    Envelope,
    Tone,
    Fx,
    Room,
}

impl Lnf1Page {
    pub const ALL: [Self; 5] = [Self::Tune, Self::Envelope, Self::Tone, Self::Fx, Self::Room];

    /// Encoder order is always blue, green, white, orange. `None` is rendered
    /// as the disabled fourth encoder on the original device layout.
    pub const fn controls(self, electric: bool) -> [Option<Lnf1ParamId>; 4] {
        use Lnf1ParamId as P;
        match self {
            Self::Tune => [
                Some(P::FineTuning),
                Some(P::RandomDetuning),
                Some(P::StretchTuning),
                None,
            ],
            Self::Envelope => [
                Some(P::EnvDecay),
                Some(P::EnvRelease),
                Some(P::VelocitySensitivity),
                None,
            ],
            Self::Tone if electric => [
                Some(P::Hardness),
                Some(P::VelocityToHardness),
                Some(P::Modulation),
                Some(P::LfoRate),
            ],
            Self::Tone => [
                Some(P::Hardness),
                Some(P::VelocityToHardness),
                Some(P::MufflingFilter),
                Some(P::VelocityToMuffling),
            ],
            Self::Fx => [
                Some(P::TrebleBoost),
                Some(P::Overdrive),
                Some(P::StereoWidth),
                None,
            ],
            Self::Room => [
                Some(P::ReverbSize),
                Some(P::ReverbDamp),
                Some(P::ReverbMix),
                Some(P::OutputLevel),
            ],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Lnf1Params {
    values: [f32; PARAM_COUNT],
}

impl Default for Lnf1Params {
    fn default() -> Self {
        Self {
            values: core::array::from_fn(|i| PARAM_SPECS[i].default),
        }
    }
}

impl Lnf1Params {
    pub fn from_values(values: [f32; PARAM_COUNT]) -> Self {
        let mut params = Self::default();
        for (index, value) in values.into_iter().enumerate() {
            params.set(Lnf1ParamId::ALL[index], value);
        }
        params
    }
    pub fn get(&self, id: Lnf1ParamId) -> f32 {
        self.values[id.index()]
    }
    pub fn set(&mut self, id: Lnf1ParamId, value: f32) {
        let spec = PARAM_SPECS[id.index()];
        self.values[id.index()] = value.clamp(spec.min, spec.max);
    }
    pub fn reset_page(&mut self, page: Lnf1Page, electric: bool) {
        for id in page.controls(electric).into_iter().flatten() {
            self.set(id, PARAM_SPECS[id.index()].default);
        }
    }
    pub const fn values(&self) -> &[f32; PARAM_COUNT] {
        &self.values
    }
}

/// UI-to-audio parameter bridge. It uses only relaxed atomics; the audio
/// thread snapshots it at the start of rendering and never waits or allocates.
#[derive(Debug)]
pub struct Lnf1ParamStore {
    values: [AtomicU32; PARAM_COUNT],
    changed: AtomicU32,
}

pub type Lnf1ParamHandle = Arc<Lnf1ParamStore>;

impl Lnf1ParamStore {
    pub fn new(params: Lnf1Params) -> Lnf1ParamHandle {
        Arc::new(Self {
            values: core::array::from_fn(|index| AtomicU32::new(params.values[index].to_bits())),
            changed: AtomicU32::new(0),
        })
    }

    pub fn set_params(&self, params: Lnf1Params) {
        for (index, value) in params.values.into_iter().enumerate() {
            self.values[index].store(value.to_bits(), Ordering::Relaxed);
        }
        self.changed
            .store(u32::MAX >> (32 - PARAM_COUNT), Ordering::Release);
    }

    fn take_snapshot(&self) -> Option<Lnf1Params> {
        if self.changed.swap(0, Ordering::AcqRel) == 0 {
            return None;
        }
        Some(Lnf1Params::from_values(core::array::from_fn(|index| {
            f32::from_bits(self.values[index].load(Ordering::Relaxed))
        })))
    }
}

#[derive(Clone, Copy, Debug)]
struct Voice {
    active: bool,
    held: bool,
    note: u8,
    phase: f32,
    phase_inc: f32,
    env: f32,
    decay: f32,
    release: f32,
    pan: f32,
    brightness: f32,
    lowpass: f32,
    lowpass_coefficient: f32,
}

impl Default for Voice {
    fn default() -> Self {
        Self {
            active: false,
            held: false,
            note: 0,
            phase: 0.0,
            phase_inc: 0.0,
            env: 0.0,
            decay: 1.0,
            release: 1.0,
            pan: 0.0,
            brightness: 0.0,
            lowpass: 0.0,
            lowpass_coefficient: 0.2,
        }
    }
}

#[derive(Clone, Debug)]
struct Ambience {
    buffers: [[f32; 1024]; 4],
    index: [usize; 4],
    damp_state: f32,
}

impl Default for Ambience {
    fn default() -> Self {
        Self {
            buffers: [[0.0; 1024]; 4],
            index: [0; 4],
            damp_state: 0.0,
        }
    }
}

impl Ambience {
    fn reset(&mut self) {
        *self = Self::default();
    }

    fn process(&mut self, input: f32, size: f32, damp: f32) -> f32 {
        let size = (0.025 + size.clamp(0.0, 10.0) * 0.2665).min(2.69);
        let lengths = [107_usize, 142, 277, 379];
        let damp_coefficient = 0.05 + (1.0 - damp.clamp(0.0, 100.0) / 100.0) * 0.9;
        self.damp_state += damp_coefficient * (input - self.damp_state);
        let mut signal = self.damp_state;
        for stage in 0..4 {
            let delay = (lengths[stage] as f32 * size).round().clamp(1.0, 1023.0) as usize;
            let read = (self.index[stage] + 1024 - delay) & 1023;
            let delayed = self.buffers[stage][read];
            let output = -signal + delayed;
            self.buffers[stage][self.index[stage]] = signal + delayed * 0.8;
            self.index[stage] = (self.index[stage] + 1) & 1023;
            signal = output;
        }
        if signal.abs() < 1.0e-10 { 0.0 } else { signal }
    }
}

/// Original two-mode piano instrument. All buffers live in the struct and its
/// render/midi methods allocate neither memory nor synchronization primitives.
#[derive(Debug)]
pub struct Lnf1Instrument {
    sample_rate: f32,
    params: Lnf1Params,
    voices: [Voice; MAX_VOICES],
    sustain: bool,
    mod_wheel: f32,
    lfo_phase: f32,
    ambience: Ambience,
    output_gain: f32,
    param_store: Option<Lnf1ParamHandle>,
}

impl Lnf1Instrument {
    pub fn new(sample_rate: u32) -> Self {
        let mut instrument = Self {
            sample_rate: sample_rate.max(8_000) as f32,
            params: Lnf1Params::default(),
            voices: [Voice::default(); MAX_VOICES],
            sustain: false,
            mod_wheel: 0.0,
            lfo_phase: 0.0,
            ambience: Ambience::default(),
            output_gain: 0.0,
            param_store: None,
        };
        instrument.output_gain = instrument.target_output_gain();
        instrument
    }

    pub fn with_param_store(sample_rate: u32, store: Lnf1ParamHandle) -> Self {
        let mut instrument = Self::new(sample_rate);
        instrument.params = store.take_snapshot().unwrap_or_else(|| {
            Lnf1Params::from_values(core::array::from_fn(|index| {
                f32::from_bits(store.values[index].load(Ordering::Relaxed))
            }))
        });
        instrument.output_gain = instrument.target_output_gain();
        instrument.param_store = Some(store);
        instrument
    }

    pub fn params(&self) -> &Lnf1Params {
        &self.params
    }
    pub fn set_param(&mut self, id: Lnf1ParamId, value: f32) {
        self.params.set(id, value);
    }
    pub fn set_params(&mut self, params: Lnf1Params) {
        self.params = params;
    }
    pub fn lfo_phase(&self) -> f32 {
        self.lfo_phase
    }
    pub fn active_voices(&self) -> usize {
        self.voices.iter().filter(|voice| voice.active).count()
    }

    fn electric(&self) -> bool {
        self.params.get(Lnf1ParamId::Instrument) >= 0.5
    }
    fn target_output_gain(&self) -> f32 {
        10.0_f32.powf(self.params.get(Lnf1ParamId::OutputLevel) / 20.0)
    }

    fn voice_index_for(&self) -> usize {
        self.voices
            .iter()
            .position(|voice| !voice.active)
            .unwrap_or_else(|| {
                self.voices
                    .iter()
                    .enumerate()
                    .min_by(|(_, a), (_, b)| {
                        a.env
                            .partial_cmp(&b.env)
                            .unwrap_or(core::cmp::Ordering::Equal)
                    })
                    .map(|(index, _)| index)
                    .unwrap_or(0)
            })
    }

    fn note_frequency(&self, note: u8) -> f32 {
        let p = &self.params;
        let deterministic = ((u32::from(note)
            .wrapping_mul(1_103_515_245)
            .wrapping_add(12_345)
            >> 16)
            & 0x7fff) as f32
            / 32_767.0;
        let random_cents =
            (deterministic * 2.0 - 1.0) * 50.0 * p.get(Lnf1ParamId::RandomDetuning).powi(2);
        let stretch = if note > 60 {
            p.get(Lnf1ParamId::StretchTuning) * (f32::from(note) - 60.0) / 67.0
        } else {
            0.0
        };
        let semitones = f32::from(note) - 69.0
            + (p.get(Lnf1ParamId::FineTuning) + random_cents + stretch) / 100.0;
        440.0 * 2.0_f32.powf(semitones / 12.0)
    }

    fn start_note(&mut self, note: u8, velocity: u8) {
        let index = self.voice_index_for();
        let velocity = f32::from(velocity) / 127.0;
        let p = &self.params;
        let velocity_sensitivity = p.get(Lnf1ParamId::VelocitySensitivity) / 100.0;
        let level = (1.0 - velocity_sensitivity) * 0.75 + velocity_sensitivity * velocity;
        let lower_note_seconds = (72.0 - f32::from(note)).max(0.0) / 72.0;
        let decay_seconds =
            0.45 + (p.get(Lnf1ParamId::EnvDecay) / 100.0) * 5.0 + lower_note_seconds * 2.0;
        let release_seconds = 0.025 + (p.get(Lnf1ParamId::EnvRelease) / 100.0) * 1.8;
        let hard = (p.get(Lnf1ParamId::Hardness) / 50.0
            + (velocity - 0.5) * 2.0 * p.get(Lnf1ParamId::VelocityToHardness) / 100.0)
            .clamp(-1.0, 1.0);
        let muffle = (p.get(Lnf1ParamId::MufflingFilter) / 100.0
            - velocity * p.get(Lnf1ParamId::VelocityToMuffling) / 100.0)
            .clamp(0.0, 1.0);
        let cutoff = 0.03 + (1.0 - muffle) * 0.55;
        let width = (p.get(Lnf1ParamId::StereoWidth) / 100.0).clamp(0.0, 2.0);
        let pan = ((f32::from(note) - 60.0) / 30.0 * width).clamp(-1.0, 1.0);
        self.voices[index] = Voice {
            active: true,
            held: true,
            note,
            phase: 0.0,
            phase_inc: TAU * self.note_frequency(note) / self.sample_rate,
            env: level,
            decay: (-1.0 / (decay_seconds * self.sample_rate)).exp(),
            release: (-1.0 / (release_seconds * self.sample_rate)).exp(),
            pan,
            brightness: (hard + 1.0) * 0.5,
            lowpass: 0.0,
            lowpass_coefficient: cutoff,
        };
    }

    fn release_note(&mut self, note: u8) {
        for voice in &mut self.voices {
            if voice.active && voice.note == note {
                voice.held = false;
            }
        }
    }

    fn render_voice(voice: &mut Voice, electric: bool, drive: f32, treble: f32) -> (f32, f32) {
        let fundamental = voice.phase.sin();
        let partial = (voice.phase * if electric { 2.01 } else { 2.0 }).sin();
        let high = (voice.phase * if electric { 3.98 } else { 3.0 }).sin();
        let mut raw = fundamental * (0.78 - voice.brightness * 0.18)
            + partial * (0.08 + voice.brightness * 0.28)
            + high * (0.01 + voice.brightness * 0.10);
        if electric {
            let positive = raw.max(0.0);
            raw += (positive - positive * positive * positive / 3.0) * drive * 0.9;
        } else {
            raw *= 1.0 + drive * 0.3;
        }
        voice.lowpass += voice.lowpass_coefficient * (raw - voice.lowpass);
        let sample =
            (voice.lowpass + (raw - voice.lowpass) * (treble * 0.5)).clamp(-1.2, 1.2) * voice.env;
        voice.phase = (voice.phase + voice.phase_inc).rem_euclid(TAU);
        (
            sample * (1.0 - voice.pan) * 0.5,
            sample * (1.0 + voice.pan) * 0.5,
        )
    }
}

impl Instrument for Lnf1Instrument {
    fn render(&mut self, left: &mut [f32], right: &mut [f32]) {
        debug_assert_eq!(left.len(), right.len());
        if let Some(store) = &self.param_store
            && let Some(params) = store.take_snapshot()
        {
            self.params = params;
        }
        let electric = self.electric();
        let drive = self.params.get(Lnf1ParamId::Overdrive) / 100.0;
        let treble = self.params.get(Lnf1ParamId::TrebleBoost) / 50.0;
        let modulation = self.params.get(Lnf1ParamId::Modulation);
        let lfo_hz = (6.22 * self.params.get(Lnf1ParamId::LfoRate) - 2.61).exp();
        let mix = self.params.get(Lnf1ParamId::ReverbMix) / 100.0;
        let size = self.params.get(Lnf1ParamId::ReverbSize);
        let damp = self.params.get(Lnf1ParamId::ReverbDamp);
        let output_target = self.target_output_gain();
        let output_step = (output_target - self.output_gain) / (self.sample_rate * 0.020).max(1.0);
        for (out_l, out_r) in left.iter_mut().zip(right.iter_mut()) {
            let mut l = 0.0;
            let mut r = 0.0;
            for voice in &mut self.voices {
                if !voice.active {
                    continue;
                }
                let (vl, vr) = Self::render_voice(voice, electric, drive, treble);
                l += vl;
                r += vr;
                voice.env *= if voice.held || self.sustain {
                    voice.decay
                } else {
                    voice.release
                };
                if voice.env < 0.00005 {
                    voice.active = false;
                }
            }
            if electric {
                let lfo = self.lfo_phase.sin();
                let depth = modulation * (0.25 + self.mod_wheel * 0.75);
                if modulation >= 0.5 {
                    let gain = 1.0 - depth * (lfo * 0.5 + 0.5);
                    l *= gain;
                    r *= gain;
                } else {
                    l *= 1.0 - depth * (lfo * 0.5 + 0.5);
                    r *= 1.0 + depth * (lfo * 0.5 + 0.5);
                }
                self.lfo_phase = (self.lfo_phase + TAU * lfo_hz / self.sample_rate).rem_euclid(TAU);
            }
            let wet = self.ambience.process((l + r) * 0.5, size, damp);
            self.output_gain += output_step;
            *out_l = (l * (1.0 - mix * mix) + wet * mix * 0.8) * self.output_gain;
            *out_r = (r * (1.0 - mix * mix) + wet * mix * 0.8) * self.output_gain;
        }
    }

    fn note_on(&mut self, note: u8, velocity: u8) {
        if velocity == 0 {
            self.release_note(note);
        } else {
            self.start_note(note, velocity);
        }
    }
    fn note_off(&mut self, note: u8) {
        self.release_note(note);
    }
    fn pitch_bend(&mut self, _value: u16) { /* off by default for LNF-1 parity */
    }
    fn control_change(&mut self, controller: u8, value: u8) {
        match controller {
            1 => self.mod_wheel = f32::from(value) / 127.0,
            64 => self.sustain = value >= 64,
            120 | 123 => self.all_notes_off(),
            _ => {}
        }
    }
    fn program_change(&mut self, _program: u8) {}
    fn all_notes_off(&mut self) {
        for voice in &mut self.voices {
            voice.held = false;
            voice.env *= 0.1;
        }
        self.sustain = false;
    }
    fn reset(&mut self) {
        self.voices = [Voice::default(); MAX_VOICES];
        self.sustain = false;
        self.mod_wheel = 0.0;
        self.lfo_phase = 0.0;
        self.ambience.reset();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_keys_and_page_layout_match_device_contract() {
        assert_eq!(Lnf1ParamId::FineTuning.key(), "fineTuning");
        assert_eq!(Lnf1ParamId::OutputLevel.key(), "outputLevel");
        assert_eq!(Lnf1Page::Tune.controls(false)[3], None);
        assert_eq!(
            Lnf1Page::Tone.controls(false)[2],
            Some(Lnf1ParamId::MufflingFilter)
        );
        assert_eq!(
            Lnf1Page::Tone.controls(true)[2],
            Some(Lnf1ParamId::Modulation)
        );
    }

    #[test]
    fn page_reset_only_resets_its_controls() {
        let mut params = Lnf1Params::default();
        params.set(Lnf1ParamId::FineTuning, 33.0);
        params.set(Lnf1ParamId::EnvDecay, 81.0);
        params.reset_page(Lnf1Page::Tune, false);
        assert_eq!(params.get(Lnf1ParamId::FineTuning), 0.0);
        assert_eq!(params.get(Lnf1ParamId::EnvDecay), 81.0);
    }

    #[test]
    fn note_renders_finite_stereo_audio_and_releases() {
        let mut piano = Lnf1Instrument::new(48_000);
        piano.note_on(60, 112);
        let mut l = [0.0; 512];
        let mut r = [0.0; 512];
        piano.render(&mut l, &mut r);
        assert!(l.iter().chain(r.iter()).all(|sample| sample.is_finite()));
        assert!(l.iter().any(|sample| sample.abs() > 0.00001));
        assert!(piano.active_voices() > 0);
        piano.note_off(60);
        for _ in 0..1_000 {
            piano.render(&mut l, &mut r);
        }
        assert_eq!(piano.active_voices(), 0);
    }

    #[test]
    fn atomic_parameter_store_updates_an_existing_voice_engine() {
        let handle = Lnf1ParamStore::new(Lnf1Params::default());
        let mut piano = Lnf1Instrument::with_param_store(48_000, handle.clone());
        let mut params = Lnf1Params::default();
        params.set(Lnf1ParamId::FineTuning, 24.0);
        params.set(Lnf1ParamId::OutputLevel, -3.0);
        handle.set_params(params);
        let mut l = [0.0; 32];
        let mut r = [0.0; 32];
        piano.render(&mut l, &mut r);
        assert_eq!(piano.params().get(Lnf1ParamId::FineTuning), 24.0);
        assert_eq!(piano.params().get(Lnf1ParamId::OutputLevel), -3.0);
    }
}

[executed on device: Johnnysstation (1200896c-45a0-4cb2-8dfa-479e25d803a4)]