//! Minimal RIFF/WAVE reader (docs/changes/0005 §2: no new dependency).
//!
//! Supported: PCM 16-bit integer (format 1) and 32-bit IEEE float (format 3),
//! also when wrapped in WAVE_FORMAT_EXTENSIBLE (0xFFFE), 1–8 channels
//! averaged down to mono. Anything else is a typed [`WavError`].
//!
//! Chunk walking:
//! - The RIFF size field is not trusted; the buffer length is.
//! - Unknown chunks (LIST, fact, cue, …) are skipped, including the pad byte
//!   after odd-sized chunks.
//! - `fmt ` may come before or after `data`; the first of each is used.
//! - A `data` chunk whose size runs past the end of the buffer (streamed
//!   recorders write 0 or 0xFFFFFFFF there) is clamped to what is present.
//!   Any other truncated chunk is an error.
//! - A trailing partial frame is dropped.
//!
//! The decoded audio exists only in memory (INV-5).

use std::fmt;

/// Mono samples in [-1.0, 1.0] at the file's own sample rate.
#[derive(Clone, PartialEq)]
pub struct Wav {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
    /// Channel count in the file, before the downmix.
    pub channels: u16,
}

impl fmt::Debug for Wav {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Wav")
            .field("samples_len", &self.samples.len())
            .field("sample_rate", &self.sample_rate)
            .field("channels", &self.channels)
            .finish()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WavError {
    NotRiff,
    NotWave,
    /// A chunk header or a non-`data` chunk body runs past the end.
    Truncated,
    MissingFmt,
    MissingData,
    /// `fmt ` chunk shorter than 16 bytes, or extensible without its tail.
    BadFmt,
    UnsupportedFormat(u16),
    UnsupportedBits(u16),
    BadChannels(u16),
    BadSampleRate,
    /// `block_align` disagrees with channels × bits / 8.
    BadBlockAlign,
}

impl fmt::Display for WavError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WavError::NotRiff => f.write_str("not a RIFF file"),
            WavError::NotWave => f.write_str("RIFF file is not WAVE"),
            WavError::Truncated => f.write_str("WAV file is truncated"),
            WavError::MissingFmt => f.write_str("WAV file has no fmt chunk"),
            WavError::MissingData => f.write_str("WAV file has no data chunk"),
            WavError::BadFmt => f.write_str("WAV fmt chunk is malformed"),
            WavError::UnsupportedFormat(c) => write!(
                f,
                "unsupported WAV format code {c:#06x} (need PCM 16-bit or IEEE float 32-bit)"
            ),
            WavError::UnsupportedBits(b) => write!(
                f,
                "unsupported bit depth {b} (need 16-bit PCM or 32-bit float)"
            ),
            WavError::BadChannels(c) => write!(f, "unsupported channel count {c}"),
            WavError::BadSampleRate => f.write_str("WAV sample rate is zero"),
            WavError::BadBlockAlign => f.write_str("WAV block align does not match the format"),
        }
    }
}

impl std::error::Error for WavError {}

const FORMAT_PCM: u16 = 0x0001;
const FORMAT_FLOAT: u16 = 0x0003;
const FORMAT_EXTENSIBLE: u16 = 0xFFFE;
const MAX_CHANNELS: u16 = 8;

fn u16_at(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([b[at], b[at + 1]])
}

