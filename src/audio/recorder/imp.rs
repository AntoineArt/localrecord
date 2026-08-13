use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crossbeam_channel::{unbounded, Receiver, Sender};
use wasapi::*;

use crate::audio::convert::{SampleKind, ToStereo48k};
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

const CHUNK_FRAMES: usize = 480; // 10 ms at 48 kHz stereo

fn capture_mic(stop: Arc<AtomicBool>, tx: Sender<Vec<f32>>) -> Result<(), String> {
    let _ = initialize_mta().ok();
    let enumerator = DeviceEnumerator::new().map_err(|e| e.to_string())?;
    let device = enumerator
        .get_default_device(&Direction::Capture)
        .map_err(|e| e.to_string())?;
    let mut audio_client = device.get_iaudioclient().map_err(|e| e.to_string())?;
    let desired_format = WaveFormat::new(32, 32, &SampleType::Float, 48_000, 2, None);
    let blockalign = desired_format.get_blockalign() as usize;

    const BUFFER_HNS: i64 = 1_000_000;
    let mode = StreamMode::EventsShared {
        autoconvert: true,
        buffer_duration_hns: BUFFER_HNS,
    };
    audio_client
        .initialize_client(&desired_format, &Direction::Capture, &mode)
        .map_err(|e| e.to_string())?;
    let event = audio_client.set_get_eventhandle().map_err(|e| e.to_string())?;
    let capture_client = audio_client
        .get_audiocaptureclient()
        .map_err(|e| e.to_string())?;
    audio_client.start_stream().map_err(|e| e.to_string())?;

    let mut scratch = vec![0u8; blockalign * 48_000];
    let mut pending: Vec<f32> = Vec::with_capacity(CHUNK_FRAMES * 2 * 4);

    while !stop.load(Ordering::SeqCst) {
        let _ = event.wait_for_event(50);
        drain_available_packets(&capture_client, &mut scratch, blockalign, |bytes, silent, _| {
            append_packet_samples(&mut pending, bytes, silent);
        })?;
        if flush_pending_chunks(&mut pending, &tx, false) {
            break;
        }
    }

    drain_available_packets(&capture_client, &mut scratch, blockalign, |bytes, silent, _| {
        append_packet_samples(&mut pending, bytes, silent);
    })?;
    let _ = flush_pending_chunks(&mut pending, &tx, true);
    audio_client.stop_stream().map_err(|e| e.to_string())?;
    Ok(())
}

