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

/// この run 自身の成果物 (`<run dir>/reports/feedback-report.md`) を置く。
fn write_run_report(run_dir: &Path) {
    let reports = run_dir.join("reports");
    std::fs::create_dir_all(&reports).unwrap();
    std::fs::write(reports.join("feedback-report.md"), "# feedback report").unwrap();
}

/// 「1 本目が成果物ゼロで死に、2 本目が完走して `<pr>.md` を書いた」状態を組む。
///
/// 戻り値は (repo root, runs dir, 1 本目の run dir, 判定時刻)。
fn repo_with_dead_first_attempt_and_successful_retry(pr: u64) -> (PathBuf, PathBuf, PathBuf, i64) {
    let root = unique_temp_root("settle-dead-first-attempt");
    let runs = root.join(".takt/runs");
    let dead_first_attempt = runs.join(format!("20260809-055921-post-merge-feedback-for-{pr}"));
    let successful_retry = runs.join(format!("20260809-063122-post-merge-feedback-for-{pr}"));
    let task = format!("post-merge-feedback for #{pr}");
    let start_iso = "2026-08-09T05:59:21Z";
    write_meta(&dead_first_attempt, &task, "running", start_iso);
    write_meta(
        &successful_retry,
        &task,
        "completed",
        "2026-08-09T06:31:22Z",
    );
    write_run_report(&successful_retry);

    let feedback_dir = root.join(FEEDBACK_DIR_REPO_RELATIVE);
    std::fs::create_dir_all(&feedback_dir).unwrap();
    std::fs::write(feedback_dir.join(format!("{pr}.md")), "# feedback report").unwrap();

    let now = parse_iso8601_to_unix(start_iso).unwrap() + ORPHAN_THRESHOLD_SECS as i64 + 60;
    (root, runs, dead_first_attempt, now)
}

#[test]
fn task_prefix_matches_canonical_literal() {
    assert_eq!(
        TAKT_TASK_PREFIX_PMF, "post-merge-feedback for #",
        "TAKT_TASK_PREFIX_PMF must match cli-merge-pipeline::feedback::TAKT_TASK_PREFIX. \
         If you changed this constant, update the corresponding test in feedback.rs as well."
    );
}

