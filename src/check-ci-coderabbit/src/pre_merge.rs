//! CodeRabbit の要約コメントにある Pre-merge checks の表を読み、失敗を判定に含める。
//!
//! **なぜ読むか**: 失敗した pre-merge check (例: Docstring Coverage が閾値未満) は、インライン
//! 指摘にもスレッドにもならない。CodeRabbit は失敗がある間「No actionable comments were
//! generated」を出さず、レビュー本文にもその check しか載らない。表を読まないと、指摘 0 件・
//! 未解決 0 件に見えて「マージ可能」と誤認する (#623 で実測。Docstring Coverage 76.92% の
//! Warning が残ったまま、check-ci も threads も pr-monitor も何も報告しなかった)。
//!
//! **判定への入れ方**: 失敗は success だけを止める ([`apply_pre_merge_gate`])。CI 失敗や
//! 監視の継続など、既存の判定 (`crate::decide::decide`) の他の結果は変えない。
//! - 今の head で測った失敗 → `action_required` (人間が直す。自動修正の `findings` には入れない)
//! - 古い commit で測った失敗 → `continue_monitoring`。直した後に再 push しても、CodeRabbit は
//!   自動ではレビューし直さない (incremental review 無効) ので、表は古いまま残る。success に
//!   すると直ったことを確かめずに終わり、action_required にすると直した後も止まり続ける。
//!   summary で「Trigger review で再測定が必要」と案内する
//!
//! **fail-closed** ([ADR-043](../../../docs/adr/adr-043-security-gates-fail-closed.md)):
//! 節はあるのに行を読めないときは、読めなかったこと自体を失敗として出す。測った commit や
//! head を読めないときは古いと見なさない (= 今の失敗として止める)。節が無いときだけ None を
//! 返す (Pre-merge checks を出さない設定や、要約コメントがまだ無い PR)。

use crate::markers::{
    rate_limit_event_time, CHANGE_ASSESSMENT_COMMIT_MARKER, PRE_MERGE_CHECKS_END_MARKER,
    PRE_MERGE_CHECKS_START_MARKER, PRE_MERGE_PASSED_STATUS, PRE_MERGE_STATUS_IMAGE_MARKER,
    REVIEW_COVERED_COMMIT_MARKER, WALKTHROUGH_HEADER_MARKER,
};
use crate::models::{GhComment, PreMergeCheckFailure, PreMergeChecks};
use crate::parsers::is_coderabbit_comment;

/// 表の行を読めなかったときに出す失敗の名前。
const UNREADABLE_TABLE_NAME: &str = "(Pre-merge checks の表を読めない)";

/// issue comments の JSON から、最新の要約コメントの Pre-merge checks を読む。
///
/// `push_time` では絞らない。要約コメントはレビューのたびに同じコメントが書き直されるので、
/// 新しさは時刻でなく、測った commit と `head_sha` の照合で決める。
pub(crate) fn parse_pre_merge_checks(comments_json: &str, head_sha: Option<&str>) -> Option<PreMergeChecks> {
    let comments: Vec<GhComment> = serde_json::from_str(comments_json).unwrap_or_else(|e| {
        eprintln!("[check-ci-coderabbit] comments JSON パースエラー (pre-merge checks): {}", e);
        vec![]
    });
    let summary = comments
        .iter()
        .filter(|c| is_coderabbit_comment(c))
        .filter(|c| {
            let body = c.body.as_deref().unwrap_or("");
            body.contains(WALKTHROUGH_HEADER_MARKER) && body.contains(PRE_MERGE_CHECKS_START_MARKER)
        })
        .max_by(|a, b| rate_limit_event_time(a).cmp(&rate_limit_event_time(b)))?;
    let body = summary.body.as_deref().unwrap_or("");
    let measured_commit = measured_commit(body);
    let stale = match (measured_commit.as_deref(), head_sha) {
        (Some(measured), Some(head)) => !measured.eq_ignore_ascii_case(head),
        _ => false,
    };
    Some(PreMergeChecks {
        failed: failed_checks(pre_merge_section(body)),
        measured_commit,
        stale,
        summary_url: summary.html_url.clone(),
    })
}

