use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use crossbeam_channel::{bounded, Receiver, Sender};
use wasapi::*;

use crate::audio::mixer::Mixer;
use crate::audio::wav::WavWriter;
use crate::config;
use crate::settings::{OutputFormat, Settings};

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
        let (loopback_tx, loopback_rx) = bounded::<Vec<f32>>(64);
        let (mic_tx, mic_rx) = bounded::<Vec<f32>>(64);
        let (result_tx, result_rx) = bounded::<Result<u64, String>>(1);

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
                let result = mix_streams(stop_mixer, loopback_rx, mic_rx, &path_for_mixer);
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

/// Flush mixed stereo samples to disk in ~100 ms chunks.
const WRITE_CHUNK_SAMPLES: usize = 4800 * 2;

fn mix_streams(
    stop: Arc<AtomicBool>,
    loopback_rx: Receiver<Vec<f32>>,
    mic_rx: Receiver<Vec<f32>>,
    output_path: &PathBuf,
) -> Result<u64, String> {
    let settings = Settings::load();
    match settings.format {
        OutputFormat::Wav => mix_streams_wav(stop, loopback_rx, mic_rx, output_path),
        OutputFormat::Opus => crate::audio::opus::mix_streams_opus(
            stop,
            loopback_rx,
            mic_rx,
            output_path,
            settings.bitrate_kbps,
        ),
    }
}

fn mix_streams_wav(
    stop: Arc<AtomicBool>,
    loopback_rx: Receiver<Vec<f32>>,
    mic_rx: Receiver<Vec<f32>>,
    output_path: &PathBuf,
) -> Result<u64, String> {
    let mut mixer = Mixer::new(1.0, 0.85);
    let mut writer = WavWriter::create(output_path).map_err(|e| e.to_string())?;

    while !stop.load(Ordering::SeqCst) {
        pump_mixer_inputs(&mut mixer, &loopback_rx, &mic_rx);
        flush_mixer_to_sink(&mut mixer, |chunk| {
            writer.write_samples(chunk).map_err(|e| e.to_string())
        })?;
        thread::sleep(std::time::Duration::from_millis(5));
    }

    pump_mixer_inputs(&mut mixer, &loopback_rx, &mic_rx);
    let tail = mixer.finish();
    writer.write_samples(&tail).map_err(|e| e.to_string())?;
    writer.finalize().map_err(|e| e.to_string())
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

fn flush_mixer_to_sink(
    mixer: &mut Mixer,
    mut write: impl FnMut(&[f32]) -> Result<(), String>,
) -> Result<u64, String> {
    let mut frames = 0u64;
    loop {
        let chunk = mixer.take_output_chunk(WRITE_CHUNK_SAMPLES);
        if chunk.is_empty() {
            break;
        }
        frames += (chunk.len() / 2) as u64;
        write(&chunk)?;
        if chunk.len() < WRITE_CHUNK_SAMPLES {
            break;
        }
    }
    Ok(frames)
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
    let (_, min_time) = audio_client.get_device_period()?;

    let mode = StreamMode::EventsShared {
        autoconvert: true,
        buffer_duration_hns: min_time,
    };

    let init_direction = &Direction::Capture;
    audio_client.initialize_client(&desired_format, init_direction, &mode)?;
    let event = audio_client.set_get_eventhandle()?;
    let capture_client = audio_client.get_audiocaptureclient()?;
    audio_client.start_stream()?;

    let mut byte_queue: VecDeque<u8> = VecDeque::with_capacity(blockalign * 4096);
    const CHUNK_FRAMES: usize = 480; // 10 ms at 48 kHz stereo

    while !stop.load(Ordering::SeqCst) {
        capture_client.read_from_device_to_deque(&mut byte_queue)?;

        while byte_queue.len() >= blockalign * CHUNK_FRAMES {
            let mut chunk_bytes = Vec::with_capacity(blockalign * CHUNK_FRAMES);
            for _ in 0..blockalign * CHUNK_FRAMES {
                if let Some(byte) = byte_queue.pop_front() {
                    chunk_bytes.push(byte);
                }
            }
            let samples = bytes_to_f32_stereo(&chunk_bytes);
            if tx.send(samples).is_err() {
                break;
            }
        }

        if event.wait_for_event(200).is_err() {
            continue;
        }
    }

    if !byte_queue.is_empty() {
        let remaining: Vec<u8> = byte_queue.drain(..).collect();
        let samples = bytes_to_f32_stereo(&remaining);
        let _ = tx.send(samples);
    }

    audio_client.stop_stream()?;
    Ok(())
}

fn bytes_to_f32_stereo(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(4)
        .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect()
}
