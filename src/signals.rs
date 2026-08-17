//! Signals as a control channel, on Linux.
//!
//! The global shortcut goes through the `global-hotkey` crate, which grabs keys
//! via X11. On a native Wayland session nothing reaches it, so the shortcut is
//! simply dead there — and the Linux tray backend (libappindicator) has no
//! `Activate`, so left-clicking the tray icon does nothing either. That leaves
//! the right-click menu as the only way to record, which is not a shortcut.
//!
//! A signal sidesteps all of it: the compositor binds a key to
//! `pkill -USR1 -x localrecord` and the app toggles. Works on X11 and Wayland
//! alike, and needs no IPC surface of its own. SIGUSR2 flips auto-levelling the
//! same way, so a desktop widget can drive the setting the tray owns instead of
//! editing the settings file behind the app's back — see [`crate::state`] for
//! the other half of that contract, which is how such a widget reads state.
//!
//! Each handler only stores into an atomic — the one thing that is safe to do
//! from a signal context. The event loop picks them up on its next tick.

use std::sync::atomic::{AtomicBool, Ordering};

static TOGGLE_REQUESTED: AtomicBool = AtomicBool::new(false);
static AGC_TOGGLE_REQUESTED: AtomicBool = AtomicBool::new(false);

extern "C" fn on_sigusr1(_signal: libc::c_int) {
    TOGGLE_REQUESTED.store(true, Ordering::SeqCst);
}

extern "C" fn on_sigusr2(_signal: libc::c_int) {
    AGC_TOGGLE_REQUESTED.store(true, Ordering::SeqCst);
}

pub fn install() {
    // SAFETY: registering handlers whose bodies are a single atomic store.
    unsafe {
        libc::signal(libc::SIGUSR1, on_sigusr1 as libc::sighandler_t);
        libc::signal(libc::SIGUSR2, on_sigusr2 as libc::sighandler_t);
    }
}

/// Returns true once per signal received, clearing the request.
pub fn take_toggle_request() -> bool {
    TOGGLE_REQUESTED.swap(false, Ordering::SeqCst)
}

pub fn take_agc_toggle_request() -> bool {
    AGC_TOGGLE_REQUESTED.swap(false, Ordering::SeqCst)
}
