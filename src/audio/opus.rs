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

pub fn write_opus_recording(
    path: &Path,
    bitrate_kbps: u32,
    mut feed: impl FnMut(&mut dyn FnMut(&[f32]) -> Result<(), String>) -> Result<(), String>,
) -> Result<u64, String> {
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

    let pre_skip = u16::try_from(encoder.lookahead().map_err(|e| e.to_string())?).unwrap_or(312);

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

    let mut pending = Vec::with_capacity(FRAME_SAMPLES_STEREO * 2);
    let mut packet_buf = vec![0u8; MAX_PACKET_BYTES];
    let mut sample_frames = 0u64;
    let mut granule = 0u64;
    let mut first_audio_page = true;
    let mut queued_packet: Option<(Vec<u8>, u64)> = None;

    let mut flush_queued = |end_stream: bool| -> Result<(), String> {
        let Some((packet, granule_pos)) = queued_packet.take() else {
            return Ok(());
        };
        let end_info = if end_stream {
            PacketWriteEndInfo::EndStream
        } else {
            PacketWriteEndInfo::NormalPacket
        };
        ogg.write_packet(packet, STREAM_SERIAL, end_info, granule_pos)
            .map_err(|e| e.to_string())
    };

    let mut queue_packet = |packet: Vec<u8>, granule_pos: u64| -> Result<(), String> {
        flush_queued(false)?;
        queued_packet = Some((packet, granule_pos));
        Ok(())
    };

    let mut write_chunk = |samples: &[f32]| -> Result<(), String> {
        pending.extend_from_slice(samples);
        while pending.len() >= FRAME_SAMPLES_STEREO {
            let frame = pending.drain(..FRAME_SAMPLES_STEREO).collect::<Vec<_>>();
            let packet_len = encoder
                .encode_float(&frame, &mut packet_buf)
                .map_err(|e| e.to_string())?;
            granule += GRANULE_STEP;
            sample_frames += GRANULE_STEP;
            let packet = packet_buf[..packet_len].to_vec();

            if first_audio_page {
                first_audio_page = false;
                ogg.write_packet(packet, STREAM_SERIAL, PacketWriteEndInfo::EndPage, granule)
                    .map_err(|e| e.to_string())?;
            } else {
                queue_packet(packet, granule)?;
            }
        }
        Ok(())
    };

    feed(&mut write_chunk)?;

    if !pending.is_empty() {
        pending.resize(FRAME_SAMPLES_STEREO, 0.0);
        let packet_len = encoder
            .encode_float(&pending, &mut packet_buf)
            .map_err(|e| e.to_string())?;
        granule += GRANULE_STEP;
        sample_frames += GRANULE_STEP;
        queue_packet(packet_buf[..packet_len].to_vec(), granule)?;
    }

    flush_queued(true)?;

    ogg.into_inner().flush().map_err(|e| e.to_string())?;
    Ok(sample_frames)
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
