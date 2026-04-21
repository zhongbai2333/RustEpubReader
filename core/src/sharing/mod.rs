//! Module for local P2P book sharing and synchronization.
// ── Debug logging with Android logcat support ──

use std::sync::atomic::{AtomicBool, Ordering};

static DEBUG_LOG_ENABLED: AtomicBool = AtomicBool::new(false);

pub fn set_debug_logging_enabled(enabled: bool) {
    DEBUG_LOG_ENABLED.store(enabled, Ordering::Relaxed);
}

pub fn is_debug_logging_enabled() -> bool {
    DEBUG_LOG_ENABLED.load(Ordering::Relaxed)
}

#[cfg(target_os = "android")]
#[link(name = "log")]
unsafe extern "C" {
    fn __android_log_write(
        prio: i32,
        tag: *const std::ffi::c_char,
        text: *const std::ffi::c_char,
    ) -> i32;
}

pub fn share_dbg_log(msg: &str) {
    if is_debug_logging_enabled() {
        eprintln!("[SHARE-DBG] {}", msg);
    }
    #[cfg(target_os = "android")]
    {
        if !is_debug_logging_enabled() {
            return;
        }
        use std::ffi::CString;
        let tag = CString::new("SHARE-DBG").expect("static tag");
        let text = CString::new(format!("[SHARE-DBG] {}", msg))
            .unwrap_or_else(|_| CString::new("[SHARE-DBG] (log error)").expect("static fallback"));
        unsafe {
            __android_log_write(3, tag.as_ptr(), text.as_ptr()); // 3 = DEBUG
        }
    }
}

macro_rules! dbg_log {
    ($($arg:tt)*) => {
        $crate::sharing::share_dbg_log(&format!($($arg)*));
    };
}
pub(crate) use dbg_log;

// ── Module declarations ──

pub mod crypto;
pub mod discovery;
#[cfg(feature = "keychain")]
pub mod keystore;
pub mod peer;
pub mod protocol;

pub use crypto::*;
pub use discovery::*;
pub use peer::*;
pub use protocol::*;

#[cfg(test)]
mod tests {
    use super::{is_debug_logging_enabled, set_debug_logging_enabled};

    #[test]
    fn debug_log_switch_should_toggle() {
        let original = is_debug_logging_enabled();
        set_debug_logging_enabled(false);
        assert!(!is_debug_logging_enabled());
        set_debug_logging_enabled(true);
        assert!(is_debug_logging_enabled());
        set_debug_logging_enabled(original);
    }
}
