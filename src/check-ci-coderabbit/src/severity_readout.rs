//! 指摘の重大度を読めたかの集計と、全件を読めなかったときの扱い (順位 603)。
//!
//! 1 件だけ読めないのは本文の揺れでありうるが、全件を読めないのは CodeRabbit の書式変更か
//! 読み取り処理 (`crate::findings::extract_severity`) の不具合である。#625 では書式変更の後、
//! Major を含む全件が約 1 日、正常な値に見える `Info` と読まれ、誰も気付かなかった。
//!
//! 全件を読めないときは、監視をエラーで止めるのではなく、success で終わらせずに原因究明の
//! 案内を出す。利用者が指摘への対応から原因究明へ切り替えられるようにするため。

use serde::Serialize;

use crate::findings::UNKNOWN_SEVERITY;

/// 全件を読めなかったときの案内。summary・stderr・takt の分析に同じ文言で出す。
pub(crate) const UNREADABLE_GUIDANCE: &str = "CodeRabbit の書式変更か、読み取り処理の不具合の疑い。\
指摘への対応の前に、コメント本文の実物と src/check-ci-coderabbit/src/findings.rs を突き合わせて原因を調べる";

/// 重大度の読み取り結果。check-ci の出力と `--list-findings` の出力に載せる。
#[derive(Serialize, Debug, PartialEq, Clone, Default)]
pub(crate) struct SeverityReadout {
    /// レビュー指摘 (スレッドの根) の件数。CodeRabbit の reply は数えない
    pub(crate) review_findings: usize,
    /// そのうち重大度を読めなかった (`Unknown`) 件数
    pub(crate) unknown: usize,
    /// 指摘が 1 件以上あり、全件を読めなかった。読み取りの仕組みの異常を示す印
    pub(crate) unreadable: bool,
}

/// 指摘の一覧から読み取り結果を集計する。一覧はスレッドの根だけであること。
pub(crate) fn read_severities<T>(findings: &[T], severity: impl Fn(&T) -> &str) -> SeverityReadout {
    let review_findings = findings.len();
    let unknown = findings.iter().filter(|f| severity(f) == UNKNOWN_SEVERITY).count();
    SeverityReadout {
        review_findings,
        unknown,
        unreadable: review_findings > 0 && unknown == review_findings,
    }
}

/// 読めなかった指摘があれば、件数付きの警告を stderr に出す。stdout は JSON 専用のため。
pub(crate) fn warn_on_stderr(readout: &SeverityReadout) {
    if let Some(message) = warning_message(readout) {
        eprintln!("[check-ci-coderabbit] {}", message);
    }
}

fn warning_message(readout: &SeverityReadout) -> Option<String> {
    if readout.unknown == 0 {
        return None;
    }
    let base = format!(
        "重大度を読めなかった指摘: {} / {} 件",
        readout.unknown, readout.review_findings
    );
    Some(if readout.unreadable {
        format!("{}。{}", base, UNREADABLE_GUIDANCE)
    } else {
        base
    })
}

/// 全件を読めなかったときは success を止める。success 以外の判定はそのまま返す。
///
/// 指摘の重大度が分からないまま「問題なし」で監視を終えると、読めなかった Major を見逃す。
pub(crate) fn apply_severity_gate(decided: (String, String), readout: &SeverityReadout) -> (String, String) {
    if !readout.unreadable || decided.1 != "stop_monitoring_success" {
        return decided;
    }
    ("action_required".to_string(), "action_required".to_string())
}

/// summary に足す文言。読めなかった指摘が無ければ None。
pub(crate) fn summary_part(readout: &SeverityReadout) -> Option<String> {
    warning_message(readout)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn readout_of(severities: &[&str]) -> SeverityReadout {
        read_severities(severities, |s| s)
    }

    fn success() -> (String, String) {
        ("success".to_string(), "stop_monitoring_success".to_string())
    }

    #[test]
    fn all_unknown_findings_raise_the_unreadable_mark() {
        let r = readout_of(&["Unknown", "Unknown"]);
        assert_eq!(r, SeverityReadout { review_findings: 2, unknown: 2, unreadable: true });
    }

    #[test]
    fn one_readable_finding_keeps_the_mark_down() {
        let r = readout_of(&["Unknown", "Major"]);
        assert_eq!(r, SeverityReadout { review_findings: 2, unknown: 1, unreadable: false });
    }

    /// 指摘 0 件は「全件を読めなかった」ではない (読むものが無い)。
    #[test]
    fn no_findings_keeps_the_mark_down() {
        assert_eq!(readout_of(&[]), SeverityReadout::default());
    }

    /// `Info` / `Trivial` は読めた重大度であり、`Unknown` に数えない。
    #[test]
    fn info_and_trivial_are_not_counted_as_unknown() {
        let r = readout_of(&["Info", "Trivial"]);
        assert_eq!(r.unknown, 0);
        assert!(!r.unreadable);
    }

    #[test]
    fn unreadable_mark_blocks_success() {
        let r = readout_of(&["Unknown"]);
        assert_eq!(
            apply_severity_gate(success(), &r),
            ("action_required".to_string(), "action_required".to_string())
        );
    }

    #[test]
    fn partial_unknown_does_not_change_the_action() {
        let r = readout_of(&["Unknown", "Minor"]);
        assert_eq!(apply_severity_gate(success(), &r), success());
    }

    /// success 以外 (監視の継続など) は変えない。止めるのは success だけ。
    #[test]
    fn gate_leaves_non_success_actions_alone() {
        let r = readout_of(&["Unknown"]);
        let pending = ("pending".to_string(), "continue_monitoring".to_string());
        assert_eq!(apply_severity_gate(pending.clone(), &r), pending);
    }

    #[test]
    fn summary_counts_unknown_and_guides_only_when_all_are_unreadable() {
        let partial = summary_part(&readout_of(&["Unknown", "Minor"])).unwrap();
        assert_eq!(partial, "重大度を読めなかった指摘: 1 / 2 件");
        let all = summary_part(&readout_of(&["Unknown"])).unwrap();
        assert!(all.starts_with("重大度を読めなかった指摘: 1 / 1 件。"));
        assert!(all.contains("findings.rs を突き合わせて原因を調べる"));
        assert_eq!(summary_part(&readout_of(&["Major"])), None);
    }
}