fn u32_at(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

#[derive(Debug, Clone, Copy)]
struct Fmt {
    format: u16,
    channels: u16,
    sample_rate: u32,
    block_align: u16,
    bits: u16,
}

fn parse_fmt(body: &[u8]) -> Result<Fmt, WavError> {
    if body.len() < 16 {
        return Err(WavError::BadFmt);
    }
    let mut format = u16_at(body, 0);
    let channels = u16_at(body, 2);
    let sample_rate = u32_at(body, 4);
    let block_align = u16_at(body, 12);
    let bits = u16_at(body, 14);
    if format == FORMAT_EXTENSIBLE {
        // cbSize(2) validBits(2) channelMask(4) subFormat GUID(16): the GUID
        // starts with the plain format code.
        if body.len() < 40 {
            return Err(WavError::BadFmt);
        }
        format = u16_at(body, 24);
    }
    Ok(Fmt {
        format,
        channels,
        sample_rate,
        block_align,
        bits,
    })
}

/// Decode a whole WAV file held in memory.
pub fn parse(bytes: &[u8]) -> Result<Wav, WavError> {
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" {
        return Err(WavError::NotRiff);
    }
    if &bytes[8..12] != b"WAVE" {
        return Err(WavError::NotWave);
    }

    let mut fmt: Option<Fmt> = None;
    let mut data: Option<&[u8]> = None;
    let mut at = 12usize;
    while at < bytes.len() {
        if bytes.len() - at < 8 {
            // Some writers leave a stray pad or junk byte at the end.
            if fmt.is_some() && data.is_some() {
                break;
            }
            return Err(WavError::Truncated);
        }
        let id = &bytes[at..at + 4];
        let size = u32_at(bytes, at + 4) as usize;
        let body_start = at + 8;
        let available = bytes.len() - body_start;
        if id == b"data" {
            let len = size.min(available);
            if data.is_none() {
                data = Some(&bytes[body_start..body_start + len]);
            }
            if size > available {
                break;
            }
        } else {
            if size > available {
                if fmt.is_some() && data.is_some() {
                    break;
                }
                return Err(WavError::Truncated);
            }
            if id == b"fmt " && fmt.is_none() {
                fmt = Some(parse_fmt(&bytes[body_start..body_start + size])?);
            }
        }
        // size <= available here, so this cannot overflow.
        at = body_start + size + (size & 1);
    }

    let fmt = fmt.ok_or(WavError::MissingFmt)?;
    let data = data.ok_or(WavError::MissingData)?;
    decode(fmt, data)
}

fn decode(fmt: Fmt, data: &[u8]) -> Result<Wav, WavError> {
    let bytes_per_sample = match (fmt.format, fmt.bits) {
        (FORMAT_PCM, 16) => 2usize,
        (FORMAT_FLOAT, 32) => 4usize,
        (FORMAT_PCM | FORMAT_FLOAT, b) => return Err(WavError::UnsupportedBits(b)),
        (c, _) => return Err(WavError::UnsupportedFormat(c)),
    };
    if fmt.channels == 0 || fmt.channels > MAX_CHANNELS {
        return Err(WavError::BadChannels(fmt.channels));
    }
    if fmt.sample_rate == 0 {
        return Err(WavError::BadSampleRate);
    }
    let channels = usize::from(fmt.channels);
    let frame = channels * bytes_per_sample;
    if usize::from(fmt.block_align) != frame {
        return Err(WavError::BadBlockAlign);
    }

    let read = |s: &[u8]| -> f32 {
        if bytes_per_sample == 2 {
            f32::from(i16::from_le_bytes([s[0], s[1]])) / 32_768.0
        } else {
            let x = f32::from_le_bytes([s[0], s[1], s[2], s[3]]);
            if x.is_finite() {
                x.clamp(-1.0, 1.0)
            } else {
                0.0
            }
        }
    };
    let samples = data
        .chunks_exact(frame)
        .map(|fr| {
            let sum: f32 = fr.chunks_exact(bytes_per_sample).map(read).sum();
            sum / channels as f32
        })
        .collect();
    Ok(Wav {
        samples,
        sample_rate: fmt.sample_rate,
        channels: fmt.channels,
    })
}

