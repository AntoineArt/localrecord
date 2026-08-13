//! SIGUSR1 as a toggle channel, on Linux.
//!
//! The global shortcut goes through the `global-hotkey` crate, which grabs keys
//! via X11. On a native Wayland session nothing reaches it, so the shortcut is
//! simply dead there — and the Linux tray backend (libappindicator) has no
//! `Activate`, so left-clicking the tray icon does nothing either. That leaves
//! the right-click menu as the only way to record, which is not a shortcut.
//!
//! A signal sidesteps all of it: the compositor binds a key to
//! `pkill -USR1 -x localrecord` and the app toggles. Works on X11 and Wayland
//! alike, and needs no IPC surface of its own.
//!
//! The handler only stores into an atomic — the one thing that is safe to do
//! from a signal context. The event loop picks it up on its next tick.

use std::sync::atomic::{AtomicBool, Ordering};

static TOGGLE_REQUESTED: AtomicBool = AtomicBool::new(false);

extern "C" fn on_sigusr1(_signal: libc::c_int) {
    TOGGLE_REQUESTED.store(true, Ordering::SeqCst);
}

pub fn install() {
    // SAFETY: registering a handler whose body is a single atomic store.
    unsafe {
        libc::signal(libc::SIGUSR1, on_sigusr1 as libc::sighandler_t);
    }
}

/// Returns true once per signal received, clearing the request.
pub fn take_toggle_request() -> bool {
    TOGGLE_REQUESTED.swap(false, Ordering::SeqCst)
}
