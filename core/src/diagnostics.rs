//! Read-only process memory counters for opt-in diagnostics; unsupported counters are absent.
pub fn peak_rss() -> Option<u64> {
    #[cfg(unix)]
    {
        let mut usage = std::mem::MaybeUninit::<libc::rusage>::uninit();
        // getrusage initializes the output on success; no shared application state.
        if unsafe { libc::getrusage(libc::RUSAGE_SELF, usage.as_mut_ptr()) } != 0 {
            return None;
        }
        let value = u64::try_from(unsafe { usage.assume_init() }.ru_maxrss).ok()?;
        #[cfg(target_vendor = "apple")]
        {
            Some(value)
        }
        #[cfg(not(target_vendor = "apple"))]
        {
            Some(value * 1024)
        }
    }
    #[cfg(not(unix))]
    {
        None
    }
}
pub fn rss() -> Option<u64> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("ps")
            .args(["-o", "rss=", "-p", &std::process::id().to_string()])
            .output()
            .ok()
            .and_then(|v| String::from_utf8(v.stdout).ok())
            .and_then(|v| v.trim().parse::<u64>().ok())
            .map(|value| value * 1024)
    }
    #[cfg(target_os = "linux")]
    {
        std::fs::read_to_string("/proc/self/status")
            .ok()
            .and_then(|text| {
                text.lines().find_map(|line| {
                    line.strip_prefix("VmRSS:")?
                        .split_whitespace()
                        .next()?
                        .parse::<u64>()
                        .ok()
                        .map(|kb| kb * 1024)
                })
            })
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        None
    }
}
