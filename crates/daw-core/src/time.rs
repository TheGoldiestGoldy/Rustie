use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;

use serde::{Deserialize, Serialize};

/// 960 musical ticks per quarter note, independent of the audio sample rate.
pub const PPQ: i64 = 960;

#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub struct MusicalTime(i64);

impl MusicalTime {
    pub const ZERO: Self = Self(0);

    pub const fn from_ticks(ticks: i64) -> Self {
        Self(ticks)
    }

    pub const fn ticks(self) -> i64 {
        self.0
    }

    pub fn from_quarters(quarters: i64) -> Result<Self, TimeError> {
        quarters
            .checked_mul(PPQ)
            .map(Self)
            .ok_or(TimeError::Overflow)
    }
}

/// Micro-beats-per-minute (BPM * 1_000_000), so tempo integration is integer-based.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tempo {
    micro_bpm: u64,
}

impl Tempo {
    pub fn from_bpm(bpm: f64) -> Result<Self, TimeError> {
        if !bpm.is_finite() || !(0.000_001..=1_000_000.0).contains(&bpm) {
            return Err(TimeError::InvalidTempo);
        }
        let micro_bpm = (bpm * 1_000_000.0).round() as u64;
        Self::from_micro_bpm(micro_bpm)
    }

    pub const fn from_micro_bpm(micro_bpm: u64) -> Result<Self, TimeError> {
        if micro_bpm == 0 {
            Err(TimeError::InvalidTempo)
        } else {
            Ok(Self { micro_bpm })
        }
    }

    pub const fn micro_bpm(self) -> u64 {
        self.micro_bpm
    }

    pub fn bpm(self) -> f64 {
        self.micro_bpm as f64 / 1_000_000.0
    }
}

