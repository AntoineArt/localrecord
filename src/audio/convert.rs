//! Convert a WASAPI mix-format packet to 48 kHz stereo f32.
//!
//! Loopback capture must use the render device's mix format. Windows often
//! ignores AUTOCONVERTPCM on loopback, so treating that stream as 48 kHz stereo
//! float produces crackling even when the microphone path is clean.

use crate::audio::pcm::f32_from_le_bytes;

pub const OUTPUT_RATE: u32 = 48_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SampleKind {
    F32,
    I16,
    I24,
    I32,
}

impl SampleKind {
    pub fn from_format(is_float: bool, bits_per_sample: u16, bytes_per_sample: u16) -> Self {
        if is_float {
            return SampleKind::F32;
        }
        match (bits_per_sample, bytes_per_sample) {
            (_, 2) | (16, _) => SampleKind::I16,
            (24, 3) => SampleKind::I24,
            _ => SampleKind::I32,
        }
    }
}

/// Native-format frames in, 48 kHz stereo interleaved f32 out.
pub struct ToStereo48k {
    channels: usize,
    kind: SampleKind,
    bytes_per_sample: usize,
    resampler: LinearResampler,
}

impl ToStereo48k {
    pub fn new(channels: u16, kind: SampleKind, sample_rate: u32) -> Self {
        let channels = channels.max(1) as usize;
        let bytes_per_sample = match kind {
            SampleKind::F32 | SampleKind::I32 => 4,
            SampleKind::I24 => 3,
            SampleKind::I16 => 2,
        };
        Self {
            channels,
            kind,
            bytes_per_sample,
            resampler: LinearResampler::new(sample_rate.max(1)),
        }
    }

    pub fn push(&mut self, out: &mut Vec<f32>, bytes: &[u8], silent: bool, frames: usize) {
        let stereo = if silent || frames == 0 {
            vec![(0.0, 0.0); frames]
        } else {
            self.decode_downmix(bytes, frames)
        };
        self.resampler.push(&stereo, out);
    }
}

impl ToStereo48k {
    fn decode_downmix(&self, bytes: &[u8], frames: usize) -> Vec<(f32, f32)> {
        let frame_bytes = self.channels * self.bytes_per_sample;
        if frame_bytes == 0 {
            return vec![(0.0, 0.0); frames];
        }
        let available = bytes.len() / frame_bytes;
        let n = frames.min(available);
        let mut stereo = Vec::with_capacity(frames);
        let mut tmp = vec![0.0f32; self.channels];
        for i in 0..n {
            let base = i * frame_bytes;
            for ch in 0..self.channels {
                let s = base + ch * self.bytes_per_sample;
                tmp[ch] = decode_sample(self.kind, &bytes[s..]);
            }
            stereo.push(downmix_frame(&tmp));
        }
        while stereo.len() < frames {
            stereo.push((0.0, 0.0));
        }
        stereo
    }
}

fn decode_sample(kind: SampleKind, bytes: &[u8]) -> f32 {
    match kind {
        SampleKind::F32 => {
            if bytes.len() < 4 {
                return 0.0;
            }
            f32_from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
        }
        SampleKind::I16 => {
            if bytes.len() < 2 {
                return 0.0;
            }
            let v = i16::from_le_bytes([bytes[0], bytes[1]]);
            (v as f32 / 32768.0).clamp(-1.0, 1.0)
        }
        SampleKind::I24 => {
            if bytes.len() < 3 {
                return 0.0;
            }
            let extend = if bytes[2] & 0x80 != 0 { 0xFF } else { 0 };
            let v = i32::from_le_bytes([bytes[0], bytes[1], bytes[2], extend]);
            (v as f32 / 8_388_608.0).clamp(-1.0, 1.0)
        }
        SampleKind::I32 => {
            if bytes.len() < 4 {
                return 0.0;
            }
            let v = i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
            (v as f32 / 2_147_483_648.0).clamp(-1.0, 1.0)
        }
    }
}

fn downmix_frame(ch: &[f32]) -> (f32, f32) {
    let (l, r) = match ch.len() {
        0 => (0.0, 0.0),
        1 => (ch[0], ch[0]),
        2 => (ch[0], ch[1]),
        6 => {
            // FL FR FC LFE BL BR
            let l = ch[0] + 0.707 * ch[2] + 0.707 * ch[4] + 0.5 * ch[3];
            let r = ch[1] + 0.707 * ch[2] + 0.707 * ch[5] + 0.5 * ch[3];
            (l, r)
        }
        8 => {
            // FL FR FC LFE BL BR SL SR
            let l = ch[0] + 0.707 * ch[2] + 0.5 * ch[3] + 0.707 * ch[4] + 0.707 * ch[6];
            let r = ch[1] + 0.707 * ch[2] + 0.5 * ch[3] + 0.707 * ch[5] + 0.707 * ch[7];
            (l, r)
        }
        _ => (ch[0], ch[1]),
    };
    (l.clamp(-1.0, 1.0), r.clamp(-1.0, 1.0))
}

struct LinearResampler {
    step: f64,
    pos: f64,
    buf: Vec<(f32, f32)>,
}