/// `cli-merge-pipeline::feedback::run_registry::REPORT_FILE_NAME` と同じ literal を pin する。
///
/// 片方の crate だけがファイル名を変えると、reaper は run 自身の成果物を見つけられず
/// **成功した run を `failed` として確定**するが、それを落とすテストが他に無い。
#[test]
fn run_report_file_name_matches_canonical_literal() {
    assert_eq!(
        RUN_REPORT_FILE_NAME, "feedback-report.md",
        "RUN_REPORT_FILE_NAME must match \
         cli-merge-pipeline::feedback::run_registry::REPORT_FILE_NAME. \
         If you changed this constant, update the corresponding test in run_registry as well."
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
    assert_eq!(
        t, t_no_frac,
        "fractional seconds must be truncated, not rejected"
    );
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
    assert!(
        find_orphan_post_merge_feedback_runs(&root.join(".takt/runs"), 9_999_999_999).is_empty()
    );
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
    write_meta(
        &run,
        "post-merge-feedback for #151",
        "completed",
        "2026-05-13T03:26:40Z",
    );
    let orphans = find_orphan_post_merge_feedback_runs(&runs, 9_999_999_999);
    assert!(orphans.is_empty(), "completed runs must not be reaped");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn find_orphans_skips_non_post_merge_feedback_workflows() {
    let root = unique_temp_root("non-pmf");
    let runs = root.join(".takt/runs");
    let pre_push = runs.join("20260513-100000-pre-push-review");
    write_meta(
        &pre_push,
        "pre-push-review",
        "running",
        "2026-05-13T03:26:40Z",
    );
    let post_pr = runs.join("20260513-100001-post-pr-review");
    write_meta(
        &post_pr,
        "post-pr-review",
        "running",
        "2026-05-13T03:26:40Z",
    );
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

    let outcome = reap_orphans(&root, &orphans);
    assert_eq!(outcome.marked_failed.len(), 1);
    assert_eq!(outcome.marked_failed[0].0, 200);

    let marker = root.join(FEEDBACK_DIR_REPO_RELATIVE).join("200.md.failed");
    assert!(marker.exists());
    let body = std::fs::read_to_string(&marker).unwrap();
    assert!(body.contains("PR #200"));
    assert!(body.contains("abrupt"));
    assert!(body.contains("orphan reaper"));

    let updated_meta: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(run.join("meta.json")).unwrap()).unwrap();
    assert_eq!(
        updated_meta.get("status").and_then(|v| v.as_str()),
        Some("failed")
    );
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
    let outcome = reap_orphans(&root, &orphans);
    assert!(
        outcome.marked_failed.is_empty(),
        "ADR-030 §Reconciliation path: success report exists despite stale meta.json — must not write .failed marker"
    );
    assert!(
        !feedback_dir.join("202.md.failed").exists(),
        "no .failed marker may be written when <pr>.md success report is present"
    );

    let _ = std::fs::remove_dir_all(&root);
}

/// **2026-08-17 incident の核心**: 成功レポートがあって marker を書かない場合でも、
/// meta.json の `status: "running"` は必ず終端状態へ確定させる。ここを skip したため
/// `20260706-...-for-249` が 6 週間 running のまま残り、cli-merge-pipeline の
/// 並行起動 guard が #394 / #408 の feedback を恒久 block した。
#[test]
fn reap_orphans_settles_stale_meta_even_when_it_skips_the_marker() {
    let root = unique_temp_root("settle-on-reconciled-success");
    let runs = root.join(".takt/runs");
    let run = runs.join("20260706-044830-post-merge-feedback-for-249");
    let start_iso = "2026-07-06T04:48:30Z";
    let start_unix = parse_iso8601_to_unix(start_iso).unwrap();
    write_meta(&run, "post-merge-feedback for #249", "running", start_iso);
    write_run_report(&run);
    let now = start_unix + ORPHAN_THRESHOLD_SECS as i64 + 60;

    let feedback_dir = root.join(FEEDBACK_DIR_REPO_RELATIVE);
    std::fs::create_dir_all(&feedback_dir).unwrap();
    std::fs::write(feedback_dir.join("249.md"), "# feedback report").unwrap();

    let orphans = find_orphan_post_merge_feedback_runs(&runs, now);
    let outcome = reap_orphans(&root, &orphans);
    assert_eq!(outcome.settled_only, vec![249]);

    let updated: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(run.join("meta.json")).unwrap()).unwrap();
    assert_eq!(
        updated.get("status").and_then(|v| v.as_str()),
        Some("completed"),
        "この run 自身が feedback-report.md を書いているなら completed へ確定させる"
    );

    assert!(
        find_orphan_post_merge_feedback_runs(&runs, now).is_empty(),
        "確定後は二度と orphan として検出されない (冪等)"
    );

    let _ = std::fs::remove_dir_all(&root);
}

