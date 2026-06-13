#[cfg(windows)]
#[path = "recorder/imp.rs"]
mod imp;

#[cfg(not(windows))]
#[path = "recorder/stub.rs"]
mod stub;

#[cfg(windows)]
pub use imp::Recorder;

#[cfg(not(windows))]
pub use stub::Recorder;
