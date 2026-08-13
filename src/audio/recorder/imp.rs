use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use crossbeam_channel::{unbounded, Receiver, Sender};
use wasapi::*;

use crate::audio::pcm::append_packet_samples;
use crate::config;

use super::mix_output;

pub struct RecordingResult {
    pub path: PathBuf,
    pub duration_secs: f64,
    pub sample_frames: u64,
}

pub struct Recorder {
    stop: Arc<AtomicBool>,
    handles: Vec<JoinHandle<()>>,
    result_rx: Option<Receiver<Result<u64, String>>>,
    output_path: PathBuf,
    started_at: Option<std::time::Instant>,
}

impl Recorder {
    pub fn start() -> Result<Self, String> {
        let _ = initialize_mta();

        let output_path = config::recording_filename();
        let stop = Arc::new(AtomicBool::new(false));
        // Unbounded so capture threads never block on send and miss WASAPI packets.
        let (loopback_tx, loopback_rx) = unbounded::<Vec<f32>>();
        let (mic_tx, mic_rx) = unbounded::<Vec<f32>>();
        let (result_tx, result_rx) = unbounded::<Result<u64, String>>();

        let stop_loopback = Arc::clone(&stop);
        let loopback_handle = thread::Builder::new()
            .name("loopback".into())
            .spawn(move || {
                if let Err(err) = capture_loopback(stop_loopback, loopback_tx) {
                    eprintln!("Loopback capture error: {err}");
                }
            })
            .map_err(|e| e.to_string())?;

        let stop_mic = Arc::clone(&stop);
        let mic_handle = thread::Builder::new()
            .name("mic".into())
            .spawn(move || {
                if let Err(err) = capture_mic(stop_mic, mic_tx) {
                    eprintln!("Mic capture error: {err}");
                }
            })
            .map_err(|e| e.to_string())?;

        let stop_mixer = Arc::clone(&stop);
        let path_for_mixer = output_path.clone();
        let mixer_handle = thread::Builder::new()
            .name("mixer".into())
            .spawn(move || {
                let result = mix_output::mix_streams(stop_mixer, loopback_rx, mic_rx, &path_for_mixer);
                let _ = result_tx.send(result);
            })
            .map_err(|e| e.to_string())?;

        Ok(Self {
            stop,
            handles: vec![loopback_handle, mic_handle, mixer_handle],
            result_rx: Some(result_rx),
            output_path,
            started_at: Some(std::time::Instant::now()),
        })
    }

    pub fn stop(mut self) -> Result<RecordingResult, String> {
        self.stop.store(true, Ordering::SeqCst);

        for handle in self.handles.drain(..) {
            handle
                .join()
                .map_err(|_| "Capture thread panicked".to_string())?;
        }

        let sample_frames = self
            .result_rx
            .take()
            .and_then(|rx| rx.recv().ok())
            .transpose()?
            .unwrap_or(0);

        let duration_secs = self
            .started_at
            .map(|t| t.elapsed().as_secs_f64())
            .unwrap_or(0.0);

        Ok(RecordingResult {
            path: self.output_path,
            duration_secs,
            sample_frames,
        })
    }
}

fn capture_loopback(stop: Arc<AtomicBool>, tx: Sender<Vec<f32>>) -> Result<(), String> {
    let _ = initialize_mta().ok();
    capture_stream(stop, tx, StreamKind::Loopback).map_err(|e| e.to_string())
}

fn capture_mic(stop: Arc<AtomicBool>, tx: Sender<Vec<f32>>) -> Result<(), String> {
    let _ = initialize_mta().ok();
    capture_stream(stop, tx, StreamKind::Microphone).map_err(|e| e.to_string())
}

enum StreamKind {
    Loopback,
    Microphone,
}

const CHUNK_FRAMES: usize = 480; // 10 ms at 48 kHz stereo

fn capture_stream(
    stop: Arc<AtomicBool>,
    tx: Sender<Vec<f32>>,
    kind: StreamKind,
) -> Result<(), wasapi::WasapiError> {
    let enumerator = DeviceEnumerator::new()?;
    let device = match kind {
        StreamKind::Loopback => enumerator.get_default_device(&Direction::Render)?,
        StreamKind::Microphone => enumerator.get_default_device(&Direction::Capture)?,
    };

    let mut audio_client = device.get_iaudioclient()?;
    let desired_format = WaveFormat::new(32, 32, &SampleType::Float, 48_000, 2, None);
    let blockalign = desired_format.get_blockalign() as usize;

    // 100 ms endpoint buffer. The previous min-period size (~3-10 ms) overran
    // whenever a packet drain was late, which sounds like crackling.
    const BUFFER_HNS: i64 = 1_000_000;
    let mode = StreamMode::EventsShared {
        autoconvert: true,
        buffer_duration_hns: BUFFER_HNS,
    };

    let init_direction = &Direction::Capture;
    audio_client.initialize_client(&desired_format, init_direction, &mode)?;
    let event = audio_client.set_get_eventhandle()?;
    let capture_client = audio_client.get_audiocaptureclient()?;
    audio_client.start_stream()?;

    let mut scratch = vec![0u8; blockalign * 48_000];
    let mut pending: Vec<f32> = Vec::with_capacity(CHUNK_FRAMES * 2 * 4);

    while !stop.load(Ordering::SeqCst) {
        let _ = event.wait_for_event(50);
        drain_available_packets(&capture_client, &mut scratch, blockalign, &mut pending)?;
        if flush_pending_chunks(&mut pending, &tx, false) {
            break;
        }
    }

    drain_available_packets(&capture_client, &mut scratch, blockalign, &mut pending)?;
    let _ = flush_pending_chunks(&mut pending, &tx, true);

    audio_client.stop_stream()?;
    Ok(())
}

fn drain_available_packets(
    capture_client: &AudioCaptureClient,
    scratch: &mut Vec<u8>,
    blockalign: usize,
    pending: &mut Vec<f32>,
) -> Result<(), wasapi::WasapiError> {
    loop {
        let frames = match capture_client.get_next_packet_size()? {
            Some(0) | None => break,
            Some(n) => n as usize,
        };
        let needed = frames.saturating_mul(blockalign);
        if scratch.len() < needed {
            scratch.resize(needed, 0);
        }
        let (nframes, info) = capture_client.read_from_device(scratch)?;
        if nframes == 0 {
            break;
        }
        let nbytes = nframes as usize * blockalign;
        if nbytes > scratch.len() {
            continue;
        }
        append_packet_samples(pending, &scratch[..nbytes], info.flags.silent);
    }
    Ok(())
}

/// Returns true if the mixer hung up.
fn flush_pending_chunks(pending: &mut Vec<f32>, tx: &Sender<Vec<f32>>, flush_all: bool) -> bool {
    let chunk_samples = CHUNK_FRAMES * 2;
    loop {
        let take = if flush_all {
            pending.len() - pending.len() % 2
        } else if pending.len() >= chunk_samples {
            chunk_samples
        } else {
            break;
        };
        if take < 2 {
            break;
        }
        let chunk: Vec<f32> = pending.drain(..take).collect();
        if tx.send(chunk).is_err() {
            return true;
        }
        if flush_all {
            break;
        }
    }
    false
}
