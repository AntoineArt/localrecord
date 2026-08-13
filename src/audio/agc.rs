//! Automatic gain control, applied per source before mixing.
//!
//! # Why
//!
//! WASAPI hands us whatever level the operating system happens to produce. The
//! microphone level depends on the device's analog gain and on the Windows input
//! slider; the loopback level depends on the system output volume and on the per
//! application sliders of whatever is playing. Nothing keeps those two in step,
//! so the two sources routinely land tens of dB apart — and video conferencing
//! apps hide this, because they run their own AGC before anyone hears the mic.
//!
//! The goal here is *equalisation*: both sources should land at a comparable
//! level in the recording, whatever they came in at. That is why the gain range
//! is symmetric — a source that is too loud gets pulled down just as a source
//! that is too quiet gets pushed up.
//!
//! # How
//!
//! One instance per source, applied in [`crate::audio::mixer::Mixer`] as chunks
//! arrive. For every stereo frame:
//!
//! 1. Track the input power with a one-pole averager ([`ENVELOPE_TC`]) to get a
//!    running RMS estimate. Both channels feed the same estimate and receive the
//!    same gain, so the stereo image is preserved.
//! 2. Derive the gain that would put that RMS at [`TARGET_RMS_DBFS`], clamped to
//!    [`MIN_GAIN_DB`]..=[`MAX_GAIN_DB`].
//! 3. Move the applied gain towards that target with asymmetric smoothing: fast
//!    when turning down ([`ATTACK_TC`]), slow when turning up ([`RELEASE_TC`]).
//!    Turning up slowly is what keeps the result from pumping between words.
//!
//! # Noise gate
//!
//! Below [`GATE_DBFS`] the gain is frozen rather than raised. Without this the
//! AGC would spend every silence winding up to maximum gain — amplifying room
//! noise and hiss, then slamming the next word. It matters especially for the
//! loopback stream, which is *digital silence* whenever nothing is playing: an
//! ungated AGC would sit at +30 dB and detonate on the first note.
//!
//! # Known trade-offs
//!
//! - Amplifying a quiet source amplifies its noise floor with it. The AGC makes
//!   a badly configured microphone audible, not clean. It is a safety net, not a
//!   substitute for setting the input gain correctly.
//! - Per-source levelling deliberately discards the natural balance between the
//!   two sources. Turning the system volume down mid-recording no longer makes
//!   the desktop audio quieter in the file, it just makes the AGC compensate.
//! - Two takes of the same material no longer produce identical files.
//!
//! Because of those, this is opt-out via the `agc` setting.

/// Level both sources aim for, as RMS. Chosen to leave ~20 dB of headroom for
/// peaks, so a normal speech crest factor lands just under full scale.
const TARGET_RMS_DBFS: f32 = -20.0;

/// Ceiling on boost. Enough to rescue a source ~35 dB below where it should be,
/// while stopping short of the range where only noise is left to amplify.
const MAX_GAIN_DB: f32 = 30.0;

/// Floor on gain. Attenuation matters as much as boost: equalising two sources
/// means pulling the loud one down as well as pushing the quiet one up.
const MIN_GAIN_DB: f32 = -20.0;

/// Below this input RMS the gain is held instead of raised. See the noise gate
/// section above.
const GATE_DBFS: f32 = -55.0;

/// Averaging window for the RMS estimate. Long enough to ignore individual
/// syllables, short enough to track a source that changes level.
const ENVELOPE_TC: f32 = 0.150;

/// Smoothing when the gain needs to come *down*. Fast, so a sudden loud passage
/// is caught before it eats the headroom.
const ATTACK_TC: f32 = 0.050;

/// Smoothing when the gain needs to go *up*. Deliberately slow — this is the
/// constant that decides whether the result breathes between words or not.
const RELEASE_TC: f32 = 2.0;

const SAMPLE_RATE: f32 = 48_000.0;

pub struct Agc {
    gain: f32,
    /// Mean square of the input, smoothed over `ENVELOPE_TC`.
    envelope: f32,
    /// Whether the gate has ever opened. Until it does, the first burst of audio
    /// snaps the gain to target instead of ramping over `RELEASE_TC`, so a
    /// recording does not start with two seconds of under-levelled audio.
    primed: bool,
    envelope_coeff: f32,
    attack_coeff: f32,
    release_coeff: f32,
    target_rms: f32,
    gate_rms: f32,
    min_gain: f32,
    max_gain: f32,
}

impl Default for Agc {
    fn default() -> Self {
        Self::new()
    }
}

impl Agc {
    pub fn new() -> Self {
        Self {
            gain: 1.0,
            envelope: 0.0,
            primed: false,
            envelope_coeff: one_pole_coeff(ENVELOPE_TC),
            attack_coeff: one_pole_coeff(ATTACK_TC),
            release_coeff: one_pole_coeff(RELEASE_TC),
            target_rms: db_to_linear(TARGET_RMS_DBFS),
            gate_rms: db_to_linear(GATE_DBFS),
            min_gain: db_to_linear(MIN_GAIN_DB),
            max_gain: db_to_linear(MAX_GAIN_DB),
        }
    }

