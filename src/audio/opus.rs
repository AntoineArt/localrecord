use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;

use crossbeam_channel::Receiver;

use audiopus::coder::Encoder;
use audiopus::{Application, Bitrate, Channels, SampleRate};
use ogg::writing::PacketWriteEndInfo;
use ogg::PacketWriter;

use crate::audio::mixer::Mixer;

const STREAM_SERIAL: u32 = 0x4C_52_43_44; // "LRCD"
const FRAME_SAMPLES_PER_CHANNEL: usize = 960; // 20 ms at 48 kHz
const FRAME_SAMPLES_STEREO: usize = FRAME_SAMPLES_PER_CHANNEL * 2;
const GRANULE_STEP: u64 = FRAME_SAMPLES_PER_CHANNEL as u64;
const MAX_PACKET_BYTES: usize = 4000;
const INPUT_SAMPLE_RATE: u32 = 48_000;
const WRITE_CHUNK_SAMPLES: usize = 4800 * 2;

pub fn mix_streams_opus(
    stop: Arc<AtomicBool>,
    loopback_rx: Receiver<Vec<f32>>,
    mic_rx: Receiver<Vec<f32>>,
    output_path: &PathBuf,
    bitrate_kbps: u32,
) -> Result<u64, String> {
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    let file = BufWriter::new(File::create(output_path).map_err(|e| e.to_string())?);
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

    let mut mixer = Mixer::new(1.0, 0.85);
    let mut state = OpusEncodeState::new();

    while !stop.load(Ordering::SeqCst) {
        pump_mixer_inputs(&mut mixer, &loopback_rx, &mic_rx);
        for chunk in take_mixer_chunks(&mut mixer) {
            state.pending.extend_from_slice(&chunk);
            encode_pending_frames(&mut state, &encoder, &mut ogg)?;
        }
        thread::sleep(std::time::Duration::from_millis(5));
    }

    pump_mixer_inputs(&mut mixer, &loopback_rx, &mic_rx);
    for chunk in take_mixer_chunks(&mut mixer) {
        state.pending.extend_from_slice(&chunk);
    }
    state.pending.extend_from_slice(&mixer.drain_remaining());
    encode_pending_frames(&mut state, &encoder, &mut ogg)?;

    if !state.pending.is_empty() {
        state.pending.resize(FRAME_SAMPLES_STEREO, 0.0);
        let packet_len = encoder
            .encode_float(&state.pending, &mut state.packet_buf)
            .map_err(|e| e.to_string())?;
        state.granule += GRANULE_STEP;
        state.sample_frames += GRANULE_STEP;
        flush_queued_packet(&mut ogg, &mut state.queued_packet, false)?;
        state.queued_packet = Some((state.packet_buf[..packet_len].to_vec(), state.granule));
    }

    flush_queued_packet(&mut ogg, &mut state.queued_packet, true)?;
    ogg.into_inner().flush().map_err(|e| e.to_string())?;
    Ok(state.sample_frames)
}

struct OpusEncodeState {
    pending: Vec<f32>,
    packet_buf: Vec<u8>,
    sample_frames: u64,
    granule: u64,
    first_audio_page: bool,
    queued_packet: Option<(Vec<u8>, u64)>,
}

impl OpusEncodeState {
    fn new() -> Self {
        Self {
            pending: Vec::with_capacity(FRAME_SAMPLES_STEREO * 2),
            packet_buf: vec![0u8; MAX_PACKET_BYTES],
            sample_frames: 0,
            granule: 0,
            first_audio_page: true,
            queued_packet: None,
        }
    }
}

fn encode_pending_frames(
    state: &mut OpusEncodeState,
    encoder: &Encoder,
    ogg: &mut PacketWriter<BufWriter<File>>,
) -> Result<(), String> {
    while state.pending.len() >= FRAME_SAMPLES_STEREO {
        let frame = state
            .pending
            .drain(..FRAME_SAMPLES_STEREO)
            .collect::<Vec<_>>();
        let packet_len = encoder
            .encode_float(&frame, &mut state.packet_buf)
            .map_err(|e| e.to_string())?;
        state.granule += GRANULE_STEP;
        state.sample_frames += GRANULE_STEP;
        let packet = state.packet_buf[..packet_len].to_vec();

        if state.first_audio_page {
            state.first_audio_page = false;
            ogg.write_packet(
                packet,
                STREAM_SERIAL,
                PacketWriteEndInfo::EndPage,
                state.granule,
            )
            .map_err(|e| e.to_string())?;
        } else {
            flush_queued_packet(ogg, &mut state.queued_packet, false)?;
            state.queued_packet = Some((packet, state.granule));
        }
    }
    Ok(())
}

fn flush_queued_packet(
    ogg: &mut PacketWriter<BufWriter<File>>,
    queued_packet: &mut Option<(Vec<u8>, u64)>,
    end_stream: bool,
) -> Result<(), String> {
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
}

fn take_mixer_chunks(mixer: &mut Mixer) -> Vec<Vec<f32>> {
    let mut chunks = Vec::new();
    loop {
        let chunk = mixer.take_output_chunk(WRITE_CHUNK_SAMPLES);
        if chunk.is_empty() {
            break;
        }
        let len = chunk.len();
        chunks.push(chunk);
        if len < WRITE_CHUNK_SAMPLES {
            break;
        }
    }
    chunks
}

fn pump_mixer_inputs(
    mixer: &mut Mixer,
    loopback_rx: &Receiver<Vec<f32>>,
    mic_rx: &Receiver<Vec<f32>>,
) {
    while let Ok(chunk) = loopback_rx.try_recv() {
        mixer.push_loopback(&chunk);
    }
    while let Ok(chunk) = mic_rx.try_recv() {
        mixer.push_mic(&chunk);
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
