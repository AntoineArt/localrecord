const SAMPLE_RATE: u32 = 48_000;
const CHANNELS: u16 = 2;
const BITS: u16 = 16;

pub fn encode_wav(samples: &[f32]) -> Vec<u8> {
    let num_samples = samples.len();
    let data_size = (num_samples * (BITS as usize / 8)) as u32;
    let file_size = 36 + data_size;

    let mut wav = Vec::with_capacity((44 + data_size) as usize);
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&file_size.to_le_bytes());
    wav.extend_from_slice(b"WAVE");
    wav.extend_from_slice(b"fmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes()); // PCM
    wav.extend_from_slice(&CHANNELS.to_le_bytes());
    wav.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    let byte_rate = SAMPLE_RATE * CHANNELS as u32 * BITS as u32 / 8;
    wav.extend_from_slice(&byte_rate.to_le_bytes());
    let block_align = CHANNELS * BITS / 8;
    wav.extend_from_slice(&block_align.to_le_bytes());
    wav.extend_from_slice(&BITS.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_size.to_le_bytes());

    for &sample in samples {
        let clamped = sample.clamp(-1.0, 1.0);
        let pcm = (clamped * i16::MAX as f32) as i16;
        wav.extend_from_slice(&pcm.to_le_bytes());
    }

    wav
}

pub fn save_wav(path: &std::path::Path, samples: &[f32]) -> std::io::Result<()> {
    std::fs::write(path, encode_wav(samples))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wav_header_is_valid() {
        let wav = encode_wav(&[0.0, 0.0, 0.5, -0.5]);
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
    }
}
