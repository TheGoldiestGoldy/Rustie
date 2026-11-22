// Slate Studio built-in effects. Independent DSP code, original UI and
// algorithms using conventional biquad/schroeder building blocks.
// No dependency on third-party plugin binaries or proprietary audio assets.
use daw_core::model::{BuiltinEffect, BuiltinEffectKind};

use crate::graph::StereoFrame;
use crate::live_plugin::{LiveMidiMessage, LivePluginEffect};

#[derive(Debug, Default)]
pub struct EffectRack {
    units: Vec<EffectUnit>,
}

impl EffectRack {
    pub fn prepare(specs: &[BuiltinEffect], sample_rate: u32) -> Self {
        let units = specs
            .iter()
            .take(8)
            .filter(|spec| spec.enabled)
            .map(|spec| match spec.kind {
                BuiltinEffectKind::Eq3 => EffectUnit::Eq3(Eq3::new(spec, sample_rate)),
                BuiltinEffectKind::Eq8 => EffectUnit::Eq8(Eq8::new(spec, sample_rate)),
                BuiltinEffectKind::VocalReverb | BuiltinEffectKind::GuitarReverb => {
                    EffectUnit::Reverb(Reverb::new(spec, sample_rate))
                }
            })
            .collect();
        Self { units }
    }

    /// Connect an already-created, isolated CLAP realtime bridge after the
    /// built-in effects. This never loads a plug-in on the audio callback.
    pub fn add_live(&mut self, effect: LivePluginEffect) {
        self.units.push(EffectUnit::Live(Box::new(effect)));
    }

    #[inline]
    pub fn process(&mut self, input: StereoFrame) -> StereoFrame {
        self.units
            .iter_mut()
            .fold(input, |frame, unit| unit.process(frame))
    }

    pub fn note_on(&mut self, note: u8, velocity: u8) {
        self.live_midi(0, LiveMidiMessage::NoteOn(note, velocity));
    }

    pub fn note_off(&mut self, note: u8) {
        self.live_midi(0, LiveMidiMessage::NoteOff(note));
    }

    pub fn all_notes_off(&mut self) {
        self.live_midi(0, LiveMidiMessage::Panic);
    }

    pub fn live_midi(&mut self, channel: u8, message: LiveMidiMessage) {
        for unit in &mut self.units {
            if let EffectUnit::Live(plugin) = unit {
                plugin.midi(channel, message);
            }
        }
    }

    pub fn reset(&mut self) {
        for unit in &mut self.units {
            unit.reset();
        }
    }
}

#[derive(Debug)]
enum EffectUnit {
    Eq3(Eq3),
    Eq8(Eq8),
    Reverb(Reverb),
    Live(Box<LivePluginEffect>),
}

impl EffectUnit {
    fn process(&mut self, frame: StereoFrame) -> StereoFrame {
        match self {
            Self::Eq3(eq) => eq.process(frame),
            Self::Eq8(eq) => eq.process(frame),
            Self::Reverb(reverb) => reverb.process(frame),
            Self::Live(plugin) => plugin.process(frame),
        }
    }
    fn reset(&mut self) {
        match self {
            Self::Eq3(eq) => eq.reset(),
            Self::Eq8(eq) => eq.reset(),
            Self::Reverb(reverb) => reverb.reset(),
            Self::Live(plugin) => plugin.reset(),
        }
    }
}

fn db_to_amp(db: f32) -> f32 {
    10_f32.powf(db.clamp(-24.0, 24.0) / 20.0)
}

#[derive(Debug)]
struct Eq3 {
    low: [f32; 2],
    high_lp: [f32; 2],
    a_low: f32,
    a_high: f32,
    gains: [f32; 3],
}
impl Eq3 {
    fn new(spec: &BuiltinEffect, rate: u32) -> Self {
        let alpha = |freq: f32| {
            (1.0 - (-std::f32::consts::TAU * freq / rate.max(1) as f32).exp()).clamp(0.0, 1.0)
        };
        Self {
            low: [0.0; 2],
            high_lp: [0.0; 2],
            a_low: alpha(230.0),
            a_high: alpha(3200.0),
            gains: [
                db_to_amp(spec.gains_db[0]),
                db_to_amp(spec.gains_db[1]),
                db_to_amp(spec.gains_db[2]),
            ],
        }
    }
    fn process(&mut self, frame: StereoFrame) -> StereoFrame {
        let mut x = [frame.left, frame.right];
        for (ch, sample) in x.iter_mut().enumerate() {
            let v = *sample;
            self.low[ch] += self.a_low * (v - self.low[ch]);
            self.high_lp[ch] += self.a_high * (v - self.high_lp[ch]);
            let low = self.low[ch];
            let high = v - self.high_lp[ch];
            let mid = v - low - high;
            *sample = low * self.gains[0] + mid * self.gains[1] + high * self.gains[2];
        }
        StereoFrame {
            left: x[0],
            right: x[1],
        }
    }
    fn reset(&mut self) {
        self.low = [0.0; 2];
        self.high_lp = [0.0; 2];
    }
}