impl Default for Tempo {
    fn default() -> Self {
        Self {
            micro_bpm: 120_000_000,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimeSignature {
    pub numerator: u8,
    pub denominator: u8,
}

impl TimeSignature {
    pub const fn new(numerator: u8, denominator: u8) -> Result<Self, TimeError> {
        if numerator == 0
            || numerator > 32
            || denominator == 0
            || denominator > 32
            || !denominator.is_power_of_two()
        {
            Err(TimeError::InvalidTimeSignature)
        } else {
            Ok(Self {
                numerator,
                denominator,
            })
        }
    }
}

impl Default for TimeSignature {
    fn default() -> Self {
        Self {
            numerator: 4,
            denominator: 4,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimeError {
    InvalidTempo,
    InvalidTimeSignature,
    InvalidSampleRate,
    NegativePosition,
    Overflow,
}

impl fmt::Display for TimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidTempo => write!(f, "tempo must be finite and positive"),
            Self::InvalidTimeSignature => write!(f, "invalid time signature"),
            Self::InvalidSampleRate => write!(f, "sample rate must be non-zero"),
            Self::NegativePosition => write!(f, "negative position cannot map to a project frame"),
            Self::Overflow => write!(f, "musical time conversion overflow"),
        }
    }
}
impl Error for TimeError {}

/// Maps integer tick positions onto real time, with changes at exact tick boundaries.
/// Keeps time positions in ticks and integrates each constant-tempo segment with
/// i128 fixed-point nanoseconds. No cumulative floating-point beat-position drift.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TempoMap {
    changes: BTreeMap<i64, Tempo>,
}

impl TempoMap {
    pub fn new(initial: Tempo) -> Self {
        let mut changes = BTreeMap::new();
        changes.insert(0, initial);
        Self { changes }
    }

    pub fn insert(&mut self, at: MusicalTime, tempo: Tempo) -> Result<(), TimeError> {
        if at.ticks() < 0 {
            return Err(TimeError::NegativePosition);
        }
        self.changes.insert(at.ticks(), tempo);
        Ok(())
    }

    pub fn tempo_at(&self, at: MusicalTime) -> Result<Tempo, TimeError> {
        if at.ticks() < 0 {
            return Err(TimeError::NegativePosition);
        }
        self.changes
            .range(..=at.ticks())
            .next_back()
            .map(|(_, tempo)| *tempo)
            .ok_or(TimeError::InvalidTempo)
    }

    pub fn changes(&self) -> impl Iterator<Item = (MusicalTime, Tempo)> + '_ {
        self.changes
            .iter()
            .map(|(&tick, &tempo)| (MusicalTime::from_ticks(tick), tempo))
    }

    /// Rounded to the nearest sample. Supports rates such as 44_100, 48_000, 96_000.
    pub fn sample_frame_at(&self, at: MusicalTime, sample_rate: u32) -> Result<u64, TimeError> {
        if sample_rate == 0 {
            return Err(TimeError::InvalidSampleRate);
        }
        if at.ticks() < 0 {
            return Err(TimeError::NegativePosition);
        }
        let mut total_ns = 0_i128;
        let mut last_tick = 0_i64;
        let mut tempo = self.tempo_at(MusicalTime::ZERO)?;
        if at.ticks() > 0 {
            for (&tick, &next_tempo) in self.changes.range(1..=at.ticks()) {
                total_ns = total_ns
                    .checked_add(segment_nanoseconds(tick - last_tick, tempo)?)
                    .ok_or(TimeError::Overflow)?;
                last_tick = tick;
                tempo = next_tempo;
            }
        }
        total_ns = total_ns
            .checked_add(segment_nanoseconds(at.ticks() - last_tick, tempo)?)
            .ok_or(TimeError::Overflow)?;

        let sample_product = total_ns
            .checked_mul(i128::from(sample_rate))
            .and_then(|n| n.checked_add(500_000_000))
            .ok_or(TimeError::Overflow)?;
        u64::try_from(sample_product / 1_000_000_000).map_err(|_| TimeError::Overflow)
    }
}

impl Default for TempoMap {
    fn default() -> Self {
        Self::new(Tempo::default())
    }
}

fn segment_nanoseconds(ticks: i64, tempo: Tempo) -> Result<i128, TimeError> {
    // ticks * 60 seconds * 1e9 ns/sec * 1e6 micro-BPM/BPM / (PPQ * micro-BPM)
    const NS_NUMERATOR: i128 = 60_000_000_000_000_000;
    let numerator = i128::from(ticks)
        .checked_mul(NS_NUMERATOR)
        .ok_or(TimeError::Overflow)?;
    let denominator = i128::from(PPQ)
        .checked_mul(i128::from(tempo.micro_bpm()))
        .ok_or(TimeError::Overflow)?;
    Ok(numerator / denominator)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quarter_notes_use_exact_ticks() {
        assert_eq!(MusicalTime::from_quarters(4).unwrap().ticks(), 3840);
        assert_eq!(MusicalTime::from_quarters(-2).unwrap().ticks(), -1920);
        assert!(MusicalTime::from_quarters(i64::MAX).is_err());
    }

    #[test]
    fn frame_conversion_at_multiple_rates() {
        let map = TempoMap::new(Tempo::from_bpm(120.0).unwrap());
        for sample_rate in [44_100, 48_000, 96_000] {
            assert_eq!(map.sample_frame_at(MusicalTime::ZERO, sample_rate), Ok(0));
            assert_eq!(
                map.sample_frame_at(MusicalTime::from_quarters(1).unwrap(), sample_rate),
                Ok(u64::from(sample_rate) / 2)
            );
        }
    }

    #[test]
    fn changes_are_sample_accurate_at_boundaries() {
        let mut map = TempoMap::new(Tempo::from_bpm(120.0).unwrap());
        map.insert(
            MusicalTime::from_quarters(4).unwrap(),
            Tempo::from_bpm(60.0).unwrap(),
        )
        .unwrap();
        assert_eq!(
            map.sample_frame_at(MusicalTime::from_quarters(4).unwrap(), 48_000),
            Ok(96_000)
        );
        assert_eq!(
            map.sample_frame_at(MusicalTime::from_quarters(8).unwrap(), 48_000),
            Ok(288_000)
        );
        assert_eq!(
            map.tempo_at(MusicalTime::from_quarters(5).unwrap())
                .unwrap()
                .bpm(),
            60.0
        );
    }

    #[test]
    fn fractional_bpm_retains_micro_bpm_precision() {
        let map = TempoMap::new(Tempo::from_bpm(123.456_789).unwrap());
        assert_eq!(
            map.tempo_at(MusicalTime::ZERO).unwrap().micro_bpm(),
            123_456_789
        );
        let frame = map
            .sample_frame_at(MusicalTime::from_quarters(1000).unwrap(), 48_000)
            .unwrap();
        let expected = (1000.0_f64 * 60.0 / 123.456_789 * 48_000.0).round() as u64;
        assert!((frame as i64 - expected as i64).abs() <= 1);
    }

    #[test]
    fn rejects_invalid_inputs() {
        assert!(Tempo::from_bpm(0.0).is_err());
        assert!(Tempo::from_bpm(f64::NAN).is_err());
        assert!(TimeSignature::new(4, 3).is_err());
        assert!(TimeSignature::new(7, 8).is_ok());
        assert_eq!(
            TempoMap::default().sample_frame_at(MusicalTime::ZERO, 0),
            Err(TimeError::InvalidSampleRate)
        );
        assert_eq!(
            TempoMap::default().sample_frame_at(MusicalTime::from_ticks(-1), 48_000),
            Err(TimeError::NegativePosition)
        );
    }
}
