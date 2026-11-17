//! Audio asset decoding and optional offline sample-rate conversion.
//! This module MUST be called on a worker/control thread BEFORE starting an audio stream.
//! Symphonia decodes supported formats into immutable in-memory stereo frames.

use std::{fmt, fs::File, path::Path};

use symphonia::{
    core::{
        audio::SampleBuffer, codecs::DecoderOptions, errors::Error as SymphoniaError,
        formats::FormatOptions, io::MediaSourceStream, meta::MetadataOptions, probe::Hint,
    },
    default::{get_codecs, get_probe},
};

use crate::graph::{Source, StereoFrame};

/// Maximum decoded frames per audio clip (30 minutes at 96 kHz).
/// Longer material needs a disk-streaming engine, not RAM buffering.
const MAX_FRAMES: usize = 30 * 60 * 96_000;

#[derive(Debug)]
pub enum AudioLoadError {
    Io(std::io::Error),
    Decode(String),
    InvalidSampleRate,
    TooLong,
    Empty,
}

impl fmt::Display for AudioLoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "audio input: {error}"),
            Self::Decode(error) => write!(f, "audio decoder: {error}"),
            Self::InvalidSampleRate => write!(f, "invalid file or project sample rate"),
            Self::TooLong => write!(f, "audio clip exceeds safe in-memory frame limit"),
            Self::Empty => write!(f, "audio file contains no decodable audio frames"),
        }
    }
}
impl std::error::Error for AudioLoadError {}
impl From<std::io::Error> for AudioLoadError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

/// Load and decode WAV/FLAC/MP3/OGG and other enabled Symphonia formats.
/// Stereo channels use the first two channels; mono is copied to L/R.
/// Multichannel downmix is NOT implemented. Linear offline SRC is a temporary
/// quality compromise, and must be replaced before professional exports.
pub fn decode_to_source(
    path: &Path,
    project_sample_rate: u32,
    start_frame: u64,
) -> Result<Source, AudioLoadError> {
    if project_sample_rate == 0 {
        return Err(AudioLoadError::InvalidSampleRate);
    }
    let file = File::open(path)?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(extension) = path.extension().and_then(|ext| ext.to_str()) {
        hint.with_extension(extension);
    }
    let mut format = get_probe()
        .format(
            &hint,
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .map_err(|e| AudioLoadError::Decode(e.to_string()))?
        .format;
    let track = format.default_track().ok_or(AudioLoadError::Empty)?;
    let track_id = track.id;
    let input_sample_rate = track
        .codec_params
        .sample_rate
        .ok_or(AudioLoadError::InvalidSampleRate)?;
    let mut decoder = get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|e| AudioLoadError::Decode(e.to_string()))?;
    let mut frames = Vec::<StereoFrame>::new();

    loop {
        let packet = match format.next_packet() {
            Ok(packet) => packet,
            Err(SymphoniaError::IoError(err))
                if err.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                break;
            }
            Err(error) => return Err(AudioLoadError::Decode(error.to_string())),
        };
        if packet.track_id() != track_id {
            continue;
        }
        let decoded = match decoder.decode(&packet) {
            Ok(decoded) => decoded,
            Err(SymphoniaError::DecodeError(_)) => continue,
            Err(SymphoniaError::ResetRequired) => {
                return Err(AudioLoadError::Decode(
                    "stream changed codec parameters".into(),
                ));
            }
            Err(error) => return Err(AudioLoadError::Decode(error.to_string())),
        };
        let count = decoded.spec().channels.count();
        if count == 0 {
            return Err(AudioLoadError::Decode("audio has zero channels".into()));
        }
        let mut float_samples =
            SampleBuffer::<f32>::new(decoded.capacity() as u64, *decoded.spec());
        float_samples.copy_interleaved_ref(decoded);
        let chunk_count = float_samples.samples().len() / count;
        if frames.len().saturating_add(chunk_count) > MAX_FRAMES {
            return Err(AudioLoadError::TooLong);
        }
        for channels in float_samples.samples().chunks_exact(count) {
            frames.push(StereoFrame {
                left: channels[0],
                right: channels.get(1).copied().unwrap_or(channels[0]),
            });
        }
    }

    if frames.is_empty() {
        return Err(AudioLoadError::Empty);
    }
    let frames = if input_sample_rate == project_sample_rate {
        frames
    } else {
        resample_linear(&frames, input_sample_rate, project_sample_rate)?
    };
    Ok(Source::Samples {
        frames: frames.into(),
        start_frame,
    })
}

