use std::collections::VecDeque;

/// Stereo interleaved f32: [L, R, L, R, ...]
pub struct Mixer {
    loopback: VecDeque<f32>,
    mic: VecDeque<f32>,
    output: Vec<f32>,
    loopback_gain: f32,
    mic_gain: f32,
}

/// ~100 ms of stereo samples at 48 kHz (4800 frames × 2 channels).
const MAX_DRIFT_SAMPLES: usize = 4800 * 2;

impl Mixer {
    pub fn new(loopback_gain: f32, mic_gain: f32) -> Self {
        Self {
            loopback: VecDeque::with_capacity(48_000 * 2),
            mic: VecDeque::with_capacity(48_000 * 2),
            output: Vec::new(),
            loopback_gain,
            mic_gain,
        }
    }

    pub fn push_loopback(&mut self, samples: &[f32]) {
        self.loopback.extend(samples);
        self.trim_drift();
        self.drain_mixed();
    }

    pub fn push_mic(&mut self, samples: &[f32]) {
        self.mic.extend(samples);
        self.trim_drift();
        self.drain_mixed();
    }

    pub fn take_output_chunk(&mut self, max_samples: usize) -> Vec<f32> {
        let end = max_samples.min(self.output.len());
        self.output.drain(..end).collect()
    }

    pub fn drain_remaining(&mut self) -> Vec<f32> {
        self.drain_mixed();
        std::mem::take(&mut self.output)
    }

    pub fn finish(mut self) -> Vec<f32> {
        self.drain_mixed();
        self.output
    }

    fn trim_drift(&mut self) {
        let loop_len = self.loopback.len();
        let mic_len = self.mic.len();
        if loop_len > mic_len + MAX_DRIFT_SAMPLES {
            let excess = loop_len - mic_len - MAX_DRIFT_SAMPLES;
            for _ in 0..excess {
                self.loopback.pop_front();
            }
        } else if mic_len > loop_len + MAX_DRIFT_SAMPLES {
            let excess = mic_len - loop_len - MAX_DRIFT_SAMPLES;
            for _ in 0..excess {
                self.mic.pop_front();
            }
        }
    }

    /// Mix one stereo frame (L/R pair) from each stream.
    fn drain_mixed(&mut self) {
        while self.loopback.len() >= 2 && self.mic.len() >= 2 {
            let ll = self.loopback.pop_front().unwrap_or(0.0);
            let lr = self.loopback.pop_front().unwrap_or(0.0);
            let ml = self.mic.pop_front().unwrap_or(0.0);
            let mr = self.mic.pop_front().unwrap_or(0.0);
            self.output
                .push((ll * self.loopback_gain + ml * self.mic_gain).clamp(-1.0, 1.0));
            self.output
                .push((lr * self.loopback_gain + mr * self.mic_gain).clamp(-1.0, 1.0));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mixes_stereo_frame_pairs_not_individual_samples() {
        let mut mixer = Mixer::new(1.0, 1.0);
        // Loopback stereo: L=1, R=0
        mixer.push_loopback(&[1.0, 0.0]);
        // Mic stereo: L=0, R=1
        mixer.push_mic(&[0.0, 1.0]);

        let out = mixer.finish();
        assert_eq!(out.len(), 2, "one stereo frame = two samples");
        assert!((out[0] - 1.0).abs() < f32::EPSILON);
        assert!((out[1] - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn trims_excess_loopback_drift() {
        let mut mixer = Mixer::new(1.0, 1.0);
        let excess = MAX_DRIFT_SAMPLES + 4;
        let big = vec![0.1; excess];
        mixer.push_loopback(&big);
        mixer.push_mic(&[0.0, 0.0]);

        let out = mixer.finish();
        assert_eq!(out.len(), 2);
    }
}