#[derive(Clone, Copy, Debug)]
struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    z1: [f32; 2],
    z2: [f32; 2],
}
impl Biquad {
    /// RBJ parametric bell in transposed direct-form II.
    fn peaking(freq: f32, q: f32, db: f32, rate: u32) -> Self {
        let w = std::f32::consts::TAU * freq.min(rate as f32 * 0.45) / rate.max(1) as f32;
        let alpha = w.sin() / (2.0 * q.max(0.1));
        let a = 10_f32.powf(db.clamp(-24.0, 24.0) / 40.0);
        let a0 = 1.0 + alpha / a;
        Self {
            b0: (1.0 + alpha * a) / a0,
            b1: (-2.0 * w.cos()) / a0,
            b2: (1.0 - alpha * a) / a0,
            a1: (-2.0 * w.cos()) / a0,
            a2: (1.0 - alpha / a) / a0,
            z1: [0.0; 2],
            z2: [0.0; 2],
        }
    }
    #[inline]
    fn run(&mut self, input: f32, channel: usize) -> f32 {
        let output = self.b0 * input + self.z1[channel];
        self.z1[channel] = self.b1 * input - self.a1 * output + self.z2[channel];
        self.z2[channel] = self.b2 * input - self.a2 * output;
        // Protect downstream DSP against denormal slowdown.
        if self.z1[channel].abs() < 1e-20 {
            self.z1[channel] = 0.0;
        }
        if self.z2[channel].abs() < 1e-20 {
            self.z2[channel] = 0.0;
        }
        output
    }
    fn reset(&mut self) {
        self.z1 = [0.0; 2];
        self.z2 = [0.0; 2];
    }
}

pub const EQ8_HZ: [f32; 8] = [45.0, 90.0, 180.0, 350.0, 700.0, 1400.0, 3200.0, 9500.0];

/// Magnitude-response curve for the same eight biquad coefficients used by
/// the audio processor. Computed on the UI thread for drawing, never in DSP.
pub fn eq8_response_db(gains: &[f32; 8], frequency: f32, rate: u32) -> f32 {
    let w = std::f32::consts::TAU * frequency.min(rate as f32 * 0.45) / rate.max(1) as f32;
    let (s1, c1) = w.sin_cos();
    let (s2, c2) = (2.0 * w).sin_cos();
    let mut total = 0.0_f32;
    for (i, center) in EQ8_HZ.iter().enumerate() {
        if gains[i].abs() < 0.001 {
            continue;
        }
        let b = Biquad::peaking(*center, 1.1, gains[i], rate);
        let nr = b.b0 + b.b1 * c1 + b.b2 * c2;
        let ni = -b.b1 * s1 - b.b2 * s2;
        let dr = 1.0 + b.a1 * c1 + b.a2 * c2;
        let di = -b.a1 * s1 - b.a2 * s2;
        let energy = (nr * nr + ni * ni) / (dr * dr + di * di).max(1e-20);
        total += 10.0 * energy.max(1e-20).log10();
    }
    total.clamp(-96.0, 96.0)
}

#[derive(Debug)]
struct Eq8 {
    bands: [Biquad; 8],
}
impl Eq8 {
    fn new(spec: &BuiltinEffect, rate: u32) -> Self {
        Self {
            bands: std::array::from_fn(|i| Biquad::peaking(EQ8_HZ[i], 1.1, spec.gains_db[i], rate)),
        }
    }
    fn process(&mut self, frame: StereoFrame) -> StereoFrame {
        let mut left = frame.left;
        let mut right = frame.right;
        for band in &mut self.bands {
            left = band.run(left, 0);
            right = band.run(right, 1);
        }
        StereoFrame { left, right }
    }
    fn reset(&mut self) {
        for band in &mut self.bands {
            band.reset();
        }
    }
}

#[derive(Debug)]
struct Comb {
    buffer: Vec<f32>,
    cursor: usize,
    feedback: f32,
    damping: f32,
    filtered: f32,
}
impl Comb {
    fn new(delay_seconds: f32, rate: u32, decay_seconds: f32, damping: f32) -> Self {
        let samples = (delay_seconds * rate as f32).round().max(1.0) as usize;
        let feedback = 10.0_f32
            .powf(-3.0 * delay_seconds / decay_seconds.max(0.2))
            .clamp(0.0, 0.97);
        Self {
            buffer: vec![0.0; samples],
            cursor: 0,
            feedback,
            damping: damping.clamp(0.05, 0.95),
            filtered: 0.0,
        }
    }
    fn process(&mut self, input: f32) -> f32 {
        let out = self.buffer[self.cursor];
        self.filtered = out * (1.0 - self.damping) + self.filtered * self.damping;
        self.buffer[self.cursor] = input + self.filtered * self.feedback;
        self.cursor = (self.cursor + 1) % self.buffer.len();
        if self.filtered.abs() < 1e-20 {
            self.filtered = 0.0;
        }
        out
    }
    fn reset(&mut self) {
        self.buffer.fill(0.0);
        self.filtered = 0.0;
        self.cursor = 0;
    }
}

