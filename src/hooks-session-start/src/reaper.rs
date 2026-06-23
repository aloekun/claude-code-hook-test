//! Orphan run reaper (Bundle c-1 順位 64、ADR-030 §L2 out-of-process)。
//!
//! `.takt/runs/<slug>/meta.json` を scan し、`status: "running"` のまま
//! `ORPHAN_THRESHOLD_SECS` を超えた post-merge-feedback run を「abrupt
//! termination で死んだ」とみなして `.failed` marker を生成 + meta.json
//! `status` を `failed` に更新する。kill -9 / SIGKILL / power loss /
//! OOM Killer など in-process Drop guard (§L1) で救済できない致命系で
//! `.failed` marker が書かれなかった orphan run を L2 で拾う。

use serde::Deserialize;
use std::path::{Path, PathBuf};

use crate::past_time::PastTime;

/// post-merge-feedback task label prefix (ADR-030 §task labeling convention)。
///
/// 値は `cli-merge-pipeline::feedback::TAKT_TASK_PREFIX` と同一でなければならない。
/// crate 間直接依存を避けるため inline duplicate しているが、両 crate の unit test
/// で literal `"post-merge-feedback for #"` を pin する drift 検出を行う
/// (`task_prefix_matches_canonical_literal` 系 test)。
pub(crate) const TAKT_TASK_PREFIX_PMF: &str = "post-merge-feedback for #";

/// orphan reaper の閾値秒数 (ADR-030 §L2 out-of-process)。
///
/// `cli-merge-pipeline::feedback::TAKT_TIMEOUT_SECS` (1200s) + 余裕 5 分。正常 run は
/// 1200s 以内に completed / failed のいずれかに遷移するため、本値を超えても
/// `status: "running"` のまま放置される run は abrupt termination で in-process Drop
/// guard を経由せず死んだとみなす。両 crate の test で `1500` を pin する。
pub(crate) const ORPHAN_THRESHOLD_SECS: u64 = 1500;

/// `.claude/feedback-reports/` の相対パス (repo root から)。
pub(crate) const FEEDBACK_DIR_REPO_RELATIVE: &str = ".claude/feedback-reports";

/// `.takt/runs/` の相対パス (repo root から)。
pub(crate) const TAKT_RUNS_DIR: &str = ".takt/runs";

/// takt meta.json の必要 field のみ部分デシリアライズ。
#[derive(Deserialize)]
pub(crate) struct TaktMeta {
    pub(crate) task: Option<String>,
    pub(crate) status: Option<String>,
    #[serde(rename = "startTime")]
    pub(crate) start_time: Option<String>,
}

/// 検出された orphan run の情報。`reap_orphans` が `.failed` marker を書く際に使う。
pub(crate) struct OrphanRun {
    pub(crate) meta_path: PathBuf,
    pub(crate) pr_number: u64,
    pub(crate) age_secs: u64,
}

/// `2026-05-13T12:33:23.908Z` 形式の ISO 8601 文字列を Unix 秒に変換する。
///
/// 失敗 (invalid date / non-ASCII / 月日範囲外) 時は `None`。fractional 秒は
/// truncate (整数秒精度で十分)。実装は `check-ci-coderabbit::parse_iso8601_to_unix`
/// と同型 (no chrono dep policy)。
pub(crate) fn parse_iso8601_to_unix(s: &str) -> Option<i64> {
    let no_frac = s.split('.').next()?.trim_end_matches('Z');
    let mut parts = no_frac.split('T');
    let date = parts.next()?;
    let time = parts.next()?;
    let mut date_parts = date.split('-');
    let year: i64 = date_parts.next()?.parse().ok()?;
    let month: i64 = date_parts.next()?.parse().ok()?;
    let day: i64 = date_parts.next()?.parse().ok()?;
    let mut time_parts = time.split(':');
    let hour: i64 = time_parts.next()?.parse().ok()?;
    let minute: i64 = time_parts.next()?.parse().ok()?;
    let second: i64 = time_parts.next()?.parse().ok()?;
    if !(1970..=9999).contains(&year)
        || !(1..=12).contains(&month)
        || !(1..=days_in_month(year, month)).contains(&day)
        || !(0..=23).contains(&hour)
        || !(0..=59).contains(&minute)
        || !(0..=59).contains(&second)
    {
        return None;
    }
    let mut days: i64 = 0;
    for y in 1970..year {
        days += if is_leap_year(y) { 366 } else { 365 };
    }
    let month_days = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    for m in 1..month {
        let idx = (m - 1) as usize;
        days += month_days[idx];
        if m == 2 && is_leap_year(year) {
            days += 1;
        }
    }
    days += day - 1;
    Some(days * 86400 + hour * 3600 + minute * 60 + second)
}

