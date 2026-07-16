use std::time::Instant;

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

/// stage の所要時間を機械可読な統一書式で記録する。
/// この書式はパイプライン改善の効果を過去ログと突き合わせて測るための contract で、
/// 変えると before/after 比較が繋がらなくなる。
fn format_stage_elapsed(stage: &str, secs: f64) -> String {
    format!("stage={} elapsed={:.1}s", stage, secs)
}

/// `f` の実行時間を計測し、成否によらず所要時間を記録して結果をそのまま返す。
/// 中断で終わった stage も計測対象に残すため、記録は `f` の戻り値を見ずに行う。
pub(crate) fn timed<T>(stage: &str, f: impl FnOnce() -> T) -> T {
    let start = Instant::now();
    let result = f();
    log_info(&format_stage_elapsed(stage, start.elapsed().as_secs_f64()));
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_stage_elapsed_uses_key_value_format() {
        assert_eq!(
            format_stage_elapsed("quality_gate", 312.04),
            "stage=quality_gate elapsed=312.0s"
        );
    }

    #[test]
    fn timed_returns_inner_value() {
        assert_eq!(timed("test", || 42), 42);
    }
}
