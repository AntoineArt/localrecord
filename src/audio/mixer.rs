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

/// One 10 ms WASAPI chunk at 48 kHz stereo (480 frames × 2 channels).
const CAPTURE_CHUNK_SAMPLES: usize = 480 * 2;

/// Wait for ~2 capture chunks before writing loopback without mic, so a mic
/// chunk for the same window is mixed instead of duplicated on a later pass.
const LOOPBACK_SOLO_WAIT_SAMPLES: usize = CAPTURE_CHUNK_SAMPLES * 2;

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
    }

    pub fn push_mic(&mut self, samples: &[f32]) {
        self.mic.extend(samples);
    }

    /// Trim drift and emit mixed output. Call after ingesting all pending chunks
    /// from both capture threads so they stay time-aligned.
    pub fn process(&mut self, flush: bool) {
        self.trim_drift();
        self.drain_mixed(flush);
    }

    pub fn take_output_chunk(&mut self, max_samples: usize) -> Vec<f32> {
        let end = max_samples.min(self.output.len());
        self.output.drain(..end).collect()
    }

    pub fn drain_remaining(&mut self) -> Vec<f32> {
        self.process(true);
        std::mem::take(&mut self.output)
    }

    pub fn finish(mut self) -> Vec<f32> {
        self.process(true);
        self.output
    }

    fn trim_drift(&mut self) {
        let loop_len = self.loopback.len();
        let mic_len = self.mic.len();
        // When one stream is idle (paused video, silent call remote, etc.) do not
        // trim the active stream or its samples get discarded.
        if loop_len == 0 || mic_len == 0 {
            return;
        }
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
    ///
    /// Keeps loopback and mic on a single timeline by pairing frames whenever
    /// both queues have data. Unpaired output is only used when one stream is
    /// genuinely idle (empty queue), never because the other chunk is simply late.
    fn drain_mixed(&mut self, flush: bool) {
        while self.loopback.len() >= 2 && self.mic.len() >= 2 {
            self.push_mixed_frame(true, true);
        }

        while self.loopback.is_empty() && self.mic.len() >= 2 {
            self.push_mixed_frame(false, true);
        }

        let loopback_solo_threshold = if flush {
            2
        } else {
            LOOPBACK_SOLO_WAIT_SAMPLES
        };
        while self.mic.is_empty() && self.loopback.len() >= loopback_solo_threshold {
            if self.loopback.len() < 2 {
                break;
            }
            self.push_mixed_frame(true, false);
        }
    }

    fn push_mixed_frame(&mut self, use_loopback: bool, use_mic: bool) {
        let (ll, lr) = if use_loopback {
            pop_stereo(&mut self.loopback)
        } else {
            (0.0, 0.0)
        };
        let (ml, mr) = if use_mic {
            pop_stereo(&mut self.mic)
        } else {
            (0.0, 0.0)
        };
        self.output
            .push((ll * self.loopback_gain + ml * self.mic_gain).clamp(-1.0, 1.0));
        self.output
            .push((lr * self.loopback_gain + mr * self.mic_gain).clamp(-1.0, 1.0));
    }
}

fn pop_stereo(queue: &mut VecDeque<f32>) -> (f32, f32) {
    (
        queue.pop_front().unwrap_or(0.0),
        queue.pop_front().unwrap_or(0.0),
    )
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

    #[test]
    fn outputs_mic_when_loopback_idle() {
        let mut mixer = Mixer::new(1.0, 1.0);
        mixer.push_mic(&[0.5, 0.25]);

        let out = mixer.finish();
        assert_eq!(out.len(), 2);
        assert!((out[0] - 0.5).abs() < f32::EPSILON);
        assert!((out[1] - 0.25).abs() < f32::EPSILON);
    }

    #[test]
    fn outputs_loopback_when_mic_idle() {
        let mut mixer = Mixer::new(1.0, 1.0);
        mixer.push_loopback(&[0.8, 0.2]);

        let out = mixer.finish();
        assert_eq!(out.len(), 2);
        assert!((out[0] - 0.8).abs() < f32::EPSILON);
        assert!((out[1] - 0.2).abs() < f32::EPSILON);
    }

    #[test]
    fn does_not_trim_mic_when_loopback_empty() {
        let mut mixer = Mixer::new(1.0, 1.0);
        let excess = MAX_DRIFT_SAMPLES + 4;
        let mic_only = vec![0.5, 0.5];
        for _ in 0..(excess / 2) {
            mixer.push_mic(&mic_only);
        }
        mixer.process(false);

        let out = mixer.finish();
        assert_eq!(out.len(), excess);
    }

    #[test]
    fn does_not_double_output_when_chunks_arrive_separately() {
        let chunk: Vec<f32> = (0..CAPTURE_CHUNK_SAMPLES).map(|i| i as f32 * 0.001).collect();
        let mut mixer = Mixer::new(1.0, 1.0);
        mixer.push_loopback(&chunk);
        mixer.process(false);
        assert!(mixer.take_output_chunk(CAPTURE_CHUNK_SAMPLES).is_empty());
        mixer.push_mic(&vec![0.0; CAPTURE_CHUNK_SAMPLES]);
        mixer.process(false);

        let out = mixer.finish();
        assert_eq!(out.len(), CAPTURE_CHUNK_SAMPLES);
    }

    #[test]
    fn aligns_both_streams_in_one_process_pass() {
        let chunk: Vec<f32> = vec![0.25; CAPTURE_CHUNK_SAMPLES];
        let mut mixer = Mixer::new(1.0, 1.0);
        mixer.push_loopback(&chunk);
        mixer.push_mic(&vec![0.5; CAPTURE_CHUNK_SAMPLES]);
        mixer.process(false);

        let out = mixer.finish();
        assert_eq!(out.len(), CAPTURE_CHUNK_SAMPLES);
        assert!((out[0] - 0.625).abs() < f32::EPSILON);
    }
}
