//! Temp file path generation shared across platforms.

/// Generate a unique temp file tag.
///
/// Format: `{millis}-{rand4hex}`. Unique enough to avoid stale file
/// collisions across CLI invocations. forepaw is a single-capture-per-call
/// CLI, so no counter is needed.
pub fn temp_tag() -> String {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let rand = rand_u16();
    format!("{ts}-{rand:04x}")
}

/// Cheap random u16 without external dependencies.
///
/// Mixes process ID, thread ID, ASLR-randomized stack address,
/// subsecond nanoseconds, and a per-process counter. The counter
/// guarantees uniqueness even when two calls land in the same
/// millisecond on the same thread (e.g. rapid tests).
fn rand_u16() -> u16 {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let count = COUNTER.fetch_add(1, Ordering::Relaxed);

    let pid = std::process::id() as usize;
    let tid = thread_id();
    let stack = &temp_tag as *const _ as usize;
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos() as usize;
    (pid.wrapping_mul(2654435761)
        .wrapping_add(tid)
        .wrapping_add(stack)
        .wrapping_add(count)
        ^ nanos) as u16
}

/// Get current thread ID as a usize.
///
/// `std::thread::current().id()` returns an opaque `ThreadId` without
/// a stable numeric value, so we use platform-specific methods.
#[cfg(unix)]
fn thread_id() -> usize {
    unsafe { libc::pthread_self() as usize }
}

#[cfg(windows)]
fn thread_id() -> usize {
    use windows::Win32::System::Threading::GetCurrentThreadId;
    unsafe { GetCurrentThreadId() as usize }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn temp_tag_format() {
        let tag = temp_tag();
        let parts: Vec<&str> = tag.split('-').collect();
        assert!(parts.len() >= 2, "tag should have millis-random: {tag}");
        // Random suffix should be 4 hex chars
        let suffix = parts.last().unwrap();
        assert_eq!(suffix.len(), 4, "random suffix should be 4 hex chars");
        assert!(
            suffix.chars().all(|c| c.is_ascii_hexdigit()),
            "random suffix should be hex"
        );
    }

    #[test]
    fn temp_tags_differ() {
        let a = temp_tag();
        let b = temp_tag();
        assert_ne!(a, b, "two consecutive temp_tags should differ");
    }
}
