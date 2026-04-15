pub(crate) fn log_info(msg: &str) {
    eprintln!("[post-pr-monitor] {}", msg);
}

/// UTF-8 safe string truncation (truncates at char boundary, not byte boundary)
pub(crate) fn truncate_safe(s: &str, max_chars: usize) -> &str {
    match s.char_indices().nth(max_chars) {
        Some((idx, _)) => &s[..idx],
        None => s,
    }
}