fn is_leap_year(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn days_in_month(year: i64, month: i64) -> i64 {
    let month_days = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let base = month_days[(month - 1) as usize];
    if month == 2 && is_leap_year(year) {
        base + 1
    } else {
        base
    }
}

fn read_takt_meta(path: &Path) -> Option<TaktMeta> {
    let content = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&content).ok()
}

/// task label `"post-merge-feedback for #N"` から PR 番号 N を抽出する。
fn extract_pr_number_from_task(task: &str) -> Option<u64> {
    task.strip_prefix(TAKT_TASK_PREFIX_PMF)?.trim().parse().ok()
}

/// meta.json から orphan 判定に必要な要素 (pr_number, start_unix) を抽出する。
///
/// status / task / startTime のいずれかが orphan 条件を満たさなければ `None`。
fn meta_to_orphan_inputs(meta: &TaktMeta) -> Option<(u64, i64)> {
    if meta.status.as_deref() != Some("running") {
        return None;
    }
    let pr = extract_pr_number_from_task(meta.task.as_deref()?)?;
    let start = parse_iso8601_to_unix(meta.start_time.as_deref()?)?;
    Some((pr, start))
}

/// `.takt/runs/<slug>/meta.json` を scan して orphan な post-merge-feedback run を返す。
///
/// 条件: `status: "running"` AND task が `TAKT_TASK_PREFIX_PMF` で始まる AND
/// `now_unix - startTime >= ORPHAN_THRESHOLD_SECS`。malformed meta.json / non-PMF task /
/// PR 番号 parse 失敗 / startTime parse 失敗は defensive に skip。
pub(crate) fn find_orphan_post_merge_feedback_runs(
    runs_dir: &Path,
    now_unix: i64,
) -> Vec<OrphanRun> {
    let entries = match std::fs::read_dir(runs_dir) {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };
    let mut orphans = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let meta_path = path.join("meta.json");
        let Some(meta) = read_takt_meta(&meta_path) else {
            continue;
        };
        let Some((pr_number, start_unix)) = meta_to_orphan_inputs(&meta) else {
            continue;
        };
        let Some(past) = PastTime::from_parts(start_unix, now_unix) else {
            continue;
        };
        let age = past.age_secs();
        if age < ORPHAN_THRESHOLD_SECS as i64 {
            continue;
        }
        orphans.push(OrphanRun {
            meta_path,
            pr_number,
            age_secs: age as u64,
        });
    }
    orphans
}

/// orphan の meta.json を `status: "failed"` に書き換える。reaper の責任明示のため
/// `reaped_by: "hooks-session-start"` も追加する。malformed JSON は skip (Err 返す)。
fn mark_meta_failed(meta_path: &Path) -> std::io::Result<()> {
    let content = std::fs::read_to_string(meta_path)?;
    let mut value: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    if let Some(obj) = value.as_object_mut() {
        obj.insert(
            "status".to_string(),
            serde_json::Value::String("failed".to_string()),
        );
        obj.insert(
            "reaped_by".to_string(),
            serde_json::Value::String("hooks-session-start".to_string()),
        );
    }
    let serialized = serde_json::to_string_pretty(&value).map_err(std::io::Error::other)?;
    std::fs::write(meta_path, serialized)
}

