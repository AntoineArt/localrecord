use std::collections::VecDeque;

use super::agc::Agc;

/// Stereo interleaved f32: [L, R, L, R, ...]
pub struct Mixer {
    loopback: VecDeque<f32>,
    mic: VecDeque<f32>,
    output: Vec<f32>,
    loopback_gain: f32,
    mic_gain: f32,
    /// One AGC per source, so the two are levelled independently before they
    /// are summed. `None` when the `agc` setting is off.
    loopback_agc: Option<Agc>,
    mic_agc: Option<Agc>,
}

/// ~500 ms of stereo samples at 48 kHz (24000 frames × 2 channels): the
/// pairing window. Queues within this of each other still pair; only samples
/// beyond it are treated as unrecoverable drift and dropped. Sized to cover a
/// whole PulseAudio fragment (up to ~370 ms with server-default buffer
/// attributes), so bursty delivery is absorbed instead of trimmed.
const MAX_DRIFT_SAMPLES: usize = 24_000 * 2;

/// One 10 ms capture chunk at 48 kHz stereo (480 frames × 2 channels).
#[cfg(test)]
const CAPTURE_CHUNK_SAMPLES: usize = 480 * 2;

/// While the other stream's queue is empty, hold this many samples back
/// instead of writing them unpaired. An empty peer queue usually means its
/// next chunk is merely late — bursty fragment delivery, thread scheduling —
/// not that the stream is idle. Writing solo audio that should have been
/// paired inserts the peer's window twice and stretches the timeline (the
/// 1.57x-length recordings with silence stutters every few hundred ms).
/// Matches the pairing window: anything older could not pair anyway.
const SOLO_RESERVE_SAMPLES: usize = MAX_DRIFT_SAMPLES;

impl Mixer {
    pub fn new(loopback_gain: f32, mic_gain: f32) -> Self {
        Self {
            loopback: VecDeque::with_capacity(48_000 * 2),
            mic: VecDeque::with_capacity(48_000 * 2),
            output: Vec::new(),
            loopback_gain,
            mic_gain,
            loopback_agc: None,
            mic_agc: None,
        }
    }

    /// Enable per-source automatic gain control, which levels the loopback and
    /// microphone streams towards a common target before mixing. See
    /// [`crate::audio::agc`] for what that costs.
    pub fn with_agc(mut self, enabled: bool) -> Self {
        if enabled {
            self.loopback_agc = Some(Agc::new());
            self.mic_agc = Some(Agc::new());
        }
        self
    }

    pub fn push_loopback(&mut self, samples: &[f32]) {
        push_leveled(&mut self.loopback, &mut self.loopback_agc, samples);
    }

    pub fn push_mic(&mut self, samples: &[f32]) {
        push_leveled(&mut self.mic, &mut self.mic_agc, samples);
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

        let solo_threshold = if flush { 2 } else { SOLO_RESERVE_SAMPLES + 2 };
        while self.loopback.is_empty() && self.mic.len() >= solo_threshold {
            self.push_mixed_frame(false, true);
        }
        while self.mic.is_empty() && self.loopback.len() >= solo_threshold {
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
            .push(soft_clip(ll * self.loopback_gain + ml * self.mic_gain));
        self.output
            .push(soft_clip(lr * self.loopback_gain + mr * self.mic_gain));
    }
}

/// Level a chunk through `agc` (when enabled) on its way into `queue`.
fn push_leveled(queue: &mut VecDeque<f32>, agc: &mut Option<Agc>, samples: &[f32]) {
    match agc {
        Some(agc) => {
            let mut leveled = samples.to_vec();
            agc.process(&mut leveled);
            queue.extend(leveled);
        }
        None => queue.extend(samples),
    }
}

/// Threshold above which the mix bus starts saturating, ~-1 dBFS.
const LIMIT_THRESHOLD: f32 = 0.891;

/// Soft saturation of the summed mix.
///
/// Summing two AGC-levelled sources overshoots full scale more often than
/// summing two raw ones, and a hard `clamp` turns every overshoot into audible
/// distortion. Everything below the threshold passes untouched; above it the
/// curve bends asymptotically towards 1.0, so the output never clips outright.
///
/// This is a memoryless saturator, not a look-ahead limiter: it colours loud
/// peaks rather than transparently ducking ahead of them. That is the right
/// trade here, since it adds no latency and no state to the mix path.
fn soft_clip(sample: f32) -> f32 {
    let magnitude = sample.abs();
    if magnitude <= LIMIT_THRESHOLD {
        return sample;
    }
    let over = (magnitude - LIMIT_THRESHOLD) / (1.0 - LIMIT_THRESHOLD);
    let shaped = LIMIT_THRESHOLD + (1.0 - LIMIT_THRESHOLD) * over.tanh();
    shaped.copysign(sample)
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
        // Loopback stereo: L=0.5, R=0
        mixer.push_loopback(&[0.5, 0.0]);
        // Mic stereo: L=0, R=0.25
        mixer.push_mic(&[0.0, 0.25]);

        let out = mixer.finish();
        assert_eq!(out.len(), 2, "one stereo frame = two samples");
        // Distinct values, both below the saturation threshold, so a swapped
        // pairing would be visible rather than hidden by the limiter.
        assert!((out[0] - 0.5).abs() < f32::EPSILON);
        assert!((out[1] - 0.25).abs() < f32::EPSILON);
    }

