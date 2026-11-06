//! The microphone's samples brought to what the engines read: mono, 16 kHz.

/// Interleaved frames of `channels` samples mixed down to one (their mean).
pub fn mono(samples: impl Iterator<Item = f32>, channels: usize) -> Vec<f32> {
    let channels = channels.max(1);
    if channels == 1 {
        return samples.collect();
    }
    let mut out = Vec::new();
    let (mut sum, mut n) = (0f32, 0usize);
    for s in samples {
        sum += s;
        n += 1;
        if n == channels {
            out.push(sum / channels as f32);
            (sum, n) = (0.0, 0);
        }
    }
    out
}

/// `samples` at `from` Hz brought to `to` Hz. Down, each output sample is the mean of the input
/// ones it covers (a box filter: enough against aliasing for speech); up or between, a linear
/// interpolation.
pub fn to_rate(samples: &[f32], from: u32, to: u32) -> Vec<f32> {
    if from == to || samples.is_empty() || from == 0 || to == 0 {
        return samples.to_vec();
    }
    let ratio = from as f64 / to as f64;
    let len = (samples.len() as f64 / ratio).floor() as usize;
    let mut out = Vec::with_capacity(len);
    if ratio > 1.0 {
        for i in 0..len {
            let start = (i as f64 * ratio) as usize;
            let end = (((i + 1) as f64 * ratio) as usize).clamp(start + 1, samples.len());
            let window = &samples[start.min(samples.len() - 1)..end];
            out.push(window.iter().sum::<f32>() / window.len() as f32);
        }
    } else {
        for i in 0..len {
            let at = i as f64 * ratio;
            let j = at.floor() as usize;
            let frac = (at - j as f64) as f32;
            let a = samples[j.min(samples.len() - 1)];
            let b = samples[(j + 1).min(samples.len() - 1)];
            out.push(a + (b - a) * frac);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stereo_frames_are_averaged() {
        assert_eq!(mono([1.0, 0.0, 0.5, 0.5, 0.2].into_iter(), 2), vec![0.5, 0.5]);
        assert_eq!(mono([0.1, 0.2].into_iter(), 1), vec![0.1, 0.2]);
    }

    #[test]
    fn down_by_three_averages_each_triple() {
        let s = [0.0, 0.3, 0.6, 1.0, 1.0, 1.0, 0.9];
        assert_eq!(to_rate(&s, 48_000, 16_000), vec![0.3, 1.0]);
    }

    #[test]
    fn a_tone_keeps_its_length_and_level() {
        let tone: Vec<f32> = (0..44_100).map(|i| (i as f32 * 440.0 * std::f32::consts::TAU / 44_100.0).sin() * 0.5).collect();
        let out = to_rate(&tone, 44_100, 16_000);
        assert_eq!(out.len(), 16_000);
        let peak = out.iter().fold(0f32, |m, s| m.max(s.abs()));
        assert!(peak > 0.4 && peak <= 0.5, "{peak}");
    }

    #[test]
    fn up_interpolates() {
        assert_eq!(to_rate(&[0.0, 1.0], 8_000, 16_000), vec![0.0, 0.5, 1.0, 1.0]);
        assert_eq!(to_rate(&[0.2, 0.4], 16_000, 16_000), vec![0.2, 0.4]);
    }
}