/// `.failed` marker の本文を組み立てる。L2 recovery が拾う際の根拠 + 復旧手順を含む。
fn build_reaper_failed_marker_body(orphan: &OrphanRun) -> String {
    format!(
        "# post-merge-feedback failed (PR #{pr})\n\n\
         takt workflow が abrupt 終了 (kill -9 / SIGKILL / power loss / OOM 等) で中断され、\n\
         in-process Drop guard 経路を経由せずに死んだとみなされました\n\
         (orphan reaper, ADR-030 §L2 out-of-process)。\n\n\
         ## 検出情報\n\n\
         - meta.json: `{meta}`\n\
         - 経過時間: {age} 秒 (閾値: {threshold} 秒 = TAKT_TIMEOUT_SECS + 余裕 5 分)\n\n\
         ## 復旧手順\n\n\
         1. このマーカーを残したまま、Claude Code セッションで何か入力する\n\
         2. UserPromptSubmit hook (`hooks-user-prompt-feedback-recovery`) が検出し、\n   \
         Claude に再実行を促す\n\
         3. 手動で再実行する場合: `pnpm exec takt -w post-merge-feedback -t \"post-merge-feedback for #{pr}\"`\n",
        pr = orphan.pr_number,
        meta = orphan.meta_path.display(),
        age = orphan.age_secs,
        threshold = ORPHAN_THRESHOLD_SECS,
    )
}

/// 検出された orphan run に対し `.failed` marker と meta.json `status=failed` を書く。
///
/// 冪等性:
/// - 既存 `.failed` marker がある → skip (L1 / 前回 reaper pass による処理済み)
/// - 既存 `<pr>.md` 成功レポートがある → skip (ADR-030 §Reconciliation で documented されている
///   「takt parent kill 後に descendants が report 完成」path。meta.json は `status: "running"`
///   のままだが実際は成功しているため、ここで `.failed` marker を書くと false-positive nag になる)
///
/// marker 書込失敗時は当該 orphan を skip して次に進む (best-effort)。
/// 戻り値: 新規 reap した (PR 番号, age_secs) リスト。
pub(crate) fn reap_orphans(repo_root: &Path, orphans: &[OrphanRun]) -> Vec<(u64, u64)> {
    let mut reaped = Vec::new();
    for orphan in orphans {
        let feedback_dir = repo_root.join(FEEDBACK_DIR_REPO_RELATIVE);
        let marker = feedback_dir.join(format!("{}.md.failed", orphan.pr_number));
        let success_report = feedback_dir.join(format!("{}.md", orphan.pr_number));
        if marker.exists() || success_report.exists() {
            continue;
        }
        if let Some(parent) = marker.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let body = build_reaper_failed_marker_body(orphan);
        if std::fs::write(&marker, body).is_err() {
            continue;
        }
        let _ = mark_meta_failed(&orphan.meta_path);
        reaped.push((orphan.pr_number, orphan.age_secs));
    }
    reaped
}

