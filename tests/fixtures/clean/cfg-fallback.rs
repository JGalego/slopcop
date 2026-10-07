/// Raises the open file limit so large page loads do not exhaust descriptors.
#[cfg(unix)]
pub fn raise_file_handle_limit() {
    rlimit::increase_nofile_limit(u64::MAX).ok();
}

/// Other platforms have no per-process descriptor limit to raise.
#[cfg(not(unix))]
pub fn raise_file_handle_limit() {}
