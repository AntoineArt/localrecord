use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use crossbeam_channel::{bounded, Receiver, Sender};
use wasapi::*;

use crate::audio::mixer::Mixer;

pub struct RecordingResult {
    pub samples: Vec<f32>,
    pub duration_secs: f64,
}

pub struct Recorder {
    stop: Arc<AtomicBool>,
    handles: Vec<JoinHandle<()>>,
    mixer_rx: Option<Receiver<Vec<f32>>>,
    started_at: Option<std::time::Instant>,
}

impl Recorder {
    pub fn start() -> Result<Self, String> {
        let _ = initialize_mta();

        let stop = Arc::new(AtomicBool::new(false));
        let (loopback_tx, loopback_rx) = bounded::<Vec<f32>>(64);
        let (mic_tx, mic_rx) = bounded::<Vec<f32>>(64);
        let (mixed_tx, mixed_rx) = bounded::<Vec<f32>>(8);

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
        let mixer_handle = thread::Builder::new()
            .name("mixer".into())
            .spawn(move || {
                mix_streams(stop_mixer, loopback_rx, mic_rx, mixed_tx);
            })
            .map_err(|e| e.to_string())?;

        Ok(Self {
            stop,
            handles: vec![loopback_handle, mic_handle, mixer_handle],
            mixer_rx: Some(mixed_rx),
            started_at: Some(std::time::Instant::now()),
        })
    }

    pub fn stop(mut self) -> Result<RecordingResult, String> {
        self.stop.store(true, Ordering::SeqCst);

        for handle in self.handles.drain(..) {
            handle.join().map_err(|_| "Capture thread panicked".to_string())?;
        }

        let samples = self
            .mixer_rx
            .take()
            .and_then(|rx| rx.recv().ok())
            .unwrap_or_default();

        let duration_secs = self
            .started_at
            .map(|t| t.elapsed().as_secs_f64())
            .unwrap_or(0.0);

        Ok(RecordingResult {
            samples,
            duration_secs,
        })
    }
}

fn mix_streams(
    stop: Arc<AtomicBool>,
    loopback_rx: Receiver<Vec<f32>>,
    mic_rx: Receiver<Vec<f32>>,
    mixed_tx: Sender<Vec<f32>>,
) {
    let mut mixer = Mixer::new(1.0, 0.85);

    while !stop.load(Ordering::SeqCst) {
        while let Ok(chunk) = loopback_rx.try_recv() {
            mixer.push_loopback(&chunk);
        }
        while let Ok(chunk) = mic_rx.try_recv() {
            mixer.push_mic(&chunk);
        }
        thread::sleep(std::time::Duration::from_millis(5));
    }

    while let Ok(chunk) = loopback_rx.try_recv() {
        mixer.push_loopback(&chunk);
    }
    while let Ok(chunk) = mic_rx.try_recv() {
        mixer.push_mic(&chunk);
    }

    let _ = mixed_tx.send(mixer.finish());
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
    const CHUNK_FRAMES: usize = 480; // 10 ms at 48 kHz

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

    // Flush remaining bytes
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
