use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use crossbeam_channel::{unbounded, Receiver, Sender};
use libpulse_binding as pulse;
use libpulse_simple_binding as psimple;
use pulse::sample::{Format, Spec};

use crate::audio::pcm::bytes_to_f32;

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

const SAMPLE_RATE: u32 = 48_000;
const CHANNELS: u8 = 2;
const CHUNK_FRAMES: usize = 480;
const CHUNK_BYTES: usize = CHUNK_FRAMES * CHANNELS as usize * 4;

impl Recorder {
    pub fn start() -> Result<Self, String> {
        let output_path = crate::config::recording_filename();
        let stop = Arc::new(AtomicBool::new(false));
        let (loopback_tx, loopback_rx) = unbounded::<Vec<f32>>();
        let (mic_tx, mic_rx) = unbounded::<Vec<f32>>();
        let (result_tx, result_rx) = unbounded::<Result<u64, String>>();

        let stop_loopback = Arc::clone(&stop);
        let loopback_handle = thread::Builder::new()
            .name("loopback".into())
            .spawn(move || {
                if let Err(err) = capture_pulse(
                    stop_loopback,
                    loopback_tx,
                    "@DEFAULT_MONITOR@",
                    "Desktop audio",
                ) {
                    eprintln!("Loopback capture error: {err}");
                }
            })
            .map_err(|e| e.to_string())?;

        let stop_mic = Arc::clone(&stop);
        let mic_handle = thread::Builder::new()
            .name("mic".into())
            .spawn(move || {
                if let Err(err) =
                    capture_pulse(stop_mic, mic_tx, "@DEFAULT_SOURCE@", "Microphone")
                {
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

fn capture_pulse(
    stop: Arc<AtomicBool>,
    tx: Sender<Vec<f32>>,
    device: &str,
    stream_name: &str,
) -> Result<(), String> {
    let spec = Spec {
        format: Format::F32le,
        channels: CHANNELS,
        rate: SAMPLE_RATE,
    };

    if !spec.is_valid() {
        return Err("Invalid PulseAudio sample spec".to_string());
    }

    let simple = psimple::Simple::new(
        None,
        "localrecord",
        pulse::stream::Direction::Record,
        Some(device),
        stream_name,
        &spec,
        None,
        None,
    )
    .map_err(|e| format!("PulseAudio open failed for {device}: {e}"))?;

    let mut byte_buf = vec![0u8; CHUNK_BYTES];

    while !stop.load(Ordering::SeqCst) {
        match simple.read(&mut byte_buf) {
            Ok(()) => {
                let samples = bytes_to_f32(&byte_buf);
                if tx.send(samples).is_err() {
                    break;
                }
            }
            Err(_err) if stop.load(Ordering::SeqCst) => break,
            Err(err) => return Err(format!("PulseAudio read failed: {err}")),
        }
    }

    Ok(())
}