impl LinearResampler {
    fn new(in_rate: u32) -> Self {
        Self {
            step: in_rate as f64 / OUTPUT_RATE as f64,
            pos: 0.0,
            buf: Vec::new(),
        }
    }

    fn push(&mut self, frames: &[(f32, f32)], out: &mut Vec<f32>) {
        self.buf.extend_from_slice(frames);
        if self.step <= 0.0 {
            return;
        }
        loop {
            let i = self.pos.floor() as usize;
            let frac = self.pos - i as f64;
            if i >= self.buf.len() {
                break;
            }
            if frac > 0.0 && i + 1 >= self.buf.len() {
                break;
            }
            let (l0, r0) = self.buf[i];
            let (l, r) = if frac == 0.0 {
                (l0, r0)
            } else {
                let (l1, r1) = self.buf[i + 1];
                let f = frac as f32;
                (l0 + (l1 - l0) * f, r0 + (r1 - r0) * f)
            };
            out.push(l);
            out.push(r);
            self.pos += self.step;
        }
        let drop = self.pos.floor() as usize;
        if drop > 0 {
            let drop = drop.min(self.buf.len());
            self.buf.drain(..drop);
            self.pos -= drop as f64;
            if self.pos < 0.0 {
                self.pos = 0.0;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_48k_stereo_float_passthrough() {
        let mut conv = ToStereo48k::new(2, SampleKind::F32, 48_000);
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&0.5f32.to_le_bytes());
        bytes.extend_from_slice(&(-0.25f32).to_le_bytes());
        let mut out = Vec::new();
        conv.push(&mut out, &bytes, false, 1);
        assert_eq!(out.len(), 2);
        assert!((out[0] - 0.5).abs() < 1e-6);
        assert!((out[1] + 0.25).abs() < 1e-6);
    }

    #[test]
    fn silent_packet_is_zeros_at_output_rate() {
        let mut conv = ToStereo48k::new(2, SampleKind::F32, 48_000);
        let garbage = vec![0xFFu8; 8];
        let mut out = Vec::new();
        conv.push(&mut out, &garbage, true, 1);
        assert_eq!(out, vec![0.0, 0.0]);
    }

    #[test]
    fn mono_is_duplicated_to_stereo() {
        let mut conv = ToStereo48k::new(1, SampleKind::F32, 48_000);
        let mut out = Vec::new();
        conv.push(&mut out, &0.4f32.to_le_bytes(), false, 1);
        assert!((out[0] - 0.4).abs() < 1e-6);
        assert!((out[1] - 0.4).abs() < 1e-6);
    }

    #[test]
    fn int16_is_scaled_to_f32() {
        let mut conv = ToStereo48k::new(2, SampleKind::I16, 48_000);
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&16384i16.to_le_bytes());
        bytes.extend_from_slice(&(-16384i16).to_le_bytes());
        let mut out = Vec::new();
        conv.push(&mut out, &bytes, false, 1);
        assert!((out[0] - 0.5).abs() < 0.01);
        assert!((out[1] + 0.5).abs() < 0.01);
    }

    #[test]
    fn surround51_downmixes_center_to_both() {
        let mut conv = ToStereo48k::new(6, SampleKind::F32, 48_000);
        let mut bytes = Vec::new();
        for v in [0.0f32, 0.0, 0.5, 0.0, 0.0, 0.0] {
            bytes.extend_from_slice(&v.to_le_bytes());
        }
        let mut out = Vec::new();
        conv.push(&mut out, &bytes, false, 1);
        assert!((out[0] - 0.3535).abs() < 0.01);
        assert!((out[1] - 0.3535).abs() < 0.01);
    }

    #[test]
    fn upsample_24000_doubles_frame_count() {
        let mut conv = ToStereo48k::new(2, SampleKind::F32, 24_000);
        let mut bytes = Vec::new();
        for v in [0.0f32, 0.0, 1.0, 1.0] {
            bytes.extend_from_slice(&v.to_le_bytes());
        }
        let mut out = Vec::new();
        conv.push(&mut out, &bytes, false, 2);
        assert_eq!(out.len() / 2, 3);
    }

    #[test]
    fn resample_44100_to_48000_keeps_expected_length() {
        let mut conv = ToStereo48k::new(2, SampleKind::F32, 44_100);
        let frames = 441;
        let mut bytes = Vec::with_capacity(frames * 8);
        for _ in 0..frames {
            bytes.extend_from_slice(&0.1f32.to_le_bytes());
            bytes.extend_from_slice(&0.1f32.to_le_bytes());
        }
        let mut out = Vec::new();
        conv.push(&mut out, &bytes, false, frames);
        let out_frames = out.len() / 2;
        assert!(
            (out_frames as i32 - 480).abs() <= 2,
            "expected ~480 frames, got {out_frames}"
        );
    }

    #[test]
    fn sample_kind_from_float_and_int() {
        assert_eq!(SampleKind::from_format(true, 32, 4), SampleKind::F32);
        assert_eq!(SampleKind::from_format(false, 16, 2), SampleKind::I16);
        assert_eq!(SampleKind::from_format(false, 24, 3), SampleKind::I24);
        assert_eq!(SampleKind::from_format(false, 24, 4), SampleKind::I32);
    }
}