fn resample_linear(
    input: &[StereoFrame],
    input_rate: u32,
    output_rate: u32,
) -> Result<Vec<StereoFrame>, AudioLoadError> {
    if input_rate == 0 || output_rate == 0 {
        return Err(AudioLoadError::InvalidSampleRate);
    }
    if input.is_empty() {
        return Ok(Vec::new());
    }
    let frames = ((input.len() as u128 * output_rate as u128) + u128::from(input_rate) - 1)
        / u128::from(input_rate);
    let frames = usize::try_from(frames).map_err(|_| AudioLoadError::TooLong)?;
    if frames > MAX_FRAMES {
        return Err(AudioLoadError::TooLong);
    }
    let ratio = input_rate as f64 / output_rate as f64;
    let mut output = Vec::with_capacity(frames);
    for sample in 0..frames {
        let position = sample as f64 * ratio;
        let start = (position.floor() as usize).min(input.len() - 1);
        let end = start.saturating_add(1).min(input.len() - 1);
        let frac = (position - start as f64).clamp(0.0, 1.0) as f32;
        output.push(StereoFrame {
            left: input[start].left + frac * (input[end].left - input[start].left),
            right: input[start].right + frac * (input[end].right - input[start].right),
        });
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linear_resampler_keeps_endpoints_and_channel_separation() {
        let input = vec![
            StereoFrame {
                left: 0.0,
                right: 1.0,
            },
            StereoFrame {
                left: 1.0,
                right: 0.0,
            },
        ];
        let output = resample_linear(&input, 24_000, 48_000).unwrap();
        assert_eq!(output.len(), 4);
        assert_eq!(output[0], input[0]);
        assert_eq!(
            output[1],
            StereoFrame {
                left: 0.5,
                right: 0.5
            }
        );
        assert_eq!(output[2], input[1]);
        assert_eq!(output[3], input[1]);
    }

    #[test]
    fn decode_real_mono_pcm_wav_into_stereo_source() {
        use std::io::Write;
        let path = std::env::temp_dir().join(format!(
            "rust-daw-test-{}-{:?}.wav",
            std::process::id(),
            std::thread::current().id()
        ));
        let pcm = [0_i16, 16384, -16384, 0];
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&(36_u32 + pcm.len() as u32 * 2).to_le_bytes());
        bytes.extend_from_slice(b"WAVEfmt ");
        bytes.extend_from_slice(&16_u32.to_le_bytes());
        bytes.extend_from_slice(&1_u16.to_le_bytes()); // PCM format
        bytes.extend_from_slice(&1_u16.to_le_bytes()); // mono
        bytes.extend_from_slice(&48_000_u32.to_le_bytes());
        bytes.extend_from_slice(&96_000_u32.to_le_bytes()); // bytes/sec
        bytes.extend_from_slice(&2_u16.to_le_bytes()); // block align
        bytes.extend_from_slice(&16_u16.to_le_bytes()); // bits/sample
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&(pcm.len() as u32 * 2).to_le_bytes());
        for sample in pcm {
            bytes.extend_from_slice(&sample.to_le_bytes());
        }
        std::fs::File::create(&path)
            .unwrap()
            .write_all(&bytes)
            .unwrap();
        let result = decode_to_source(&path, 48_000, 32);
        let _ = std::fs::remove_file(path);
        let Source::Samples {
            frames,
            start_frame,
        } = result.unwrap()
        else {
            panic!("expected RAM stereo buffer");
        };
        assert_eq!(start_frame, 32);
        assert_eq!(frames.len(), 4);
        assert!((frames[1].left - 0.5).abs() < 0.0001);
        assert_eq!(frames[1].left, frames[1].right);
        assert!((frames[2].left + 0.5).abs() < 0.0001);
    }

    #[test]
    fn bad_sample_rate_rejected_before_io() {
        assert!(matches!(
            decode_to_source(Path::new("/does/not/exist.wav"), 0, 0),
            Err(AudioLoadError::InvalidSampleRate)
        ));
    }
}