/// **誤帰属の回帰テスト**: feedback が再実行された PR で、先に死んだ run を後続 run の
/// 成果物で「成功」と誤判定しない。
///
/// #374 (2026-08-09) / #281 (2026-07-16) の実例: 1 本目が analyze 開始 34 秒以内に死んで
/// 成果物ゼロ、2 本目が正常完了して `<pr>.md` を書いた。PR 単位の `<pr>.md` を根拠にすると
/// 1 本目まで `completed` になり、手動復旧が実際にその誤りを記録した。
#[test]
fn reap_orphans_settles_a_dead_first_attempt_to_failed_when_a_retry_wrote_the_pr_report() {
    let (root, runs, dead_first_attempt, now) =
        repo_with_dead_first_attempt_and_successful_retry(374);
    let feedback_dir = root.join(FEEDBACK_DIR_REPO_RELATIVE);

    let orphans = find_orphan_post_merge_feedback_runs(&runs, now);
    let outcome = reap_orphans(&root, &orphans);

    assert_eq!(
        outcome.settled_only,
        vec![374],
        "別 run が `<pr>.md` を書いているので nag は不要 (marker を書かない)"
    );
    assert!(
        outcome.marked_failed.is_empty(),
        "再実行が成功しているのに `.failed` marker を書いてはいけない"
    );
    assert!(
        !feedback_dir.join("374.md.failed").exists(),
        "false-positive nag を作らないこと"
    );

    let updated: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(dead_first_attempt.join("meta.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        updated.get("status").and_then(|v| v.as_str()),
        Some("failed"),
        "成果物を出していない run は failed。後続 run の `<pr>.md` を借りて completed にしない"
    );
    assert!(
        updated.get("endTime").is_none(),
        "reaper は完了時刻を観測していないので endTime を捏造しない"
    );

    let _ = std::fs::remove_dir_all(&root);
}

/// L1 の pre-emptive marker を残したまま死んだ run。marker は上書きしないが、
/// status は `failed` へ確定させる。
#[test]
fn reap_orphans_settles_stale_meta_when_a_marker_already_exists() {
    let root = unique_temp_root("settle-on-existing-marker");
    let runs = root.join(".takt/runs");
    let run = runs.join("20260513-100000-post-merge-feedback-for-203");
    let start_iso = "2026-05-13T03:26:40Z";
    let start_unix = parse_iso8601_to_unix(start_iso).unwrap();
    write_meta(&run, "post-merge-feedback for #203", "running", start_iso);
    let now = start_unix + ORPHAN_THRESHOLD_SECS as i64 + 60;

    let feedback_dir = root.join(FEEDBACK_DIR_REPO_RELATIVE);
    std::fs::create_dir_all(&feedback_dir).unwrap();
    let marker = feedback_dir.join("203.md.failed");
    std::fs::write(&marker, "pre-existing detailed marker from L1").unwrap();

    let orphans = find_orphan_post_merge_feedback_runs(&runs, now);
    let outcome = reap_orphans(&root, &orphans);
    assert_eq!(outcome.settled_only, vec![203]);
    assert_eq!(
        std::fs::read_to_string(&marker).unwrap(),
        "pre-existing detailed marker from L1",
        "既存 marker は上書きしない"
    );

    let updated: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(run.join("meta.json")).unwrap()).unwrap();
    assert_eq!(
        updated.get("status").and_then(|v| v.as_str()),
        Some("failed")
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
    let outcome = reap_orphans(&root, &orphans);
    assert!(
        outcome.marked_failed.is_empty(),
        "must not re-reap when marker already exists"
    );

    let body = std::fs::read_to_string(&marker).unwrap();
    assert_eq!(body, "pre-existing detailed marker from L1");

    let _ = std::fs::remove_dir_all(&root);
}

/// status 確定に失敗した run を `settled_only` に数えない。
///
/// 数えてしまうと nudge が「終端状態へ確定させた」と報告する一方 `meta.json` は `running` の
/// まま残り、恒久 block が続いていることに気づけなくなる。
#[test]
fn reap_orphans_does_not_report_a_failed_settle_as_settled() {
    let root = unique_temp_root("settle-failure");
    let run = root.join(".takt/runs/20260513-100000-post-merge-feedback-for-210");
    std::fs::create_dir_all(&run).unwrap();
    std::fs::write(run.join("meta.json"), "not-valid-json{").unwrap();

    let feedback_dir = root.join(FEEDBACK_DIR_REPO_RELATIVE);
    std::fs::create_dir_all(&feedback_dir).unwrap();
    std::fs::write(feedback_dir.join("210.md"), "# feedback report").unwrap();

    let orphans = vec![OrphanRun {
        meta_path: run.join("meta.json"),
        pr_number: 210,
        age_secs: ORPHAN_THRESHOLD_SECS + 60,
        report_directory: None,
    }];
    let outcome = reap_orphans(&root, &orphans);

    assert!(
        outcome.settled_only.is_empty(),
        "書き換えに失敗した run を確定済みとして報告してはいけない"
    );
    assert!(outcome.marked_failed.is_empty());

    let _ = std::fs::remove_dir_all(&root);
}

/// `meta.json` が JSON object でない場合、書き換えていないのに成功を返さない。
#[test]
fn settle_meta_status_fails_when_meta_is_not_a_json_object() {
    let root = unique_temp_root("settle-non-object");
    let run = root.join(".takt/runs/20260513-100000-post-merge-feedback-for-211");
    std::fs::create_dir_all(&run).unwrap();
    let meta_path = run.join("meta.json");
    std::fs::write(&meta_path, "[1, 2, 3]").unwrap();

    let feedback_dir = root.join(FEEDBACK_DIR_REPO_RELATIVE);
    std::fs::create_dir_all(&feedback_dir).unwrap();
    std::fs::write(feedback_dir.join("211.md"), "# feedback report").unwrap();

    let orphans = vec![OrphanRun {
        meta_path: meta_path.clone(),
        pr_number: 211,
        age_secs: ORPHAN_THRESHOLD_SECS + 60,
        report_directory: None,
    }];
    let outcome = reap_orphans(&root, &orphans);

    assert!(
        outcome.settled_only.is_empty(),
        "object でない meta.json を確定済みとして報告してはいけない"
    );
    assert_eq!(
        std::fs::read_to_string(&meta_path).unwrap(),
        "[1, 2, 3]",
        "書き換えられないこと"
    );

    let _ = std::fs::remove_dir_all(&root);
}

/// marker を書いた経路でも status 確定の失敗を握り潰さない。
#[test]
fn reap_orphans_reports_settle_failure_on_the_marker_path() {
    let root = unique_temp_root("marker-path-settle-failure");
    let run = root.join(".takt/runs/20260513-100000-post-merge-feedback-for-212");
    std::fs::create_dir_all(&run).unwrap();
    std::fs::write(run.join("meta.json"), "not-valid-json{").unwrap();

    let orphans = vec![OrphanRun {
        meta_path: run.join("meta.json"),
        pr_number: 212,
        age_secs: ORPHAN_THRESHOLD_SECS + 60,
        report_directory: None,
    }];
    let outcome = reap_orphans(&root, &orphans);

    assert_eq!(
        outcome.marked_failed,
        vec![(212, ORPHAN_THRESHOLD_SECS + 60)],
        "marker は実際に書いたので報告する"
    );
    assert_eq!(
        outcome.settle_failed,
        vec![212],
        "status 確定の失敗を別枠で可視化する (block が続いていることに気づけるように)"
    );

    let _ = std::fs::remove_dir_all(&root);
}

/// `reportDirectory` の `..` で別 run のレポートを成功証拠にできない。
#[test]
fn run_report_path_rejects_parent_dir_escape_to_another_run() {
    let root = unique_temp_root("report-dir-escape");
    let runs = root.join(".takt/runs");
    let dead = runs.join("20260809-055921-post-merge-feedback-for-375");
    let other = runs.join("20260809-063122-post-merge-feedback-for-375");
    let start_iso = "2026-08-09T05:59:21Z";
    write_meta(&dead, "post-merge-feedback for #375", "running", start_iso);
    write_run_report(&other);

    let escaping = OrphanRun {
        meta_path: dead.join("meta.json"),
        pr_number: 375,
        age_secs: ORPHAN_THRESHOLD_SECS + 60,
        report_directory: Some(
            ".takt/runs/20260809-055921-post-merge-feedback-for-375/../\
             20260809-063122-post-merge-feedback-for-375/reports"
                .to_string(),
        ),
    };
    let outcome = reap_orphans(&root, &[escaping]);

    let updated: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dead.join("meta.json")).unwrap()).unwrap();
    assert_eq!(
        updated.get("status").and_then(|v| v.as_str()),
        Some("failed"),
        "`..` で別 run のレポートに到達しても completed にしない"
    );
    assert_eq!(outcome.marked_failed.len(), 1);

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
