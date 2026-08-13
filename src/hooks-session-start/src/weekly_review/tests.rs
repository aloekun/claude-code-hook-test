use super::*;
use std::path::PathBuf;

fn unique_temp_root(prefix: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!(
        "weekly-review-{}-{}-{}",
        prefix,
        std::process::id(),
        nanos
    ))
}

#[test]
fn compute_weekly_review_reminder_nudge_returns_none_when_disabled() {
    let root = unique_temp_root("disabled");
    std::fs::create_dir_all(&root).unwrap();
    let config = WeeklyReviewReminderConfig {
        enabled: Some(false),
        reminder_threshold_days: Some(7),
        failed_marker_check_enabled: Some(true),
        system_message_enabled: Some(false),
    };
    assert!(compute_weekly_review_reminder_nudge(&root, &config, 2_000_000_000).is_none());
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn weekly_review_failed_markers_returns_empty_when_dir_missing() {
    let root = unique_temp_root("no-dir");
    std::fs::create_dir_all(&root).unwrap();
    let markers = weekly_review_failed_markers(&root);
    assert!(markers.is_empty());
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn weekly_review_failed_markers_lists_failed_md_files_only() {
    let root = unique_temp_root("markers");
    let reviews_dir = root.join(".claude/weekly-reviews");
    std::fs::create_dir_all(&reviews_dir).unwrap();
    std::fs::write(reviews_dir.join("2026-05-22.md.failed"), "fail1").unwrap();
    std::fs::write(reviews_dir.join("2026-05-29.md.failed"), "fail2").unwrap();
    std::fs::write(reviews_dir.join("2026-05-29.md"), "report").unwrap();
    let markers = weekly_review_failed_markers(&root);
    assert_eq!(markers.len(), 2);
    assert!(markers.contains(&"2026-05-22.md.failed".to_string()));
    assert!(markers.contains(&"2026-05-29.md.failed".to_string()));
    assert!(!markers.contains(&"2026-05-29.md".to_string()));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn compute_weekly_review_reminder_nudge_emits_staleness_when_never_run() {
    let root = unique_temp_root("staleness-never");
    std::fs::create_dir_all(&root).unwrap();
    let config = WeeklyReviewReminderConfig {
        enabled: Some(true),
        reminder_threshold_days: Some(7),
        failed_marker_check_enabled: Some(false),
        system_message_enabled: Some(false),
    };
    let nudge = compute_weekly_review_reminder_nudge(&root, &config, 2_000_000_000)
        .expect("staleness nudge must be emitted when last-run file missing");
    assert!(nudge.additional_context.contains("[WEEKLY_REVIEW_REMINDER]"));
    assert!(nudge.additional_context.contains("threshold (7 日)"));
    assert!(nudge.additional_context.contains("未実行"));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn compute_weekly_review_reminder_nudge_emits_failed_marker_when_present() {
    let root = unique_temp_root("failed-only");
    let reviews_dir = root.join(".claude/weekly-reviews");
    std::fs::create_dir_all(&reviews_dir).unwrap();
    std::fs::write(reviews_dir.join("2026-05-15.md.failed"), "fail").unwrap();
    let last_run_str = "2026-06-01T00:00:00Z";
    let then = parse_iso8601_to_unix(last_run_str).unwrap();
    let now = then + 2 * 86_400;
    std::fs::write(
        root.join(WEEKLY_REVIEW_LAST_RUN_PATH),
        format!("{{\"last_run_at\": \"{}\"}}", last_run_str),
    )
    .unwrap();
    let config = WeeklyReviewReminderConfig {
        enabled: Some(true),
        reminder_threshold_days: Some(7),
        failed_marker_check_enabled: Some(true),
        system_message_enabled: Some(false),
    };
    let nudge = compute_weekly_review_reminder_nudge(&root, &config, now)
        .expect("failed marker nudge must be emitted");
    assert!(nudge.additional_context.contains("[WEEKLY_REVIEW_REMINDER]"));
    assert!(nudge.additional_context.contains(".failed` marker が 1 件残存"));
    assert!(nudge.additional_context.contains("2026-05-15.md.failed"));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn last_run_read_from_main_root_while_markers_stay_workspace_local() {
    let base = unique_temp_root("main-root-split");
    let main = base.join("main");
    let ws = base.join("ws");
    let last_run_str = "2026-06-01T00:00:00Z";
    let then = parse_iso8601_to_unix(last_run_str).unwrap();
    let now = then + 18 * 86_400;
    std::fs::create_dir_all(main.join(".claude")).unwrap();
    std::fs::write(
        main.join(WEEKLY_REVIEW_LAST_RUN_PATH),
        format!("{{\"last_run_at\": \"{}\"}}", last_run_str),
    )
    .unwrap();
    std::fs::create_dir_all(ws.join(".jj")).unwrap();
    std::fs::write(ws.join(".jj/repo"), "../../main/.jj/repo").unwrap();
    std::fs::create_dir_all(ws.join(".claude/weekly-reviews")).unwrap();
    std::fs::write(
        ws.join(".claude/weekly-reviews/2026-05-15.md.failed"),
        "fail",
    )
    .unwrap();
    let config = WeeklyReviewReminderConfig {
        enabled: Some(true),
        reminder_threshold_days: Some(7),
        failed_marker_check_enabled: Some(true),
        system_message_enabled: Some(true),
    };
    let nudge = compute_weekly_review_reminder_nudge(&ws, &config, now)
        .expect("secondary workspace でもメイン root の last-run で発火する");
    assert!(
        nudge.additional_context.contains("18 日経過"),
        "last-run はメイン workspace root から読む (secondary の未実行に fallback しない): {}",
        nudge.additional_context
    );
    assert!(
        nudge.additional_context.contains("2026-05-15.md.failed"),
        "failed marker は現 workspace ローカルから読む"
    );
    let msg = nudge
        .system_message
        .expect("system_message_enabled = true なので systemMessage が付く");
    assert!(
        msg.as_str().contains("18 日経過"),
        "systemMessage も main-root 由来の経過日数: {}",
        msg
    );
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn compute_weekly_review_reminder_nudge_uses_last_run_at_over_fresh_mtime() {
    let root = unique_temp_root("last-run-at-stale");
    let last_run_path = root.join(WEEKLY_REVIEW_LAST_RUN_PATH);
    std::fs::create_dir_all(last_run_path.parent().unwrap()).unwrap();
    let last_run_str = "2026-06-01T00:00:00Z";
    let then = parse_iso8601_to_unix(last_run_str).unwrap();
    let now = then + 40 * 86_400;
    std::fs::write(
        &last_run_path,
        format!("{{\"last_run_at\": \"{}\"}}", last_run_str),
    )
    .unwrap();
    let config = WeeklyReviewReminderConfig {
        enabled: Some(true),
        reminder_threshold_days: Some(7),
        failed_marker_check_enabled: Some(false),
        system_message_enabled: Some(false),
    };
    let nudge = compute_weekly_review_reminder_nudge(&root, &config, now)
        .expect("40 日前の last_run_at は fresh な mtime に関わらず staleness を発火させる");
    assert!(nudge.additional_context.contains("[WEEKLY_REVIEW_REMINDER]"));
    assert!(nudge.additional_context.contains("40 日経過"));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn compute_weekly_review_reminder_nudge_recent_last_run_at_skips_staleness() {
    let root = unique_temp_root("last-run-at-recent");
    let last_run_path = root.join(WEEKLY_REVIEW_LAST_RUN_PATH);
    std::fs::create_dir_all(last_run_path.parent().unwrap()).unwrap();
    let last_run_str = "2026-06-01T00:00:00Z";
    let then = parse_iso8601_to_unix(last_run_str).unwrap();
    let now = then + 2 * 86_400;
    std::fs::write(
        &last_run_path,
        format!("{{\"last_run_at\": \"{}\"}}", last_run_str),
    )
    .unwrap();
    let config = WeeklyReviewReminderConfig {
        enabled: Some(true),
        reminder_threshold_days: Some(7),
        failed_marker_check_enabled: Some(false),
        system_message_enabled: Some(false),
    };
    assert!(
        compute_weekly_review_reminder_nudge(&root, &config, now).is_none(),
        "2 日前の last_run_at は threshold (7 日) 未満なので発火しない"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn last_run_state_from_content_prefers_last_run_at() {
    let then = parse_iso8601_to_unix("2026-06-01T00:00:00Z").unwrap();
    let now = then + 10 * 86_400;
    let content = "{\"last_run_at\": \"2026-06-01T00:00:00Z\"}";
    match last_run_state_from_content(content, now) {
        Some(WeeklyLastRunState::ElapsedDays(d)) => assert_eq!(d, 10),
        _ => panic!("expected ElapsedDays(10) derived from last_run_at"),
    }
}

#[test]
fn last_run_state_from_content_none_when_field_absent() {
    assert!(last_run_state_from_content("{}", 2_000_000_000).is_none());
}

#[test]
fn last_run_state_from_content_none_when_unparseable() {
    assert!(
        last_run_state_from_content("{\"last_run_at\": \"not-a-date\"}", 2_000_000_000)
            .is_none()
    );
}

#[test]
fn last_run_state_from_content_none_when_future() {
    let now = parse_iso8601_to_unix("2026-06-01T00:00:00Z").unwrap();
    let content = "{\"last_run_at\": \"2026-06-02T00:00:00Z\"}";
    assert!(
        last_run_state_from_content(content, now).is_none(),
        "未来 timestamp は None を返し caller が Stale 扱いにする (silent-fresh 防止)"
    );
}

#[test]
fn compute_weekly_review_reminder_nudge_treats_missing_last_run_at_as_stale() {
    let root = unique_temp_root("missing-last-run-at");
    let last_run_path = root.join(WEEKLY_REVIEW_LAST_RUN_PATH);
    std::fs::create_dir_all(last_run_path.parent().unwrap()).unwrap();
    std::fs::write(&last_run_path, "{}").unwrap();
    let config = WeeklyReviewReminderConfig {
        enabled: Some(true),
        reminder_threshold_days: Some(7),
        failed_marker_check_enabled: Some(false),
        system_message_enabled: Some(false),
    };
    let nudge = compute_weekly_review_reminder_nudge(&root, &config, 2_000_000_000).expect(
        "last_run_at 欠落は mtime にフォールバックせず stale 扱いで発火する (CR #233 Major)",
    );
    assert!(nudge.additional_context.contains("[WEEKLY_REVIEW_REMINDER]"));
    assert!(nudge.additional_context.contains("stale 扱い"));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn weekly_review_staleness_hits_for_missing_state() {
    assert!(weekly_review_staleness_hits(
        &WeeklyLastRunState::Missing,
        7
    ));
}

#[test]
fn weekly_review_staleness_hits_for_stale_state() {
    assert!(weekly_review_staleness_hits(&WeeklyLastRunState::Stale, 7));
}

#[test]
fn weekly_review_staleness_hits_for_elapsed_above_threshold() {
    assert!(weekly_review_staleness_hits(
        &WeeklyLastRunState::ElapsedDays(10),
        7
    ));
}

#[test]
fn weekly_review_staleness_skips_for_elapsed_below_threshold() {
    assert!(!weekly_review_staleness_hits(
        &WeeklyLastRunState::ElapsedDays(3),
        7
    ));
}

#[test]
fn weekly_review_staleness_skips_for_unreadable_state() {
    assert!(!weekly_review_staleness_hits(
        &WeeklyLastRunState::Unreadable,
        7
    ));
}

#[test]
fn system_message_is_some_when_enabled_and_never_run() {
    let root = unique_temp_root("sysmsg-never");
    std::fs::create_dir_all(&root).unwrap();
    let config = WeeklyReviewReminderConfig {
        enabled: Some(true),
        reminder_threshold_days: Some(7),
        failed_marker_check_enabled: Some(false),
        system_message_enabled: Some(true),
    };
    let nudge = compute_weekly_review_reminder_nudge(&root, &config, 2_000_000_000)
        .expect("nudge must fire when last-run file missing");
    let msg = nudge
        .system_message
        .expect("system_message_enabled = true なので systemMessage が付く");
    assert!(msg.as_str().contains("週次レビュー監査"));
    assert!(
        msg.as_str().contains("ローカル実行の記録なし"),
        "ADR-070: staleness は**ローカル**実行の記録に限定した表現であること (cloud routine の\
         実行は観測できないため「未実行」と断定しない): {}",
        msg
    );
    assert!(
        msg.as_str().contains("routine"),
        "監査対象が routine であることを示すこと: {}",
        msg
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn system_message_reports_elapsed_days_when_enabled() {
    let root = unique_temp_root("sysmsg-elapsed");
    let last_run_path = root.join(WEEKLY_REVIEW_LAST_RUN_PATH);
    std::fs::create_dir_all(last_run_path.parent().unwrap()).unwrap();
    let last_run_str = "2026-06-01T00:00:00Z";
    let then = parse_iso8601_to_unix(last_run_str).unwrap();
    let now = then + 18 * 86_400;
    std::fs::write(
        &last_run_path,
        format!("{{\"last_run_at\": \"{}\"}}", last_run_str),
    )
    .unwrap();
    let config = WeeklyReviewReminderConfig {
        enabled: Some(true),
        reminder_threshold_days: Some(7),
        failed_marker_check_enabled: Some(false),
        system_message_enabled: Some(true),
    };
    let nudge = compute_weekly_review_reminder_nudge(&root, &config, now)
        .expect("18 日経過で nudge が発火する");
    let msg = nudge.system_message.expect("systemMessage が付く");
    assert!(msg.as_str().contains("18 日経過"));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn system_message_is_none_when_disabled_but_additional_context_still_fires() {
    let root = unique_temp_root("sysmsg-off");
    std::fs::create_dir_all(&root).unwrap();
    let config = WeeklyReviewReminderConfig {
        enabled: Some(true),
        reminder_threshold_days: Some(7),
        failed_marker_check_enabled: Some(false),
        system_message_enabled: Some(false),
    };
    let nudge = compute_weekly_review_reminder_nudge(&root, &config, 2_000_000_000)
        .expect("system_message_enabled = false でも additionalContext の nudge は発火する");
    assert!(nudge.system_message.is_none());
    assert!(nudge
        .additional_context
        .contains("[WEEKLY_REVIEW_REMINDER]"));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn additional_context_includes_tell_user_instruction() {
    let root = unique_temp_root("tell-user");
    std::fs::create_dir_all(&root).unwrap();
    let config = WeeklyReviewReminderConfig {
        enabled: Some(true),
        reminder_threshold_days: Some(7),
        failed_marker_check_enabled: Some(false),
        system_message_enabled: Some(false),
    };
    let nudge = compute_weekly_review_reminder_nudge(&root, &config, 2_000_000_000)
        .expect("nudge fires");
    assert!(
        nudge
            .additional_context
            .contains("ユーザーに一言伝えること"),
        "ADR-059 defense-in-depth の明示指示が additionalContext に含まれる"
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// 既定 threshold は週次サイクル (7 日) で、**config 行が無い環境でも週次で鳴る**。
///
/// 境界は定数を直接渡すのではなく `reminder_threshold_days: None` から
/// `compute_weekly_review_reminder_nudge` が既定へ解決する**実際の経路**で確認する。
/// 定数値だけを assert すると、解決側が `unwrap_or(30)` 等へ書き換わっても検知できず、
/// 本 PR が主張する「config 行が無くても週次で鳴る」が無検証のまま壊れうる。
///
/// weekly と monthly の既定が**独立**であることも同時に固定する — 片方を直したつもりで
/// もう片方も動く事故を防ぐため、どちらを変更しても本テストが落ちて「片方だけの意図か」を
/// 明示的に判断させる (旧 weekly 既定 30 は monthly の 28 と近く混同を招いた)。
#[test]
fn default_threshold_is_weekly_and_independent_from_monthly() {
    use crate::monthly_review::MONTHLY_REVIEW_DEFAULT_THRESHOLD_DAYS;
    assert_eq!(WEEKLY_REVIEW_DEFAULT_THRESHOLD_DAYS, 7, "weekly = 週次");
    assert_eq!(MONTHLY_REVIEW_DEFAULT_THRESHOLD_DAYS, 28, "monthly = 4 週間");
    assert_ne!(
        WEEKLY_REVIEW_DEFAULT_THRESHOLD_DAYS, MONTHLY_REVIEW_DEFAULT_THRESHOLD_DAYS,
        "別周期。片方の変更をもう片方へ波及させてはならない"
    );

    let root = unique_temp_root("default-fallback");
    std::fs::create_dir_all(root.join(".claude")).unwrap();
    let last_run_str = "2026-06-01T00:00:00Z";
    let then = parse_iso8601_to_unix(last_run_str).unwrap();
    std::fs::write(
        root.join(WEEKLY_REVIEW_LAST_RUN_PATH),
        format!("{{\"last_run_at\": \"{}\"}}", last_run_str),
    )
    .unwrap();
    let config = WeeklyReviewReminderConfig {
        enabled: Some(true),
        reminder_threshold_days: None,
        failed_marker_check_enabled: Some(false),
        system_message_enabled: Some(false),
    };
    assert!(
        compute_weekly_review_reminder_nudge(&root, &config, then + 6 * 86_400).is_none(),
        "config 未指定 + 6 日経過では発火しない (既定 7 日へ解決されている)"
    );
    assert!(
        compute_weekly_review_reminder_nudge(&root, &config, then + 7 * 86_400).is_some(),
        "config 未指定 + 7 日経過で発火する (既定 7 日へ解決されている)"
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// ADR-070: additionalContext は「本 reminder が cloud routine を観測できない」ことと、
/// routine の稼働確認を第一アクションとすることを明示する。これが無いと、routine が
/// 正常でも「レビュー未実施」と読める旧文言に戻り、ユーザーを誤誘導する。
#[test]
fn additional_context_states_routine_is_unobservable_and_primary() {
    let root = unique_temp_root("routine-framing");
    std::fs::create_dir_all(&root).unwrap();
    let config = WeeklyReviewReminderConfig {
        enabled: Some(true),
        reminder_threshold_days: Some(30),
        failed_marker_check_enabled: Some(false),
        system_message_enabled: Some(false),
    };
    let nudge = compute_weekly_review_reminder_nudge(&root, &config, 2_000_000_000)
        .expect("nudge fires");
    let ctx = &nudge.additional_context;
    assert!(
        ctx.contains("cloud routine の実行を観測できません"),
        "観測不能であることの明示が必要 (誤誘導防止): {}",
        ctx
    );
    assert!(
        ctx.contains("claude.ai/code/routines"),
        "稼働確認先の導線が必要: {}",
        ctx
    );
    assert!(
        ctx.contains("ローカル"),
        "staleness がローカル実行に限定された指標であることを示すこと: {}",
        ctx
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn build_weekly_review_system_message_none_when_fresh_and_no_marker() {
    assert!(
        build_weekly_review_system_message(&WeeklyLastRunState::ElapsedDays(3), 7, 0).is_none()
    );
}

#[test]
fn build_weekly_review_system_message_combines_staleness_and_marker() {
    let msg = build_weekly_review_system_message(&WeeklyLastRunState::Missing, 7, 2)
        .expect("staleness or marker があれば Some");
    assert!(msg.as_str().contains("ローカル実行の記録なし"));
    assert!(msg.as_str().contains("失敗"));
    assert!(msg.as_str().contains("2 件"));
}
