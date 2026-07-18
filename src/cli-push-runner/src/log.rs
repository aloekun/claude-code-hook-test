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

/// stage の所要時間を contract 書式で stderr に 1 行出力する (T0)。計測は
/// `RunMetrics::timed` (R3) が行い、そこから本関数で stderr 行を出しつつ JSONL へ永続化する。
pub(crate) fn log_stage_elapsed(stage: &str, secs: f64) {
    log_info(&format_stage_elapsed(stage, secs));
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
}
