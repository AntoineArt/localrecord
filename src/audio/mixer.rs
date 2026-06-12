use std::collections::VecDeque;

/// Mix two interleaved stereo f32 streams. Shorter stream is padded with silence.
pub struct Mixer {
    loopback: VecDeque<f32>,
    mic: VecDeque<f32>,
    output: Vec<f32>,
    loopback_gain: f32,
    mic_gain: f32,
}

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
        self.drain_mixed();
    }

    pub fn push_mic(&mut self, samples: &[f32]) {
        self.mic.extend(samples);
        self.drain_mixed();
    }

    pub fn finish(mut self) -> Vec<f32> {
        self.drain_mixed();
        self.output
    }

    fn drain_mixed(&mut self) {
        while !self.loopback.is_empty() || !self.mic.is_empty() {
            let l = self.loopback.pop_front().unwrap_or(0.0);
            let m = self.mic.pop_front().unwrap_or(0.0);
            self.output
                .push((l * self.loopback_gain + m * self.mic_gain).clamp(-1.0, 1.0));
        }
    }
}
