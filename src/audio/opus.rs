use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

use audiopus::coder::Encoder;
use audiopus::{Application, Bitrate, Channels, SampleRate};
use ogg::writing::PacketWriteEndInfo;
use ogg::PacketWriter;

const STREAM_SERIAL: u32 = 0x4C_52_43_44; // "LRCD"
const FRAME_SAMPLES_PER_CHANNEL: usize = 960; // 20 ms at 48 kHz
const FRAME_SAMPLES_STEREO: usize = FRAME_SAMPLES_PER_CHANNEL * 2;
const GRANULE_STEP: u64 = FRAME_SAMPLES_PER_CHANNEL as u64;
const MAX_PACKET_BYTES: usize = 4000;
const INPUT_SAMPLE_RATE: u32 = 48_000;

pub struct OpusStreamWriter {
    encoder: Encoder,
    ogg: PacketWriter<BufWriter<File>>,
    pending: Vec<f32>,
    packet_buf: Vec<u8>,
    sample_frames: u64,
    granule: u64,
    first_audio_page: bool,
    queued_packet: Option<(Vec<u8>, u64)>,
}

impl OpusStreamWriter {
    pub fn create(path: &Path, bitrate_kbps: u32) -> Result<Self, String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }

        let file = BufWriter::new(File::create(path).map_err(|e| e.to_string())?);
        let mut ogg = PacketWriter::new(file);

        let mut encoder = Encoder::new(SampleRate::Hz48000, Channels::Stereo, Application::Audio)
            .map_err(|e| e.to_string())?;
        encoder
            .set_bitrate(Bitrate::BitsPerSecond(
                i32::try_from(bitrate_kbps.saturating_mul(1000)).unwrap_or(64_000),
            ))
            .map_err(|e| e.to_string())?;
        encoder.enable_vbr().map_err(|e| e.to_string())?;

        let pre_skip =
            u16::try_from(encoder.lookahead().map_err(|e| e.to_string())?).unwrap_or(312);

        ogg.write_packet(
            build_opus_head(2, INPUT_SAMPLE_RATE, pre_skip),
            STREAM_SERIAL,
            PacketWriteEndInfo::EndPage,
            0,
        )
        .map_err(|e| e.to_string())?;

        ogg.write_packet(
            build_opus_tags("LocalRecord"),
            STREAM_SERIAL,
            PacketWriteEndInfo::EndPage,
            0,
        )
        .map_err(|e| e.to_string())?;

        Ok(Self {
            encoder,
            ogg,
            pending: Vec::with_capacity(FRAME_SAMPLES_STEREO * 2),
            packet_buf: vec![0u8; MAX_PACKET_BYTES],
            sample_frames: 0,
            granule: 0,
            first_audio_page: true,
            queued_packet: None,
        })
    }

    fn flush_queued(&mut self, end_stream: bool) -> Result<(), String> {
        let Some((packet, granule_pos)) = self.queued_packet.take() else {
            return Ok(());
        };
        let end_info = if end_stream {
            PacketWriteEndInfo::EndStream
        } else {
            PacketWriteEndInfo::NormalPacket
        };
        self.ogg
            .write_packet(packet, STREAM_SERIAL, end_info, granule_pos)
            .map_err(|e| e.to_string())
    }

    fn queue_packet(&mut self, packet: Vec<u8>, granule_pos: u64) -> Result<(), String> {
        self.flush_queued(false)?;
        self.queued_packet = Some((packet, granule_pos));
        Ok(())
    }

    pub fn write_samples(&mut self, samples: &[f32]) -> Result<(), String> {
        self.pending.extend_from_slice(samples);
        while self.pending.len() >= FRAME_SAMPLES_STEREO {
            let frame = self
                .pending
                .drain(..FRAME_SAMPLES_STEREO)
                .collect::<Vec<_>>();
            let packet_len = self
                .encoder
                .encode_float(&frame, &mut self.packet_buf)
                .map_err(|e| e.to_string())?;
            self.granule += GRANULE_STEP;
            self.sample_frames += GRANULE_STEP;
            let packet = self.packet_buf[..packet_len].to_vec();

            if self.first_audio_page {
                self.first_audio_page = false;
                self.ogg
                    .write_packet(
                        packet,
                        STREAM_SERIAL,
                        PacketWriteEndInfo::EndPage,
                        self.granule,
                    )
                    .map_err(|e| e.to_string())?;
            } else {
                self.queue_packet(packet, self.granule)?;
            }
        }
        Ok(())
    }

    pub fn finalize(mut self) -> Result<u64, String> {
        if !self.pending.is_empty() {
            self.pending.resize(FRAME_SAMPLES_STEREO, 0.0);
            let packet_len = self
                .encoder
                .encode_float(&self.pending, &mut self.packet_buf)
                .map_err(|e| e.to_string())?;
            self.granule += GRANULE_STEP;
            self.sample_frames += GRANULE_STEP;
            self.queue_packet(self.packet_buf[..packet_len].to_vec(), self.granule)?;
        }

        self.flush_queued(true)?;
        self.ogg.into_inner().flush().map_err(|e| e.to_string())?;
        Ok(self.sample_frames)
    }
}

fn build_opus_head(channels: u8, input_sample_rate: u32, pre_skip: u16) -> Vec<u8> {
    let mut head = Vec::with_capacity(19);
    head.extend_from_slice(b"OpusHead");
    head.push(1);
    head.push(channels);
    head.extend_from_slice(&pre_skip.to_le_bytes());
    head.extend_from_slice(&input_sample_rate.to_le_bytes());
    head.extend_from_slice(&0i16.to_le_bytes());
    head.push(0);
    head
}

fn build_opus_tags(vendor: &str) -> Vec<u8> {
    let vendor_bytes = vendor.as_bytes();
    let mut tags = Vec::with_capacity(8 + 4 + vendor_bytes.len() + 4);
    tags.extend_from_slice(b"OpusTags");
    tags.extend_from_slice(&(vendor_bytes.len() as u32).to_le_bytes());
    tags.extend_from_slice(vendor_bytes);
    tags.extend_from_slice(&0u32.to_le_bytes());
    tags
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opus_head_is_19_bytes() {
        let head = build_opus_head(2, 48_000, 312);
        assert_eq!(head.len(), 19);
        assert_eq!(&head[0..8], b"OpusHead");
    }
}