/// Build a WAV file in memory. Used by tests and by nothing else.
#[cfg(test)]
pub fn encode(format: u16, channels: u16, sample_rate: u32, bits: u16, data: &[u8]) -> Vec<u8> {
    let block_align = channels * bits / 8;
    let mut fmt = Vec::new();
    fmt.extend_from_slice(&format.to_le_bytes());
    fmt.extend_from_slice(&channels.to_le_bytes());
    fmt.extend_from_slice(&sample_rate.to_le_bytes());
    fmt.extend_from_slice(&(sample_rate * u32::from(block_align)).to_le_bytes());
    fmt.extend_from_slice(&block_align.to_le_bytes());
    fmt.extend_from_slice(&bits.to_le_bytes());
    let mut out = Vec::new();
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(b"WAVE");
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&(fmt.len() as u32).to_le_bytes());
    out.extend_from_slice(&fmt);
    out.extend_from_slice(b"data");
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(data);
    let riff = (out.len() - 8) as u32;
    out[4..8].copy_from_slice(&riff.to_le_bytes());
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn i16s(v: &[i16]) -> Vec<u8> {
        v.iter().flat_map(|x| x.to_le_bytes()).collect()
    }

    fn f32s(v: &[f32]) -> Vec<u8> {
        v.iter().flat_map(|x| x.to_le_bytes()).collect()
    }

    #[test]
    fn pcm16_mono() {
        let w = parse(&encode(1, 1, 16_000, 16, &i16s(&[0, 16_384, -32_768]))).unwrap();
        assert_eq!(w.sample_rate, 16_000);
        assert_eq!(w.channels, 1);
        assert_eq!(w.samples, vec![0.0, 0.5, -1.0]);
    }

    #[test]
    fn pcm16_stereo_downmix() {
        let w = parse(&encode(
            1,
            2,
            48_000,
            16,
            &i16s(&[16_384, 0, -16_384, -16_384]),
        ))
        .unwrap();
        assert_eq!(w.channels, 2);
        assert_eq!(w.samples, vec![0.25, -0.5]);
    }

    #[test]
    fn float32_clamps_and_cleans() {
        let w = parse(&encode(3, 1, 44_100, 32, &f32s(&[0.25, 2.0, f32::NAN]))).unwrap();
        assert_eq!(w.samples, vec![0.25, 1.0, 0.0]);
    }

    #[test]
    fn extensible_float() {
        // Hand-build a 40-byte extensible fmt with the float subformat.
        let mut b = encode(3, 2, 16_000, 32, &f32s(&[0.5, 0.5]));
        let mut fmt = b[20..36].to_vec();
        fmt[0..2].copy_from_slice(&FORMAT_EXTENSIBLE.to_le_bytes());
        fmt.extend_from_slice(&22u16.to_le_bytes()); // cbSize
        fmt.extend_from_slice(&32u16.to_le_bytes()); // valid bits
        fmt.extend_from_slice(&3u32.to_le_bytes()); // channel mask
        let mut guid = [0u8; 16];
        guid[0..2].copy_from_slice(&FORMAT_FLOAT.to_le_bytes());
        fmt.extend_from_slice(&guid);
        let data = b.split_off(36);
        let mut out = b[..12].to_vec();
        out.extend_from_slice(b"fmt ");
        out.extend_from_slice(&(fmt.len() as u32).to_le_bytes());
        out.extend_from_slice(&fmt);
        out.extend_from_slice(&data);
        let w = parse(&out).unwrap();
        assert_eq!(w.samples, vec![0.5]);
    }

    #[test]
    fn skips_unknown_and_odd_chunks_and_data_before_fmt() {
        let base = encode(1, 1, 16_000, 16, &i16s(&[1000, 2000]));
        let fmt_chunk = &base[12..36];
        let data_chunk = &base[36..];
        let mut out = base[..12].to_vec();
        out.extend_from_slice(b"LIST");
        out.extend_from_slice(&3u32.to_le_bytes());
        out.extend_from_slice(&[1, 2, 3, 0]); // 3 bytes + pad
        out.extend_from_slice(data_chunk);
        out.extend_from_slice(fmt_chunk);
        assert_eq!(parse(&out).unwrap().samples.len(), 2);
    }

    #[test]
    fn streamed_data_size_is_clamped() {
        let mut b = encode(1, 1, 16_000, 16, &i16s(&[1, 2, 3]));
        let at = b.len() - 6 - 4;
        b[at..at + 4].copy_from_slice(&u32::MAX.to_le_bytes());
        b.push(0x7f); // trailing partial frame is dropped
        assert_eq!(parse(&b).unwrap().samples.len(), 3);
    }

    #[test]
    fn typed_errors() {
        assert_eq!(parse(b""), Err(WavError::NotRiff));
        assert_eq!(parse(b"RIFX\0\0\0\0WAVE"), Err(WavError::NotRiff));
        assert_eq!(parse(b"RIFF\0\0\0\0AVI "), Err(WavError::NotWave));
        assert_eq!(parse(b"RIFF\0\0\0\0WAVE"), Err(WavError::MissingFmt));
        assert_eq!(parse(b"RIFF\0\0\0\0WAVEfm"), Err(WavError::Truncated));
        let b = encode(1, 1, 16_000, 16, &[]);
        assert_eq!(parse(&b[..36]), Err(WavError::MissingData));
        assert_eq!(parse(&b[..30]), Err(WavError::Truncated));
        assert_eq!(
            parse(&encode(1, 1, 16_000, 24, &[0; 6])),
            Err(WavError::UnsupportedBits(24))
        );
        assert_eq!(
            parse(&encode(3, 1, 16_000, 64, &[0; 8])),
            Err(WavError::UnsupportedBits(64))
        );
        assert_eq!(
            parse(&encode(2, 1, 16_000, 4, &[0; 8])),
            Err(WavError::UnsupportedFormat(2))
        );
        assert_eq!(
            parse(&encode(1, 0, 16_000, 16, &[])),
            Err(WavError::BadChannels(0))
        );
        assert_eq!(
            parse(&encode(1, 9, 16_000, 16, &[])),
            Err(WavError::BadChannels(9))
        );
        assert_eq!(
            parse(&encode(1, 1, 0, 16, &[])),
            Err(WavError::BadSampleRate)
        );
        let mut b = encode(1, 2, 16_000, 16, &[]);
        b[32..34].copy_from_slice(&3u16.to_le_bytes());
        assert_eq!(parse(&b), Err(WavError::BadBlockAlign));
        let mut b = encode(1, 1, 16_000, 16, &[]);
        b[16..20].copy_from_slice(&8u32.to_le_bytes());
        assert_eq!(parse(&b), Err(WavError::BadFmt));
    }

    #[test]
    fn debug_hides_samples() {
        let w = parse(&encode(1, 1, 16_000, 16, &i16s(&[12_345]))).unwrap();
        assert!(!format!("{w:?}").contains("0.37"));
    }

    proptest! {
        #[test]
        fn never_panics_on_arbitrary_bytes(bytes in proptest::collection::vec(any::<u8>(), 0..512)) {
            let _ = parse(&bytes);
        }

        #[test]
        fn never_panics_with_valid_header(
            body in proptest::collection::vec(any::<u8>(), 0..512),
        ) {
            let mut b = b"RIFF\0\0\0\0WAVE".to_vec();
            b.extend_from_slice(&body);
            let _ = parse(&b);
        }

        #[test]
        fn never_panics_on_mutated_valid_file(
            data in proptest::collection::vec(any::<u8>(), 0..64),
            flips in proptest::collection::vec((0usize..128, any::<u8>()), 0..8),
            cut in 0usize..160,
        ) {
            let mut b = encode(1, 2, 16_000, 16, &data);
            for (at, v) in flips {
                if let Some(x) = b.get_mut(at) { *x = v; }
            }
            b.truncate(cut.max(12).min(b.len()));
            if let Ok(w) = parse(&b) {
                prop_assert!(w.samples.iter().all(|s| s.is_finite() && s.abs() <= 1.0));
            }
        }
    }
}
