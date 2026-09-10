//! Linux task names are limited to 15 bytes, including the visible version.
//! Keep the executable path stable for launchers and autostart.

pub const NAME: &str = concat!("localrec-", env!("CARGO_PKG_VERSION"));

pub fn set() {
    let name = std::ffi::CString::new(NAME).expect("process name contains no NUL");
    if unsafe { libc::prctl(libc::PR_SET_NAME, name.as_ptr(), 0, 0, 0) } != 0 {
        crate::log::error(&format!(
            "Could not set process name: {}",
            std::io::Error::last_os_error()
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn process_name_includes_the_full_version_without_truncation() {
        assert!(NAME.len() <= 15, "Linux task names allow at most 15 bytes");
        assert!(NAME.ends_with(crate::VERSION));
        std::thread::spawn(|| {
            set();
            let tid = unsafe { libc::syscall(libc::SYS_gettid) };
            let name = std::fs::read_to_string(format!("/proc/self/task/{tid}/comm")).unwrap();
            assert_eq!(name.trim(), NAME);
        })
        .join()
        .unwrap();
    }
}
