//! 分離型 fix commit の pre-create と description 生成。
//!
//! ADR-022 例外条項 (2026-04-20): 自動生成された修正を独立した child commit として
//! 分離する場合に限り、その child commit への description 付与を許可する。
//! 元 commit (= 人間が意図を込めた初回 PR commit) の description は改変しない。
//!
//! pre-takt で `jj new -m "..."` により空 child を作成し、takt が `@` を amend する
//! ことで fix 内容が自動的に child commit へ入る仕組み。

use lib_report_formatter::Finding;

use crate::log::log_info;
use crate::runner::{capture_commit_id, diff_at_is_empty, run_cmd_direct, JJ_CMD_TIMEOUT_SECS};

/// 分離型 fix commit の状態。
///
/// pre-takt で作成を試み、成否を型で表現する。
/// post-takt の分岐 (re-push / abandon / 放置) で消費される。
#[derive(Debug, Clone)]
pub(crate) enum FixCommitState {
    /// 分離を行わなかった (findings なし、takt 未構成、または作成失敗)
    None,
    /// fix commit を pre-create 済み
    Created { commit_id: String },
}

impl FixCommitState {
    pub(crate) fn is_created(&self) -> bool {
        matches!(self, Self::Created { .. })
    }
}

/// fix commit の description を生成する。
///
/// ADR-022 例外の「新規 child commit への自己記述」として、
/// - header ラベル: commit 種別を示す
/// - findings summary: 何を問題と捉え、どれを修正したかの文脈
///
/// の 2 段構成で返す。findings が空なら header のみ返す。
pub(crate) fn build_fix_commit_description(pr_number: Option<u64>, findings: &[Finding]) -> String {
    let header = match pr_number {
        Some(n) => format!("fix(review): apply CodeRabbit fixes for #{}", n),
        None => "fix(review): apply CodeRabbit fixes".to_string(),
    };

    if findings.is_empty() {
        return header;
    }

    let mut body = String::with_capacity(256);
    body.push_str(&header);
    body.push_str("\n\nResolved findings:\n");
    for f in findings {
        body.push_str(&format!(
            "- [{}] {}:{} {}\n",
            f.severity, f.file, f.line, f.issue
        ));
    }
    body.trim_end().to_string()
}

/// pre-takt で fix commit を新規作成する (`jj new -m "..."`)。
///
/// 成功時: `FixCommitState::Created { commit_id }` を返す。@ は空 child を指す状態。
/// 失敗時: `FixCommitState::None` を返す (fallback = 分離なしで元の flow へフォールバック)。
///
/// 失敗要因として想定:
/// - `jj` コマンドが見つからない / 失敗
/// - 直後の `capture_commit_id` 失敗 (この場合は jj new の結果を追跡できないため None 扱い)
pub(crate) fn create_fix_commit(pr_number: Option<u64>, findings: &[Finding]) -> FixCommitState {
    let desc = build_fix_commit_description(pr_number, findings);
    let (ok, output) = run_cmd_direct("jj", &["new", "-m", &desc], &[], JJ_CMD_TIMEOUT_SECS);
    if !ok {
        log_info(&format!(
            "[action] fix commit 分離 skip: jj new 失敗: {}",
            output
        ));
        return FixCommitState::None;
    }
    match capture_commit_id() {
        Some(cid) => {
            log_info(&format!("[state] fix commit pre-created: {}", cid));
            FixCommitState::Created { commit_id: cid }
        }
        None => {
            log_info("[state] fix commit 作成後の commit id capture 失敗 (fallback)");
            FixCommitState::None
        }
    }
}

/// 空 fix commit を安全に abandon する。
///
/// `commit_id` が `Some(expected)` のとき: 現在の `@` が `expected` と一致する場合のみ
/// abandon を実行する。不一致または capture 失敗時は `[warn]` を出してスキップする。
/// `commit_id` が `None` のとき: 従来通り diff チェックのみで判定する。
///
/// diff あり判定失敗時は abandon をスキップ (fail-safe: 誤 abandon 防止)。
pub(crate) fn try_abandon_empty_fix_commit(context: &str, commit_id: Option<&str>) {
    if let Some(expected) = commit_id {
        match capture_commit_id().as_deref() {
            Some(current) if current == expected => {}
            Some(current) => {
                log_info(&format!(
                    "[warn] {} expected={}, current={} abandon を見送り",
                    context, expected, current
                ));
                return;
            }
            None => {
                log_info(&format!(
                    "[warn] {} expected={}, current=<capture失敗> abandon を見送り",
                    context, expected
                ));
                return;
            }
        }
    }

    if diff_at_is_empty() {
        let label = commit_id.map_or_else(String::new, |id| format!(" ({})", id));
        log_info(&format!(
            "[action] {} 空 fix commit を abandon{}",
            context, label
        ));
        let (ok, out) = run_cmd_direct("jj", &["abandon"], &[], JJ_CMD_TIMEOUT_SECS);
        if !ok {
            log_info(&format!(
                "[action] jj abandon 失敗 (手動片付け推奨): {}",
                out
            ));
        }
    } else {
        log_info(&format!(
            "[warn] {} fix commit に diff あり、abandon を見送り",
            context
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn finding(severity: &str, file: &str, line: &str, issue: &str) -> Finding {
        Finding {
            severity: severity.to_string(),
            file: file.to_string(),
            line: line.to_string(),
            issue: issue.to_string(),
            suggestion: String::new(),
            source: "CodeRabbit".to_string(),
        }
    }

    #[test]
    fn description_without_findings_is_header_only() {
        let desc = build_fix_commit_description(Some(42), &[]);
        assert_eq!(desc, "fix(review): apply CodeRabbit fixes for #42");
    }

    #[test]
    fn description_without_pr_number_falls_back_to_generic_header() {
        let desc = build_fix_commit_description(None, &[]);
        assert_eq!(desc, "fix(review): apply CodeRabbit fixes");
    }

    #[test]
    fn description_with_findings_includes_summary_block() {
        let fs = vec![
            finding("Major", "src/foo.rs", "12", "null pointer"),
            finding("Minor", "src/bar.rs", "34", "unused variable"),
        ];
        let desc = build_fix_commit_description(Some(42), &fs);
        assert!(
            desc.starts_with("fix(review): apply CodeRabbit fixes for #42\n\nResolved findings:\n")
        );
        assert!(desc.contains("- [Major] src/foo.rs:12 null pointer"));
        assert!(desc.contains("- [Minor] src/bar.rs:34 unused variable"));
        assert!(!desc.ends_with('\n'));
    }

    #[test]
    fn description_with_findings_without_pr_number() {
        let fs = vec![finding("Major", "a.rs", "1", "issue")];
        let desc = build_fix_commit_description(None, &fs);
        assert!(desc.starts_with("fix(review): apply CodeRabbit fixes\n\n"));
        assert!(desc.contains("- [Major] a.rs:1 issue"));
    }

    #[test]
    fn fix_commit_state_is_created_truth_table() {
        assert!(!FixCommitState::None.is_created());
        assert!(FixCommitState::Created {
            commit_id: "abc".into()
        }
        .is_created());
    }
}