fn capture_loopback(stop: Arc<AtomicBool>, tx: Sender<Vec<f32>>) -> Result<(), String> {
    let _ = initialize_mta().ok();
    let enumerator = DeviceEnumerator::new().map_err(|e| e.to_string())?;
    let device = enumerator
        .get_default_device(&Direction::Render)
        .map_err(|e| e.to_string())?;
    let mut audio_client = device.get_iaudioclient().map_err(|e| e.to_string())?;

    // Loopback always delivers the render mix format. AUTOCONVERTPCM is ignored
    // or glitchy on that path, which is why desktop audio crackled while the mic
    // (a real capture device) was clean.
    let mix_format = audio_client.get_mixformat().map_err(|e| e.to_string())?;

    // 1 s endpoint buffer, polling. Event-driven loopback with a non-zero
    // duration is outside the WASAPI contract and overruns on the render engine.
    const BUFFER_HNS: i64 = 10_000_000;
    let native = StreamMode::PollingShared {
        autoconvert: false,
        buffer_duration_hns: BUFFER_HNS,
    };
    let (blockalign, mut converter) =
        match audio_client.initialize_client(&mix_format, &Direction::Capture, &native) {
            Ok(()) => (
                mix_format.get_blockalign() as usize,
                converter_from_mix_format(&mix_format),
            ),
            Err(_) => {
                audio_client = device.get_iaudioclient().map_err(|e| e.to_string())?;
                let fallback = WaveFormat::new(32, 32, &SampleType::Float, 48_000, 2, None);
                audio_client
                    .initialize_client(
                        &fallback,
                        &Direction::Capture,
                        &StreamMode::PollingShared {
                            autoconvert: true,
                            buffer_duration_hns: BUFFER_HNS,
                        },
                    )
                    .map_err(|e| e.to_string())?;
                (
                    fallback.get_blockalign() as usize,
                    ToStereo48k::new(2, SampleKind::F32, 48_000),
                )
            }
        };
    let capture_client = audio_client
        .get_audiocaptureclient()
        .map_err(|e| e.to_string())?;
    audio_client.start_stream().map_err(|e| e.to_string())?;

    let poll = poll_interval(&audio_client);
    let mut scratch = vec![0u8; blockalign.max(1) * 48_000];
    let mut pending: Vec<f32> = Vec::with_capacity(CHUNK_FRAMES * 2 * 4);

    // First loopback packet is often flagged discontinuity / startup junk.
    thread::sleep(Duration::from_millis(30));
    discard_available_packets(&capture_client, &mut scratch, blockalign)?;

    while !stop.load(Ordering::SeqCst) {
        drain_available_packets(
            &capture_client,
            &mut scratch,
            blockalign,
            |bytes, silent, frames| {
                converter.push(&mut pending, bytes, silent, frames);
            },
        )?;
        if flush_pending_chunks(&mut pending, &tx, false) {
            break;
        }
        thread::sleep(poll);
    }

    drain_available_packets(
        &capture_client,
        &mut scratch,
        blockalign,
        |bytes, silent, frames| {
            converter.push(&mut pending, bytes, silent, frames);
        },
    )?;
    let _ = flush_pending_chunks(&mut pending, &tx, true);
    audio_client.stop_stream().map_err(|e| e.to_string())?;
    Ok(())
}

fn converter_from_mix_format(fmt: &WaveFormat) -> ToStereo48k {
    let channels = fmt.get_nchannels();
    let bits = fmt.get_bitspersample();
    let blockalign = fmt.get_blockalign();
    let bytes_per_sample = if channels > 0 {
        (blockalign / u32::from(channels)) as u16
    } else {
        4
    };
    let is_float = match fmt.get_subformat() {
        Ok(SampleType::Float) => true,
        Ok(SampleType::Int) => false,
        // Shared-mode mix format is almost always IEEE float. Guessing int
        // here would reinterpret float bits and crackle.
        Err(_) => bits == 32,
    };
    let kind = SampleKind::from_format(is_float, bits, bytes_per_sample);
    ToStereo48k::new(channels, kind, fmt.get_samplespersec())
}

fn poll_interval(audio_client: &AudioClient) -> Duration {
    let def_hns = audio_client
        .get_device_period()
        .map(|(def, _)| def)
        .unwrap_or(100_000);
    let ms = (def_hns / 10_000).clamp(5, 20) as u64;
    Duration::from_millis(ms)
}

fn drain_available_packets(
    capture_client: &AudioCaptureClient,
    scratch: &mut Vec<u8>,
    blockalign: usize,
    mut on_packet: impl FnMut(&[u8], bool, usize),
) -> Result<(), String> {
    loop {
        let frames = match capture_client.get_next_packet_size().map_err(|e| e.to_string())? {
            Some(0) | None => break,
            Some(n) => n as usize,
        };
        let needed = frames.saturating_mul(blockalign.max(1));
        if scratch.len() < needed {
            scratch.resize(needed, 0);
        }
        let (nframes, info) = capture_client
            .read_from_device(scratch)
            .map_err(|e| e.to_string())?;
        if nframes == 0 {
            break;
        }
        let nbytes = nframes as usize * blockalign;
        if nbytes > scratch.len() {
            continue;
        }
        on_packet(&scratch[..nbytes], info.flags.silent, nframes as usize);
    }
    Ok(())
}

fn discard_available_packets(
    capture_client: &AudioCaptureClient,
    scratch: &mut Vec<u8>,
    blockalign: usize,
) -> Result<(), String> {
    drain_available_packets(capture_client, scratch, blockalign, |_, _, _| {})
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
