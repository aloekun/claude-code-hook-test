pub(crate) fn log_stage(stage: &str, message: &str) {
    eprintln!("[push-runner] [{}] {}", stage, message);
}

pub(crate) fn log_step(group: &str, status: &str, message: &str) {
    if message.is_empty() {
        eprintln!("[push-runner]   [{}] {}", group, status);
    } else {
        eprintln!("[push-runner]   [{}] {} — {}", group, status, message);
    }
}

pub(crate) fn log_info(message: &str) {
    eprintln!("[push-runner] {}", message);
}
