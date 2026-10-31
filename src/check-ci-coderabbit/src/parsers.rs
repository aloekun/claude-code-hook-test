//! CI / CodeRabbit state + comment JSON parsers (順位 209 / 順位 208 PR A refactor)。

use crate::markers::{
    has_checked_trigger, has_review_command, is_clean_walkthrough_comment, is_rate_limit_comment,
    is_skip_review_comment, rate_limit_event_time, STATUS_SKIPPED_PREFIX,
};
use crate::models::{
    CiRunSummary, CiStatus, GhComment, GhReview, GhRollupItem, GhRunItem, GhStatusItem,
};

/// `gh pr view --json statusCheckRollup` の JSON をパースして CI 状態を返す。
///
/// rollup は **PR head SHA に紐づく check だけ**を返すため、`gh run list --branch` と違い
/// 同一ブランチの古い SHA の run が混入しない。ブランチ名の解決も不要になる
/// (jj colocated リポジトリの git HEAD は detached で、ブランチ名を取得できない)。
///
/// **空 rollup は `pending` 扱いにする** (`success` にしない)。check がまだ作成されて
/// いない状態と「全部通った」状態は区別する必要があり、後者を陽性証拠なしに名乗ると
/// [ADR-064](../../../docs/adr/adr-064-monitor-success-positive-evidence.md) が排除した
/// silent success を再導入することになる。
pub(crate) fn parse_ci_rollup(json: &str) -> CiStatus {
    let items: Vec<GhRollupItem> = serde_json::from_str(json).unwrap_or_else(|e| {
        eprintln!("[check-ci-coderabbit] statusCheckRollup JSON パースエラー: {}", e);
        vec![]
    });

    let normalized: Vec<GhRunItem> = items
        .iter()
        .filter(|item| !is_coderabbit_status_context(item))
        .map(normalize_rollup_item)
        .collect();
    if normalized.is_empty() {
        return CiStatus {
            overall: "pending".to_string(),
            runs: vec![],
        };
    }

    let runs: Vec<CiRunSummary> = normalized
        .iter()
        .map(|item| CiRunSummary {
            name: item.name.clone(),
            conclusion: item
                .conclusion
                .clone()
                .unwrap_or_else(|| "pending".to_string()),
        })
        .collect();

    CiStatus {
        overall: classify_ci_overall(&normalized).to_string(),
        runs,
    }
}

/// rollup 要素が CodeRabbit 自身の commit status (`StatusContext.context` に
/// "CodeRabbit" を含む) かどうかを判定する。
///
/// `ci.overall` は `decide()` の最優先分岐 (`ci.overall=="failure"` → 即
/// `stop_monitoring_failure`) に使われる。CodeRabbit の commit status はレート制限中
/// でも pass を返す等、生の値が信頼できない (PR #307/#309、`decide::has_review_evidence`
/// のコメント参照) ため、独立した `review_state` シグナル (`fetch_coderabbit_commit_state`
/// 経由) としてのみ扱い、CI 判定には混ぜない。混ぜると `decide()` が CR 用に用意した
/// rate-limit gate / evidence backstop を素通りする。
fn is_coderabbit_status_context(item: &GhRollupItem) -> bool {
    item.context
        .as_deref()
        .map(|c| c.contains("CodeRabbit"))
        .unwrap_or(false)
}

/// rollup の union ノードを `gh run list` 相当の `{name, conclusion}` へ正規化する。
///
/// `CheckRun` で `conclusion` が未確定 (実行中) の場合は `status` (`IN_PROGRESS` /
/// `QUEUED`) を conclusion 位置へ落とし、[`is_pending_run`] が pending と判定できる形に
/// する。`StatusContext` は `context` / `state` から拾う。
fn normalize_rollup_item(item: &GhRollupItem) -> GhRunItem {
    let name = item
        .name
        .clone()
        .or_else(|| item.context.clone())
        .unwrap_or_else(|| "(unnamed check)".to_string());
    let conclusion = item
        .conclusion
        .clone()
        .filter(|c| !c.is_empty())
        .or_else(|| item.state.clone().filter(|s| !s.is_empty()))
        .or_else(|| item.status.clone().filter(|s| !s.is_empty()))
        .map(|c| c.to_ascii_lowercase());
    GhRunItem { name, conclusion }
}

fn classify_ci_overall(items: &[GhRunItem]) -> &'static str {
    if items.iter().any(is_pending_run) {
        "pending"
    } else if items.iter().any(is_failure_run) {
        "failure"
    } else {
        "success"
    }
}

