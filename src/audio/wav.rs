use std::fs::File;
use std::io::{self, Seek, Write};
use std::path::Path;

const SAMPLE_RATE: u32 = 48_000;
const CHANNELS: u16 = 2;
const BITS: u16 = 16;

fn float_to_pcm16(sample: f32) -> i16 {
    let clamped = sample.clamp(-1.0, 1.0);
    (clamped * i16::MAX as f32) as i16
}

/// Incrementally writes PCM16 stereo audio and patches the WAV header on finalize.
pub struct WavWriter {
    file: File,
    pcm_bytes: u64,
}

impl WavWriter {
    pub fn create(path: &Path) -> io::Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut file = File::create(path)?;
        file.write_all(&placeholder_header(0))?;
        Ok(Self { file, pcm_bytes: 0 })
    }

    pub fn write_samples(&mut self, samples: &[f32]) -> io::Result<()> {
        for &sample in samples {
            self.file.write_all(&float_to_pcm16(sample).to_le_bytes())?;
            self.pcm_bytes += 2;
        }
        Ok(())
    }

    pub fn sample_frames(&self) -> u64 {
        self.pcm_bytes / (CHANNELS as u64 * (BITS as u64 / 8))
    }

    pub fn finalize(mut self) -> io::Result<u64> {
        let header = placeholder_header(self.pcm_bytes as u32);
        self.file.seek(std::io::SeekFrom::Start(0))?;
        self.file.write_all(&header)?;
        self.file.flush()?;
        Ok(self.sample_frames())
    }
}

fn placeholder_header(data_size: u32) -> [u8; 44] {
    let file_size = 36 + data_size;
    let byte_rate = SAMPLE_RATE * CHANNELS as u32 * BITS as u32 / 8;
    let block_align = CHANNELS * BITS / 8;

    let mut header = [0u8; 44];
    header[0..4].copy_from_slice(b"RIFF");
    header[4..8].copy_from_slice(&file_size.to_le_bytes());
    header[8..12].copy_from_slice(b"WAVE");
    header[12..16].copy_from_slice(b"fmt ");
    header[16..20].copy_from_slice(&16u32.to_le_bytes());
    header[20..22].copy_from_slice(&1u16.to_le_bytes());
    header[22..24].copy_from_slice(&CHANNELS.to_le_bytes());
    header[24..28].copy_from_slice(&SAMPLE_RATE.to_le_bytes());
    header[28..32].copy_from_slice(&byte_rate.to_le_bytes());
    header[32..34].copy_from_slice(&block_align.to_le_bytes());
    header[34..36].copy_from_slice(&BITS.to_le_bytes());
    header[36..40].copy_from_slice(b"data");
    header[40..44].copy_from_slice(&data_size.to_le_bytes());
    header
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encode_wav(samples: &[f32]) -> Vec<u8> {
        let num_samples = samples.len();
        let data_size = (num_samples * (BITS as usize / 8)) as u32;
        let file_size = 36 + data_size;

        let mut wav = Vec::with_capacity((44 + data_size) as usize);
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&file_size.to_le_bytes());
        wav.extend_from_slice(b"WAVE");
        wav.extend_from_slice(b"fmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes());
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
            wav.extend_from_slice(&float_to_pcm16(sample).to_le_bytes());
        }

        wav
    }

    #[test]
    fn wav_header_is_valid() {
        let wav = encode_wav(&[0.0, 0.0, 0.5, -0.5]);
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
    }

    #[test]
    fn streaming_writer_matches_encode() {
        let samples = [0.0_f32, 0.25, -0.5, 0.75];
        let encoded = encode_wav(&samples);

        let path = std::env::temp_dir().join("localrecord_wav_writer_test.wav");
        let writer = WavWriter::create(&path).unwrap();
        let mut writer = writer;
        writer.write_samples(&samples).unwrap();
        writer.finalize().unwrap();

        let written = std::fs::read(&path).unwrap();
        assert_eq!(written, encoded);
        let _ = std::fs::remove_file(path);
    }
}