/// 節の中身 (開始マーカーの後から終了マーカーの前まで)。終了マーカーが無ければ末尾まで。
fn pre_merge_section(body: &str) -> &str {
    let Some(start) = body.find(PRE_MERGE_CHECKS_START_MARKER) else {
        return "";
    };
    let rest = &body[start + PRE_MERGE_CHECKS_START_MARKER.len()..];
    let end = rest.find(PRE_MERGE_CHECKS_END_MARKER).unwrap_or(rest.len());
    &rest[..end]
}

/// 表の行のうち、状態が `Passed` でないものを返す。check の行が 1 行も無ければ、読めなかった
/// ことを 1 件の失敗として返す。
fn failed_checks(section: &str) -> Vec<PreMergeCheckFailure> {
    let rows: Vec<PreMergeCheckFailure> = section.lines().filter_map(parse_row).collect();
    if rows.is_empty() {
        return vec![PreMergeCheckFailure {
            name: UNREADABLE_TABLE_NAME.to_string(),
            status: "Unknown".to_string(),
            explanation: "Pre-merge checks の節はあるが、check の行を読めなかった。CodeRabbit の書式が変わった可能性がある".to_string(),
        }];
    }
    rows.into_iter()
        .filter(|r| r.status != PRE_MERGE_PASSED_STATUS)
        .collect()
}

/// `| Docstring Coverage | ![Warning](…pre-merge-checks-warning.svg) | 説明 | 対処 |` を読む。
/// 状態のセルに [`PRE_MERGE_STATUS_IMAGE_MARKER`] の画像が無い行 (見出し・区切り) は None。
fn parse_row(line: &str) -> Option<PreMergeCheckFailure> {
    let line = line.trim();
    if !line.starts_with('|') {
        return None;
    }
    let cells: Vec<&str> = line.split('|').collect();
    let status_cell = cells.get(2)?.trim();
    if !status_cell.contains(PRE_MERGE_STATUS_IMAGE_MARKER) {
        return None;
    }
    let status = status_cell.strip_prefix("![")?.split(']').next()?.trim();
    Some(PreMergeCheckFailure {
        name: cells.get(1)?.trim().to_string(),
        status: status.to_string(),
        explanation: cells.get(3).map(|c| c.trim().to_string()).unwrap_or_default(),
    })
}

/// 要約コメントが見た commit の SHA。[`REVIEW_COVERED_COMMIT_MARKER`] を優先する。
fn measured_commit(body: &str) -> Option<String> {
    sha_after(body, REVIEW_COVERED_COMMIT_MARKER).or_else(|| sha_after(body, CHANGE_ASSESSMENT_COMMIT_MARKER))
}

fn sha_after(body: &str, marker: &str) -> Option<String> {
    let start = body.find(marker)? + marker.len();
    let sha: String = body[start..].chars().take_while(|c| c.is_ascii_hexdigit()).collect();
    (sha.len() == 40).then_some(sha)
}

/// pre-merge check の失敗で success を止める。success 以外の判定はそのまま返す。
///
/// 今の head で測った失敗は `action_required`、古い測定の失敗は `continue_monitoring`
/// (module doc)。
pub(crate) fn apply_pre_merge_gate(decided: (String, String), checks: Option<&PreMergeChecks>) -> (String, String) {
    let Some(checks) = checks.filter(|c| !c.failed.is_empty()) else {
        return decided;
    };
    if decided.1 != "stop_monitoring_success" {
        return decided;
    }
    if checks.stale {
        ("pending".to_string(), "continue_monitoring".to_string())
    } else {
        ("action_required".to_string(), "action_required".to_string())
    }
}

/// summary に足す文言。失敗が無ければ None。
pub(crate) fn summary_part(checks: Option<&PreMergeChecks>) -> Option<String> {
    let checks = checks.filter(|c| !c.failed.is_empty())?;
    let names: Vec<String> = checks
        .failed
        .iter()
        .map(|f| format!("{} ({})", f.name, f.status))
        .collect();
    let base = format!("pre-merge check 失敗: {}", names.join("、"));
    if !checks.stale {
        return Some(base);
    }
    let measured: String = checks.measured_commit.as_deref().unwrap_or("").chars().take(8).collect();
    Some(format!(
        "{} ({} で測った結果。今の head は未測定のため、Trigger review で再測定が必要)",
        base, measured
    ))
}

#[cfg(test)]
#[path = "pre_merge/tests.rs"]
mod tests;