#[derive(Debug)]
struct Reverb {
    lines: [Comb; 8],
    wet: f32,
}
impl Reverb {
    fn new(spec: &BuiltinEffect, rate: u32) -> Self {
        let guitar = spec.kind == BuiltinEffectKind::GuitarReverb;
        let base = if guitar {
            [0.031, 0.037, 0.041, 0.044]
        } else {
            [0.029, 0.033, 0.039, 0.043]
        };
        let decay = spec.decay_s.clamp(0.2, 8.0);
        let damping = spec.tone.clamp(0.0, 1.0) * 0.75 + 0.1;
        Self {
            lines: std::array::from_fn(|i| {
                Comb::new(
                    base[i % 4] + if i >= 4 { 0.0017 } else { 0.0 },
                    rate,
                    decay,
                    damping,
                )
            }),
            wet: spec.wet.clamp(0.0, 1.0),
        }
    }
    fn process(&mut self, frame: StereoFrame) -> StereoFrame {
        let mut left = 0.0;
        let mut right = 0.0;
        for i in 0..4 {
            left += self.lines[i].process(frame.left);
            right += self.lines[i + 4].process(frame.right);
        }
        // Parallel filtered combs; normalize so a dense tail can't explode.
        let w = self.wet;
        StereoFrame {
            left: frame.left * (1.0 - w) + left * (w * 0.25),
            right: frame.right * (1.0 - w) + right * (w * 0.25),
        }
    }
    fn reset(&mut self) {
        for line in &mut self.lines {
            line.reset();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn spec(kind: BuiltinEffectKind) -> BuiltinEffect {
        BuiltinEffect::new(kind)
    }
    #[test]
    fn eq_flat_is_transparent_and_bass_gain_is_real() {
        for kind in [BuiltinEffectKind::Eq3, BuiltinEffectKind::Eq8] {
            let mut flat = EffectRack::prepare(&[spec(kind)], 48_000);
            for i in 0..200 {
                let x = ((i as f32) * 0.1).sin() * 0.3;
                let y = flat.process(StereoFrame { left: x, right: -x });
                assert!((y.left - x).abs() < 0.0001, "flat response {kind:?} at {i}");
            }
            let mut boosted = spec(kind);
            boosted.gains_db[0] = 12.0;
            let mut rack = EffectRack::prepare(&[boosted], 48_000);
            let mut sum = 0.0;
            let mut original = 0.0;
            for i in 0..10_000 {
                let x = (std::f32::consts::TAU * 55.0 * i as f32 / 48_000.0).sin() * 0.1;
                if i > 3000 {
                    original += x * x;
                }
                let y = rack.process(StereoFrame { left: x, right: x }).left;
                if i > 3000 {
                    sum += y * y;
                }
            }
            assert!(
                sum > original * 1.1,
                "boost must change actual audio {kind:?}"
            );
        }
    }
    #[test]
    fn eq8_displayed_response_matches_the_real_filter_center_gain() {
        let mut db = [0.0_f32; 8];
        assert!(eq8_response_db(&db, 700.0, 48_000).abs() < 0.0001);
        db[4] = 12.0;
        assert!((eq8_response_db(&db, 700.0, 48_000) - 12.0).abs() < 0.05);
        db[4] = -9.5;
        assert!((eq8_response_db(&db, 700.0, 48_000) + 9.5).abs() < 0.05);
    }

    #[test]
    fn reverb_has_wet_tail_and_reset_clears_it() {
        let mut rack = EffectRack::prepare(&[spec(BuiltinEffectKind::VocalReverb)], 48_000);
        let first = rack.process(StereoFrame {
            left: 1.0,
            right: 1.0,
        });
        assert!(first.left < 1.0);
        let mut tail = false;
        for _ in 0..6000 {
            let x = rack.process(StereoFrame::ZERO);
            if x.left.abs() > 0.00001 {
                tail = true;
            }
            assert!(x.left.is_finite());
        }
        assert!(tail);
        rack.reset();
        for _ in 0..1000 {
            assert_eq!(rack.process(StereoFrame::ZERO), StereoFrame::ZERO);
        }
    }
}