    #[test]
    fn soft_clip_passes_normal_levels_untouched() {
        for sample in [0.0, 0.25, -0.5, 0.891, -0.891] {
            assert_eq!(soft_clip(sample), sample);
        }
    }

    #[test]
    fn soft_clip_keeps_overshoots_inside_full_scale() {
        for sample in [1.0_f32, -1.0, 4.0, -12.5] {
            let clipped = soft_clip(sample);
            assert!(
                clipped.abs() <= 1.0,
                "{sample} -> {clipped} overshoots full scale"
            );
            assert!(clipped.abs() < sample.abs(), "{sample} was not reduced");
            assert_eq!(clipped.signum(), sample.signum());
        }
    }

    #[test]
    fn agc_levels_a_quiet_source_before_mixing() {
        let quiet = vec![0.004_f32; CAPTURE_CHUNK_SAMPLES]; // ~-48 dBFS
        let mut raw = Mixer::new(1.0, 1.0);
        let mut leveled = Mixer::new(1.0, 1.0).with_agc(true);

        for _ in 0..200 {
            raw.push_mic(&quiet);
            leveled.push_mic(&quiet);
        }

        let raw_out = raw.finish();
        let leveled_out = leveled.finish();
        assert_eq!(raw_out.len(), leveled_out.len());

        let tail = leveled_out.len() - CAPTURE_CHUNK_SAMPLES;
        assert!(
            leveled_out[tail].abs() > raw_out[tail].abs() * 10.0,
            "AGC did not lift the quiet source ({} vs {})",
            leveled_out[tail],
            raw_out[tail]
        );
    }

    #[test]
    fn trims_excess_loopback_drift() {
        let mut mixer = Mixer::new(1.0, 1.0);
        let excess = MAX_DRIFT_SAMPLES + 4;
        let big = vec![0.1; excess];
        mixer.push_loopback(&big);
        mixer.push_mic(&[0.0, 0.0]);

        let out = mixer.finish();
        // Only the part beyond the pairing window is dropped; the rest is
        // kept and flushed (2 paired + MAX_DRIFT_SAMPLES solo).
        assert_eq!(out.len(), MAX_DRIFT_SAMPLES + 2);
    }

    #[test]
    fn bursty_fragment_delivery_does_not_stretch_the_timeline() {
        // PulseAudio with server-default buffer attributes delivers each
        // stream in ~300 ms fragments, out of phase with the other stream.
        // Both fragments cover the same wall-clock window, so the mixed
        // output must be one window long, not two back to back.
        let frag_chunks = 30; // 300 ms
        let loud = vec![0.2; CAPTURE_CHUNK_SAMPLES];
        let quiet = vec![0.0; CAPTURE_CHUNK_SAMPLES];
        let mut mixer = Mixer::new(1.0, 1.0);
        for _ in 0..20 {
            for _ in 0..frag_chunks {
                mixer.push_loopback(&loud);
            }
            mixer.process(false);
            for _ in 0..frag_chunks {
                mixer.push_mic(&quiet);
            }
            mixer.process(false);
        }

        let out = mixer.finish();
        assert_eq!(out.len(), 20 * frag_chunks * CAPTURE_CHUNK_SAMPLES);
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
        assert!((out[0] - 0.75).abs() < f32::EPSILON);
    }
}