/// 未確定 (= まだ結論が出ていない) 状態。`requested` / `expected` は rollup 語彙。
///
/// pending を success 側へ寄せないこと: 「まだ終わっていない」を「通った」と報告すると
/// ADR-064 が排除した陽性証拠なき success に戻る。
fn is_pending_run(i: &GhRunItem) -> bool {
    matches!(
        i.conclusion.as_deref(),
        None | Some("")
            | Some("pending")
            | Some("queued")
            | Some("in_progress")
            | Some("waiting")
            | Some("requested")
            | Some("expected")
    )
}

/// 失敗として扱う結論。`error` は `StatusContext` の、`startup_failure` は `CheckRun` の語彙。
///
/// `skipped` / `neutral` は失敗に含めない (GitHub 自身も PR を赤にしない)。
fn is_failure_run(i: &GhRunItem) -> bool {
    matches!(
        i.conclusion.as_deref(),
        Some("failure")
            | Some("cancelled")
            | Some("timed_out")
            | Some("action_required")
            | Some("stale")
            | Some("error")
            | Some("startup_failure")
    )
}

/// gh api .../statuses の JSON から CodeRabbit のレビュー状態を返す。
///
/// 最新の status が `success` でも、description が [`STATUS_SKIPPED_PREFIX`] で始まれば
/// `not_started` を返す (順位 585)。CR はスキップも `success` で通知するので、state だけを
/// 返すとスキップしか付いていない PR を「レビュー済み」と読む。
pub(crate) fn parse_coderabbit_status(json: &str) -> String {
    let items: Vec<GhStatusItem> = serde_json::from_str(json).unwrap_or_else(|e| {
        eprintln!("[check-ci-coderabbit] statuses JSON パースエラー: {}", e);
        vec![]
    });

    let cr_statuses: Vec<&GhStatusItem> = items
        .iter()
        .filter(|s| {
            s.context
                .as_deref()
                .map(|c| c.contains("CodeRabbit"))
                .unwrap_or(false)
        })
        .collect();

    if cr_statuses.is_empty() {
        return "not_found".to_string();
    }

    let latest = cr_statuses.first();
    let is_skipped = latest
        .and_then(|s| s.description.as_deref())
        .map(|d| d.starts_with(STATUS_SKIPPED_PREFIX))
        .unwrap_or(false);
    match latest.and_then(|s| s.state.clone()) {
        Some(state) if state == "success" && is_skipped => "not_started".to_string(),
        Some(state) => state,
        None => "not_found".to_string(),
    }
}

/// commit status と comment を合わせて、スキップと依頼の状態を確定する。
///
/// - commit status が無いときは、comment 側のスキップ通知で `not_found` を `not_started` へ
///   寄せる。CR は re-push のスキップで commit status を付けず、walkthrough comment に
///   [`crate::markers::SKIP_REVIEW_MARKER`] の段を足すだけのことがある (#600 で実測)
/// - `not_started` でも、push 後に依頼 ([`has_checked_trigger`] / [`has_review_command`]) が
///   あれば `pending` (レビュー待ち) にする。依頼の後も段と status はしばらく残る (#602 で実測)
///
/// スキップの段と Trigger review のチェックは編集時刻 (`updated_at`)、依頼コメントは投稿時刻
/// (`created_at`) が `push_time` 以降のものだけを見る。上記以外の状態は commit status の
/// ほうが新しい事実なので変えない。
pub(crate) fn resolve_review_state(status_state: String, comments_json: &str, push_time: &str) -> String {
    if status_state != "not_found" && status_state != "not_started" {
        return status_state;
    }
    let comments: Vec<GhComment> = serde_json::from_str(comments_json).unwrap_or_else(|e| {
        eprintln!("[check-ci-coderabbit] comments JSON パースエラー (review state): {}", e);
        vec![]
    });
    let after_push = |c: &GhComment| rate_limit_event_time(c).map(|t| t >= push_time).unwrap_or(false);
    let has_skip_notice = comments
        .iter()
        .any(|c| is_coderabbit_comment(c) && is_skip_review_comment(c) && after_push(c));
    let state = if status_state == "not_found" && has_skip_notice {
        "not_started".to_string()
    } else {
        status_state
    };
    let created_after_push = |c: &GhComment| c.created_at.as_deref().is_some_and(|t| t >= push_time);
    let requested = comments.iter().any(|c| {
        (after_push(c) && has_checked_trigger(c)) || (created_after_push(c) && has_review_command(c))
    });
    if state == "not_started" && requested {
        "pending".to_string()
    } else {
        state
    }
}

