mod mix_output;

#[cfg(windows)]
#[path = "recorder/imp.rs"]
mod imp;

#[cfg(target_os = "linux")]
#[path = "recorder/imp_linux.rs"]
mod imp_linux;

#[cfg(not(any(windows, target_os = "linux")))]
#[path = "recorder/stub.rs"]
mod stub;

#[cfg(windows)]
pub use imp::Recorder;

#[cfg(target_os = "linux")]
pub use imp_linux::Recorder;

#[cfg(not(any(windows, target_os = "linux")))]
pub use stub::Recorder;
