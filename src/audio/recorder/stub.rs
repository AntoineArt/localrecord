use std::path::PathBuf;

pub struct RecordingResult {
    pub samples: Vec<f32>,
    pub duration_secs: f64,
}

pub struct Recorder;

impl Recorder {
    pub fn start() -> Result<Self, String> {
        Err("LocalRecord only runs on Windows".to_string())
    }

    pub fn stop(self) -> Result<RecordingResult, String> {
        Err("LocalRecord only runs on Windows".to_string())
    }
}

#[allow(dead_code)]
pub fn _stub_path(_: PathBuf) {}
