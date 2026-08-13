use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;

use crossbeam_channel::Receiver;

use crate::audio::mixer::Mixer;
use crate::audio::wav::WavWriter;
use crate::settings::{OutputFormat, Settings};

/// Flush mixed stereo samples to disk in ~100 ms chunks.
const WRITE_CHUNK_SAMPLES: usize = 4800 * 2;

pub fn mix_streams(
    stop: Arc<AtomicBool>,
    loopback_rx: Receiver<Vec<f32>>,
    mic_rx: Receiver<Vec<f32>>,
    output_path: &PathBuf,
) -> Result<u64, String> {
    let settings = Settings::load();
    match settings.format {
        OutputFormat::Wav => mix_streams_wav(stop, loopback_rx, mic_rx, output_path, settings.agc),
        OutputFormat::Opus => crate::audio::opus::mix_streams_opus(
            stop,
            loopback_rx,
            mic_rx,
            output_path,
            settings.bitrate_kbps,
            settings.agc,
        ),
    }
}

fn mix_streams_wav(
    stop: Arc<AtomicBool>,
    loopback_rx: Receiver<Vec<f32>>,
    mic_rx: Receiver<Vec<f32>>,
    output_path: &PathBuf,
    agc: bool,
) -> Result<u64, String> {
    let mut mixer = Mixer::new(1.0, 0.85).with_agc(agc);
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
    let mut got_any = false;
    while let Ok(chunk) = loopback_rx.try_recv() {
        mixer.push_loopback(&chunk);
        got_any = true;
    }
    while let Ok(chunk) = mic_rx.try_recv() {
        mixer.push_mic(&chunk);
        got_any = true;
    }
    if got_any {
        mixer.process(false);
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