fn is_coderabbit_comment(c: &GhComment) -> bool {
    c.user
        .as_ref()
        .and_then(|u| u.login.as_deref())
        .map(|l| l == "coderabbitai[bot]")
        .unwrap_or(false)
}

/// PR コメントの JSON から push_time 以降の CodeRabbit 新規コメント数を返す。
///
/// 「review in progress」通知コメントと rate-limit コメントは除外する
/// (含めると `decide()` が action_required を早期 return し rate-limit retry 経路に入らない)。
/// スキップ通知も除外する (順位 585)。含めると、レビューが始まってもいない PR で
/// `decide()` が action_required を返す。
pub(crate) fn parse_new_comments(json: &str, push_time: &str) -> usize {
    let comments: Vec<GhComment> = serde_json::from_str(json).unwrap_or_else(|e| {
        eprintln!("[check-ci-coderabbit] comments JSON パースエラー: {}", e);
        vec![]
    });

    comments
        .iter()
        .filter(|c| is_kept_new_comment(c, push_time))
        .count()
}

fn is_kept_new_comment(c: &GhComment, push_time: &str) -> bool {
    let is_coderabbit = is_coderabbit_comment(c);

    let after_push_time = c
        .created_at
        .as_deref()
        .map(|t| t >= push_time)
        .unwrap_or(false);

    let is_review_in_progress = c
        .body
        .as_deref()
        .map(|b| b.contains("review in progress"))
        .unwrap_or(false);

    let is_rate_limit = is_rate_limit_comment(c);

    is_coderabbit
        && after_push_time
        && !is_review_in_progress
        && !is_rate_limit
        && !is_skip_review_comment(c)
}

/// 順位 208: CR walkthrough comment body から clean marker を検出する。
///
/// formal Review object が無い (= `review_state == "not_found"`) 場合でも本シグナルが
/// true なら `decide()` で `(complete, stop_monitoring_success)` を返し、PR #210/#211 で
/// 発生した recheck loop を構造的に終了させる。
pub(crate) fn parse_walkthrough_clean_marker(json: &str, push_time: &str) -> bool {
    let comments: Vec<GhComment> = serde_json::from_str(json).unwrap_or_else(|e| {
        eprintln!(
            "[check-ci-coderabbit] walkthrough JSON パースエラー: {}",
            e
        );
        vec![]
    });

    comments
        .iter()
        .any(|c| is_clean_walkthrough_comment(c, push_time))
}

/// PR レビューの JSON から最新の CodeRabbit レビューの "Actionable comments posted: N" を抽出。
pub(crate) fn parse_actionable_comments(json: &str, push_time: &str) -> Option<usize> {
    let reviews: Vec<GhReview> = serde_json::from_str(json).unwrap_or_else(|e| {
        eprintln!("[check-ci-coderabbit] reviews JSON パースエラー: {}", e);
        vec![]
    });

    let latest = reviews.iter().rfind(|r| {
        let is_coderabbit = r
            .user
            .as_ref()
            .and_then(|u| u.login.as_deref())
            .map(|l| l == "coderabbitai[bot]")
            .unwrap_or(false);

        let after_push_time = r
            .submitted_at
            .as_deref()
            .map(|t| t >= push_time)
            .unwrap_or(false);

        is_coderabbit && after_push_time
    })?;

    let body = latest.body.as_deref()?;
    extract_actionable_count(body)
}

/// 文字列から "Actionable comments posted: N" の N を抽出。
pub(crate) fn extract_actionable_count(body: &str) -> Option<usize> {
    let marker = "Actionable comments posted: ";
    let pos = body.find(marker)?;
    let rest = &body[pos + marker.len()..];
    let num_str: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    num_str.parse::<usize>().ok()
}

/// GraphQL レスポンスから未解決スレッド数をパースする。
pub(crate) fn parse_unresolved_threads(json: &str) -> Option<usize> {
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    let nodes = value
        .pointer("/data/repository/pullRequest/reviewThreads/nodes")?
        .as_array()?;

    let unresolved = nodes
        .iter()
        .filter(|n| n.get("isResolved").and_then(|v| v.as_bool()) == Some(false))
        .count();

    Some(unresolved)
}

#[cfg(test)]
#[path = "parsers/tests.rs"]
mod tests;
