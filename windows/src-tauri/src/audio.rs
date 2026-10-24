//! In-memory audio helpers (INV-5). Nothing here touches the filesystem.
//!
//! The recorder (`recorder.rs`, Windows only) appends mono samples at the
//! device rate; `finish` resamples them to 16 kHz for the STT engine.

use keepsay_core::CoreError;
use keepsay_core::stt::{SAMPLE_RATE, resample_linear};
use zeroize::Zeroize;

/// Longest recording kept. After this the recorder stops appending and the
/// overlay tells the user to stop (well under `stt::MAX_SAMPLES`).
pub const MAX_RECORDING_SECS: u32 = 5 * 60;

/// Average interleaved frames into mono and append them to `out`, keeping at
/// most `cap` samples in `out`. Returns the RMS level of the appended block
/// (0.0 – 1.0) for the overlay meter.
pub fn append_mono(interleaved: &[f32], channels: u16, out: &mut Vec<f32>, cap: usize) -> f32 {
    let ch = usize::from(channels.max(1));
    let mut sum_sq = 0.0f32;
    let mut n = 0usize;
    for frame in interleaved.chunks(ch) {
        let mut acc = 0.0f32;
        for s in frame {
            acc += if s.is_finite() { *s } else { 0.0 };
        }
        let v = (acc / frame.len() as f32).clamp(-1.0, 1.0);
        sum_sq += v * v;
        n += 1;
        if out.len() < cap {
            out.push(v);
        }
    }
    if n == 0 {
        0.0
    } else {
        (sum_sq / n as f32).sqrt().min(1.0)
    }
}

/// Resample mono PCM at `rate` to 16 kHz. The input buffer is zeroed and
/// emptied either way, so the only copy left is the returned one.
pub fn finish(pcm: &mut Vec<f32>, rate: u32) -> Result<Vec<f32>, CoreError> {
    let out = resample_linear(pcm, rate);
    pcm.zeroize();
    out
}

/// Samples to keep at `rate` for [`MAX_RECORDING_SECS`].
pub fn cap_for_rate(rate: u32) -> usize {
    rate as usize * MAX_RECORDING_SECS as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn downmix_averages_channels() {
        let mut out = Vec::new();
        let lvl = append_mono(&[0.5, -0.5, 1.0, 0.0], 2, &mut out, 100);
        assert_eq!(out, vec![0.0, 0.5]);
        assert!((lvl - (0.125f32).sqrt()).abs() < 1e-6);
    }

    #[test]
    fn non_finite_and_cap() {
        let mut out = Vec::new();
        append_mono(&[f32::NAN, 2.0, -3.0], 1, &mut out, 2);
        assert_eq!(out, vec![0.0, 1.0]);
        assert_eq!(append_mono(&[], 1, &mut out, 2), 0.0);
    }

    #[test]
    fn finish_resamples_and_clears() {
        let mut pcm = vec![0.25f32; 48_000];
        let out = finish(&mut pcm, 48_000).unwrap();
        assert_eq!(out.len(), SAMPLE_RATE as usize);
        assert!(pcm.is_empty());
        let mut bad = vec![0.0f32; 10];
        assert!(finish(&mut bad, 1).is_err());
        assert!(bad.is_empty());
    }
}