/// SessionStart 時の reaper エントリポイント。orphan を検出 + reap し、
/// nudge メッセージを返す。何も検出しなければ `None`。
pub(crate) fn compute_reaper_nudge(repo_root: &Path, now_unix: i64) -> Option<String> {
    let runs_dir = repo_root.join(TAKT_RUNS_DIR);
    let orphans = find_orphan_post_merge_feedback_runs(&runs_dir, now_unix);
    let reaped = reap_orphans(repo_root, &orphans);
    if reaped.is_empty() {
        return None;
    }
    let mut lines = Vec::with_capacity(reaped.len() + 2);
    lines.push("[POST_MERGE_FEEDBACK_REAPER]".to_string());
    lines.push(format!(
        "orphan post-merge-feedback runs を {} 件検出、`.failed` marker を生成しました \
         (abrupt termination 経路の L2 recovery、ADR-030 §L2)",
        reaped.len()
    ));
    for (pr, age) in &reaped {
        lines.push(format!("  - PR #{} (経過 {} 秒)", pr, age));
    }
    Some(lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unique_temp_root(prefix: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!(
            "reaper-{}-{}-{}",
            prefix,
            std::process::id(),
            nanos
        ))
    }

    fn write_meta(run_dir: &Path, task: &str, status: &str, start_time: &str) {
        std::fs::create_dir_all(run_dir).unwrap();
        let json = serde_json::json!({
            "task": task,
            "status": status,
            "startTime": start_time,
        });
        std::fs::write(
            run_dir.join("meta.json"),
            serde_json::to_string_pretty(&json).unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn task_prefix_matches_canonical_literal() {
        assert_eq!(
            TAKT_TASK_PREFIX_PMF, "post-merge-feedback for #",
            "TAKT_TASK_PREFIX_PMF must match cli-merge-pipeline::feedback::TAKT_TASK_PREFIX. \
             If you changed this constant, update the corresponding test in feedback.rs as well."
        );
    }

    #[test]
    fn orphan_threshold_matches_canonical_value() {
        assert_eq!(
            ORPHAN_THRESHOLD_SECS, 1500,
            "ORPHAN_THRESHOLD_SECS must match cli-merge-pipeline::feedback::ORPHAN_THRESHOLD_SECS \
             (= TAKT_TIMEOUT_SECS + 300). If TAKT_TIMEOUT_SECS changes, both crates must update."
        );
    }

    #[test]
    fn parse_iso8601_basic_epoch() {
        assert_eq!(parse_iso8601_to_unix("1970-01-01T00:00:00Z"), Some(0));
    }

    #[test]
    fn parse_iso8601_handles_fractional_seconds() {
        let t = parse_iso8601_to_unix("2026-05-13T12:33:23.908Z").unwrap();
        let t_no_frac = parse_iso8601_to_unix("2026-05-13T12:33:23Z").unwrap();
        assert_eq!(t, t_no_frac, "fractional seconds must be truncated, not rejected");
    }

    #[test]
    fn parse_iso8601_rejects_invalid_month() {
        assert!(parse_iso8601_to_unix("2026-13-01T00:00:00Z").is_none());
    }

    #[test]
    fn extract_pr_number_from_post_merge_feedback_task() {
        assert_eq!(
            extract_pr_number_from_task("post-merge-feedback for #109"),
            Some(109)
        );
        assert_eq!(
            extract_pr_number_from_task("post-merge-feedback for #42"),
            Some(42)
        );
    }

    #[test]
    fn extract_pr_number_rejects_non_pmf_task() {
        assert_eq!(extract_pr_number_from_task("pre-push-review"), None);
        assert_eq!(extract_pr_number_from_task("post-pr-review"), None);
        assert_eq!(extract_pr_number_from_task("post-merge-feedback"), None);
        assert_eq!(
            extract_pr_number_from_task("post-merge-feedback for #abc"),
            None
        );
    }

    #[test]
    fn find_orphans_returns_empty_when_runs_dir_missing() {
        let root = unique_temp_root("missing-runs");
        assert!(find_orphan_post_merge_feedback_runs(&root.join(".takt/runs"), 9_999_999_999).is_empty());
    }

    #[test]
    fn find_orphans_detects_running_post_merge_feedback_past_threshold() {
        let root = unique_temp_root("detect");
        let runs = root.join(".takt/runs");
        let run = runs.join("20260513-100000-post-merge-feedback-for-109");
        let start_iso = "2026-05-13T03:26:40Z";
        let start_unix = parse_iso8601_to_unix(start_iso).unwrap();
        write_meta(&run, "post-merge-feedback for #109", "running", start_iso);
        let now = start_unix + ORPHAN_THRESHOLD_SECS as i64 + 1;
        let orphans = find_orphan_post_merge_feedback_runs(&runs, now);
        assert_eq!(orphans.len(), 1);
        assert_eq!(orphans[0].pr_number, 109);
        assert!(orphans[0].age_secs >= ORPHAN_THRESHOLD_SECS);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn find_orphans_skips_runs_within_threshold() {
        let root = unique_temp_root("within-threshold");
        let runs = root.join(".takt/runs");
        let run = runs.join("20260513-100000-post-merge-feedback-for-150");
        let start_iso = "2026-05-13T03:26:40Z";
        let start_unix = parse_iso8601_to_unix(start_iso).unwrap();
        write_meta(&run, "post-merge-feedback for #150", "running", start_iso);
        let now = start_unix + (ORPHAN_THRESHOLD_SECS as i64 - 1);
        let orphans = find_orphan_post_merge_feedback_runs(&runs, now);
        assert!(
            orphans.is_empty(),
            "in-flight run within timeout window must not be reaped"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn find_orphans_skips_completed_runs() {
        let root = unique_temp_root("completed");
        let runs = root.join(".takt/runs");
        let run = runs.join("20260513-100000-post-merge-feedback-for-151");
        write_meta(&run, "post-merge-feedback for #151", "completed", "2026-05-13T03:26:40Z");
        let orphans = find_orphan_post_merge_feedback_runs(&runs, 9_999_999_999);
        assert!(orphans.is_empty(), "completed runs must not be reaped");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn find_orphans_skips_non_post_merge_feedback_workflows() {
        let root = unique_temp_root("non-pmf");
        let runs = root.join(".takt/runs");
        let pre_push = runs.join("20260513-100000-pre-push-review");
        write_meta(&pre_push, "pre-push-review", "running", "2026-05-13T03:26:40Z");
        let post_pr = runs.join("20260513-100001-post-pr-review");
        write_meta(&post_pr, "post-pr-review", "running", "2026-05-13T03:26:40Z");
        let orphans = find_orphan_post_merge_feedback_runs(&runs, 9_999_999_999);
        assert!(
            orphans.is_empty(),
            "non-post-merge-feedback workflows have different recovery semantics"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn find_orphans_skips_malformed_meta_json() {
        let root = unique_temp_root("malformed");
        let runs = root.join(".takt/runs");
        let run = runs.join("20260513-100000-post-merge-feedback-for-160");
        std::fs::create_dir_all(&run).unwrap();
        std::fs::write(run.join("meta.json"), "not-valid-json{").unwrap();
        let orphans = find_orphan_post_merge_feedback_runs(&runs, 9_999_999_999);
        assert!(
            orphans.is_empty(),
            "malformed meta.json must be skipped defensively"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn find_orphans_skips_future_start_time_without_silent_age_zero() {
        let root = unique_temp_root("future-start");
        let runs = root.join(".takt/runs");
        let run = runs.join("20260513-100000-post-merge-feedback-for-161");
        let start_iso = "2027-01-01T00:00:00Z";
        let start_unix = parse_iso8601_to_unix(start_iso).unwrap();
        write_meta(&run, "post-merge-feedback for #161", "running", start_iso);
        let now = start_unix - 3600;
        let orphans = find_orphan_post_merge_feedback_runs(&runs, now);
        assert!(
            orphans.is_empty(),
            "future startTime must be rejected by PastTime, not silently age=0 (順位 197 / Bundle W)"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn reap_orphans_writes_marker_and_updates_meta() {
        let root = unique_temp_root("reap");
        let runs = root.join(".takt/runs");
        let run = runs.join("20260513-100000-post-merge-feedback-for-200");
        let start_iso = "2026-05-13T03:26:40Z";
        let start_unix = parse_iso8601_to_unix(start_iso).unwrap();
        write_meta(&run, "post-merge-feedback for #200", "running", start_iso);
        let now = start_unix + ORPHAN_THRESHOLD_SECS as i64 + 60;
        let orphans = find_orphan_post_merge_feedback_runs(&runs, now);
        assert_eq!(orphans.len(), 1);

        let reaped = reap_orphans(&root, &orphans);
        assert_eq!(reaped.len(), 1);
        assert_eq!(reaped[0].0, 200);

        let marker = root.join(FEEDBACK_DIR_REPO_RELATIVE).join("200.md.failed");
        assert!(marker.exists());
        let body = std::fs::read_to_string(&marker).unwrap();
        assert!(body.contains("PR #200"));
        assert!(body.contains("abrupt"));
        assert!(body.contains("orphan reaper"));

        let updated_meta: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(run.join("meta.json")).unwrap()).unwrap();
        assert_eq!(updated_meta.get("status").and_then(|v| v.as_str()), Some("failed"));
        assert_eq!(
            updated_meta.get("reaped_by").and_then(|v| v.as_str()),
            Some("hooks-session-start")
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn reap_orphans_skips_when_success_report_exists_despite_stale_meta() {
        let root = unique_temp_root("reconciled-success");
        let runs = root.join(".takt/runs");
        let run = runs.join("20260513-100000-post-merge-feedback-for-202");
        let start_iso = "2026-05-13T03:26:40Z";
        let start_unix = parse_iso8601_to_unix(start_iso).unwrap();
        write_meta(&run, "post-merge-feedback for #202", "running", start_iso);
        let now = start_unix + ORPHAN_THRESHOLD_SECS as i64 + 60;

        let feedback_dir = root.join(FEEDBACK_DIR_REPO_RELATIVE);
        std::fs::create_dir_all(&feedback_dir).unwrap();
        let success_report = feedback_dir.join("202.md");
        std::fs::write(
            &success_report,
            "# post-merge-feedback for PR #202\n\n(takt parent killed at timeout, descendants finished after)",
        )
        .unwrap();

        let orphans = find_orphan_post_merge_feedback_runs(&runs, now);
        let reaped = reap_orphans(&root, &orphans);
        assert!(
            reaped.is_empty(),
            "ADR-030 §Reconciliation path: success report exists despite stale meta.json — must not write .failed marker"
        );
        assert!(
            !feedback_dir.join("202.md.failed").exists(),
            "no .failed marker may be written when <pr>.md success report is present"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn reap_orphans_is_idempotent_when_marker_exists() {
        let root = unique_temp_root("idempotent");
        let runs = root.join(".takt/runs");
        let run = runs.join("20260513-100000-post-merge-feedback-for-201");
        let start_iso = "2026-05-13T03:26:40Z";
        let start_unix = parse_iso8601_to_unix(start_iso).unwrap();
        write_meta(&run, "post-merge-feedback for #201", "running", start_iso);
        let now = start_unix + ORPHAN_THRESHOLD_SECS as i64 + 60;

        let marker_dir = root.join(FEEDBACK_DIR_REPO_RELATIVE);
        std::fs::create_dir_all(&marker_dir).unwrap();
        let marker = marker_dir.join("201.md.failed");
        std::fs::write(&marker, "pre-existing detailed marker from L1").unwrap();

        let orphans = find_orphan_post_merge_feedback_runs(&runs, now);
        let reaped = reap_orphans(&root, &orphans);
        assert!(reaped.is_empty(), "must not re-reap when marker already exists");

        let body = std::fs::read_to_string(&marker).unwrap();
        assert_eq!(body, "pre-existing detailed marker from L1");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn compute_reaper_nudge_returns_none_when_no_orphans() {
        let root = unique_temp_root("nudge-none");
        std::fs::create_dir_all(root.join(".takt/runs")).unwrap();
        assert!(compute_reaper_nudge(&root, 9_999_999_999).is_none());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn compute_reaper_nudge_emits_message_when_reaped() {
        let root = unique_temp_root("nudge-some");
        let runs = root.join(".takt/runs");
        let run = runs.join("20260513-100000-post-merge-feedback-for-300");
        let start_iso = "2026-05-13T03:26:40Z";
        let start_unix = parse_iso8601_to_unix(start_iso).unwrap();
        write_meta(&run, "post-merge-feedback for #300", "running", start_iso);
        let now = start_unix + ORPHAN_THRESHOLD_SECS as i64 + 100;
        let nudge = compute_reaper_nudge(&root, now).expect("nudge must be emitted");
        assert!(nudge.contains("[POST_MERGE_FEEDBACK_REAPER]"));
        assert!(nudge.contains("1 件"));
        assert!(nudge.contains("PR #300"));
        let _ = std::fs::remove_dir_all(&root);
    }
}