    /// Apply gain in place to interleaved stereo f32 samples.
    ///
    /// A trailing odd sample (only possible on the final flush) is given the
    /// current gain without updating the envelope.
    pub fn process(&mut self, samples: &mut [f32]) {
        for frame in samples.chunks_mut(2) {
            let power = match frame {
                [l, r] => 0.5 * (*l * *l + *r * *r),
                _ => {
                    frame[0] *= self.gain;
                    continue;
                }
            };

            self.envelope =
                self.envelope_coeff * self.envelope + (1.0 - self.envelope_coeff) * power;
            let rms = self.envelope.sqrt();

            if rms > self.gate_rms {
                let target = (self.target_rms / rms).clamp(self.min_gain, self.max_gain);
                if self.primed {
                    // Coming down is an emergency, going up is not.
                    let coeff = if target < self.gain {
                        self.attack_coeff
                    } else {
                        self.release_coeff
                    };
                    self.gain = coeff * self.gain + (1.0 - coeff) * target;
                } else {
                    self.gain = target;
                    self.primed = true;
                }
            }

            for sample in frame {
                *sample *= self.gain;
            }
        }
    }

    #[cfg(test)]
    fn gain(&self) -> f32 {
        self.gain
    }
}

/// Coefficient of a one-pole smoother reaching ~63% of a step in `seconds`.
fn one_pole_coeff(seconds: f32) -> f32 {
    (-1.0 / (SAMPLE_RATE * seconds)).exp()
}

fn db_to_linear(db: f32) -> f32 {
    10.0_f32.powf(db / 20.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Interleaved stereo square wave of constant magnitude, so RMS == amplitude.
    fn tone(amplitude: f32, frames: usize) -> Vec<f32> {
        (0..frames * 2)
            .map(|i| if (i / 2) % 2 == 0 { amplitude } else { -amplitude })
            .collect()
        }

    fn rms(samples: &[f32]) -> f32 {
        let sum: f32 = samples.iter().map(|s| s * s).sum();
        (sum / samples.len() as f32).sqrt()
    }

    fn linear_to_db(linear: f32) -> f32 {
        20.0 * linear.log10()
    }

    #[test]
    fn raises_a_quiet_source_towards_the_target() {
        let mut agc = Agc::new();
        // 5 seconds at -40 dBFS, i.e. 20 dB below target.
        let mut samples = tone(db_to_linear(-40.0), 48_000 * 5);
        agc.process(&mut samples);

        // Measure the last second, once the gain has settled.
        let tail = &samples[samples.len() - 48_000 * 2..];
        let level = linear_to_db(rms(tail));
        assert!(
            (level - TARGET_RMS_DBFS).abs() < 1.0,
            "expected ~{TARGET_RMS_DBFS} dBFS, got {level}"
        );
    }

    #[test]
    fn lowers_a_loud_source_towards_the_target() {
        let mut agc = Agc::new();
        let mut samples = tone(db_to_linear(-3.0), 48_000 * 5);
        agc.process(&mut samples);

        let tail = &samples[samples.len() - 48_000 * 2..];
        let level = linear_to_db(rms(tail));
        assert!(
            (level - TARGET_RMS_DBFS).abs() < 1.0,
            "expected ~{TARGET_RMS_DBFS} dBFS, got {level}"
        );
    }

    /// The whole point: two sources that arrive 30 dB apart leave together.
    #[test]
    fn equalises_two_sources_that_start_far_apart() {
        let mut quiet_agc = Agc::new();
        let mut loud_agc = Agc::new();
        let mut quiet = tone(db_to_linear(-45.0), 48_000 * 5);
        let mut loud = tone(db_to_linear(-15.0), 48_000 * 5);

        quiet_agc.process(&mut quiet);
        loud_agc.process(&mut loud);

        let window = 48_000 * 2;
        let quiet_level = linear_to_db(rms(&quiet[quiet.len() - window..]));
        let loud_level = linear_to_db(rms(&loud[loud.len() - window..]));
        assert!(
            (quiet_level - loud_level).abs() < 1.5,
            "sources still {} dB apart ({quiet_level} vs {loud_level})",
            (quiet_level - loud_level).abs()
        );
    }

    #[test]
    fn holds_gain_through_digital_silence() {
        let mut agc = Agc::new();
        let mut silence = vec![0.0_f32; 48_000 * 2 * 10];
        agc.process(&mut silence);

        assert_eq!(agc.gain(), 1.0, "gain wound up while nothing was playing");
        assert!(silence.iter().all(|s| *s == 0.0));
    }

    #[test]
    fn does_not_amplify_a_signal_below_the_gate() {
        let mut agc = Agc::new();
        let mut hiss = tone(db_to_linear(-70.0), 48_000 * 5);
        agc.process(&mut hiss);

        assert_eq!(agc.gain(), 1.0, "room noise was boosted");
    }

    #[test]
    fn never_exceeds_the_gain_ceiling() {
        let mut agc = Agc::new();
        // Just above the gate, so the gate opens but the target is unreachable.
        let mut samples = tone(db_to_linear(-54.0), 48_000 * 5);
        agc.process(&mut samples);

        assert!(agc.gain() <= db_to_linear(MAX_GAIN_DB) + f32::EPSILON);
    }

    #[test]
    fn applies_one_gain_to_both_channels() {
        let mut agc = Agc::new();
        // Hard-panned left: the right channel must stay silent, not get its own gain.
        let mut samples: Vec<f32> = (0..48_000 * 2)
            .map(|i| if i % 2 == 0 { 0.01 } else { 0.0 })
            .collect();
        agc.process(&mut samples);

        assert!(samples.iter().skip(1).step_by(2).all(|s| *s == 0.0));
    }

    #[test]
    fn handles_a_trailing_odd_sample() {
        let mut agc = Agc::new();
        let mut samples = vec![0.1, 0.1, 0.1];
        agc.process(&mut samples);
        assert_eq!(samples.len(), 3);
    }
}
