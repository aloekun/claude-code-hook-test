use super::*;
use crate::state::RateLimitState;

/// pr_number: None の `PrInfo` — `emit_shortcut_signal_if_eligible` を早期 return させ、
/// テストから実 gh CLI 呼び出し (ネットワーク依存) を発生させないための fixture。
fn pr_info_without_shortcut() -> crate::util::PrInfo {
    crate::util::PrInfo {
        pr_number: None,
        repo: None,
        push_time: None,
        head_commit: None,
        fix_push_time: None,
    }
}

#[test]
fn rate_limit_state_persists_retries_across_polls() {
    let tmp = std::env::temp_dir().join(format!("test-rl-retries-{}.json", std::process::id()));
    let mut state = PrMonitorState::new(Some(1), Some("o/r".into()), "t".into());
    state.rate_limit_retries = 2;
    state.rate_limit = Some(RateLimitState {
        until_unix_secs: 1_735_689_600,
        comment_event_time: "2026-04-30T00:00:00Z".into(),
        wait_minutes: 5,
        wait_seconds: 13,
    });
    crate::state::write_state_to(&tmp, &state).unwrap();

    let loaded = crate::state::read_state_from(&tmp).unwrap();
    assert_eq!(loaded.rate_limit_retries, 2);
    assert_eq!(
        loaded.rate_limit.as_ref().unwrap().until_unix_secs,
        1_735_689_600
    );

    let _ = std::fs::remove_file(&tmp);
}

#[test]
fn rate_limit_default_config_allows_retry_within_limit() {
    let cfg = RateLimitConfig::default();
    assert!(cfg.auto_retry_enabled);
    assert_eq!(cfg.max_retries, 3);
}

/// 同じ rate-limit comment が invocation 跨ぎで残った場合に dedup が働くことを検証する。
#[test]
fn rate_limit_dedup_skips_repeated_comment() {
    let comment_a = "2026-04-30T00:00:00Z";
    let comment_b = "2026-04-30T00:30:00Z";

    let mut state = PrMonitorState::new(Some(1), Some("o/r".into()), "t".into());
    let rl_a = RateLimitState {
        until_unix_secs: 0,
        comment_event_time: comment_a.into(),
        wait_minutes: 5,
        wait_seconds: 0,
    };
    let already_handled_iter1 = state.rate_limit_last_retriggered_at.as_deref()
        == Some(rl_a.comment_event_time.as_str());
    assert!(!already_handled_iter1, "初回 detection は handle されるべき");

    state.rate_limit_retries = 1;
    state.rate_limit_last_retriggered_at = Some(comment_a.into());

    let already_handled_iter2 = state.rate_limit_last_retriggered_at.as_deref()
        == Some(rl_a.comment_event_time.as_str());
    assert!(
        already_handled_iter2,
        "同じ comment は dedup で skip されるべき"
    );

    let rl_b = RateLimitState {
        until_unix_secs: 0,
        comment_event_time: comment_b.into(),
        wait_minutes: 5,
        wait_seconds: 0,
    };
    let already_handled_iter3 = state.rate_limit_last_retriggered_at.as_deref()
        == Some(rl_b.comment_event_time.as_str());
    assert!(!already_handled_iter3, "新 comment は再度 handle 対象");
}

/// state.json round-trip で rate_limit_last_retriggered_at が persistence される。
#[test]
fn rate_limit_last_retriggered_at_persists_across_polls() {
    let tmp =
        std::env::temp_dir().join(format!("test-rl-last-handled-{}.json", std::process::id()));
    let mut state = PrMonitorState::new(Some(1), Some("o/r".into()), "t".into());
    state.rate_limit_last_retriggered_at = Some("2026-04-30T00:00:00Z".into());
    crate::state::write_state_to(&tmp, &state).unwrap();

    let loaded = crate::state::read_state_from(&tmp).unwrap();
    assert_eq!(
        loaded.rate_limit_last_retriggered_at.as_deref(),
        Some("2026-04-30T00:00:00Z")
    );

    let _ = std::fs::remove_file(&tmp);
}

/// PR 3 (wakeup 廃止): reset 時刻が未来の場合、`handle_rate_limit_retry` は
/// WaitingReset を返し state.rate_limit_retries を変更しない (旧 Parked 相当。
/// wakeup 時刻は返さない — park しないため不要)。
#[test]
fn rate_limit_retry_returns_waiting_reset_when_reset_in_future() {
    let future_unix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
        + 600;
    let rl = RateLimitState {
        until_unix_secs: future_unix,
        comment_event_time: "2026-04-30T00:00:00Z".into(),
        wait_minutes: 10,
        wait_seconds: 0,
    };
    let mut state = PrMonitorState::new(Some(42), Some("o/r".into()), "t".into());
    let pr_info = crate::util::PrInfo {
        pr_number: Some(42),
        repo: Some("o/r".into()),
        push_time: None,
        head_commit: None,
        fix_push_time: None,
    };

    let outcome = handle_rate_limit_retry(&rl, &mut state, &pr_info);
    assert!(matches!(outcome, RateLimitOutcome::WaitingReset));
    assert_eq!(state.rate_limit_retries, 0);
    assert!(state.rate_limit_last_retriggered_at.is_none());
}

/// PR 番号未確定の場合、`handle_rate_limit_retry` は Failed を返し
/// state を変更しない (caller は action_required で抜ける)。
#[test]
fn rate_limit_retry_returns_failed_when_pr_number_missing() {
    let past_unix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
        - 60;
    let rl = RateLimitState {
        until_unix_secs: past_unix,
        comment_event_time: "2026-04-30T00:00:00Z".into(),
        wait_minutes: 0,
        wait_seconds: 0,
    };
    let mut state = PrMonitorState::new(None, None, "t".into());
    let pr_info = crate::util::PrInfo {
        pr_number: None,
        repo: None,
        push_time: None,
        head_commit: None,
        fix_push_time: None,
    };

    let outcome = handle_rate_limit_retry(&rl, &mut state, &pr_info);
    assert!(matches!(outcome, RateLimitOutcome::Failed(_)));
    assert_eq!(state.rate_limit_retries, 0);
    assert!(state.rate_limit_last_retriggered_at.is_none());
}

/// PR 3 (wakeup 廃止): reset 未来の rate-limit は terminal な `rate_limited` action で
/// 報告される。summary は保留 (レビュー未実施) と後続の引き継ぎ先を明示する
/// (ADR-064 (b): レポート判定文の保留保証を Actions 経路へ引き継ぐまでの間も維持)。
#[test]
fn finalize_waiting_reset_reports_rate_limited_terminal() {
    let tmp = tempfile::tempdir().unwrap();
    let state_path = tmp.path().join("state.json");
    let mut state = PrMonitorState::new(Some(42), Some("o/r".into()), "t".into());
    state.rate_limit = Some(RateLimitState {
        until_unix_secs: 9_999_999_999,
        comment_event_time: "2026-08-03T00:00:00Z".into(),
        wait_minutes: 47,
        wait_seconds: 10,
    });
    let rl = state.rate_limit.clone().unwrap();
    let pr_info = pr_info_without_shortcut();

    let outcome = finalize_waiting_reset(
        &mut state,
        &rl,
        &pr_info,
        &serde_json::Value::Null,
        &state_path,
    );

    assert_eq!(outcome.action, "rate_limited");
    assert!(
        outcome.summary.contains("rate-limit 中"),
        "レビュー未実施であることを明示: {}",
        outcome.summary
    );
    assert!(
        outcome.summary.contains("GitHub Actions"),
        "後続の引き継ぎ先を明示: {}",
        outcome.summary
    );
    assert!(
        outcome.rate_limit.is_some(),
        "rate_limit を伝播し caller (monitor.rs) が takt invoke を skip できること (#C-3)"
    );
    let persisted = crate::state::read_state_from(&state_path).unwrap();
    assert_eq!(persisted.action, "rate_limited");
}

/// state-continuity-drop 回帰 (SIM-NEW-rate_limit-L154): `finalize_waiting_reset` は
/// state.head_commit を pr_info から persist する。これが欠けると
/// `should_continue_state` (monitor.rs) が次回 `--monitor-only` 再実行時に
/// legacy state (head_commit None) 扱いで fresh 初期化に倒れ、push_time /
/// fix_push_time が「今」にリセットされて間に届いた CR コメントを取りこぼす。
#[test]
fn finalize_waiting_reset_persists_head_commit() {
    let tmp = tempfile::tempdir().unwrap();
    let state_path = tmp.path().join("state.json");
    let mut state = PrMonitorState::new(Some(42), Some("o/r".into()), "t".into());
    state.rate_limit = Some(RateLimitState {
        until_unix_secs: 9_999_999_999,
        comment_event_time: "2026-08-03T00:00:00Z".into(),
        wait_minutes: 47,
        wait_seconds: 10,
    });
    let rl = state.rate_limit.clone().unwrap();
    let pr_info = crate::util::PrInfo {
        pr_number: Some(42),
        repo: Some("o/r".into()),
        push_time: None,
        head_commit: Some("deadbeef".into()),
        fix_push_time: None,
    };

    let outcome = finalize_waiting_reset(
        &mut state,
        &rl,
        &pr_info,
        &serde_json::Value::Null,
        &state_path,
    );

    assert_eq!(outcome.action, "rate_limited");
    assert_eq!(state.head_commit.as_deref(), Some("deadbeef"));
    let persisted = crate::state::read_state_from(&state_path).unwrap();
    assert_eq!(
        persisted.head_commit.as_deref(),
        Some("deadbeef"),
        "head_commit が persist されないと should_continue_state が次回 fresh 初期化に倒れる"
    );
}

/// fail-open 回帰 (SIM-NEW-rate_limit-L158): state 書き込み失敗でも
/// `finalize_waiting_reset` は `action_required` に倒れず通常どおり
/// `rate_limited` を返す (`finalize_pending_review` と同じ fail-open 方針。
/// 副作用を伴わないこの経路は checker 判定を書き込み失敗で破棄しない)。
#[test]
fn finalize_waiting_reset_survives_write_failure() {
    let bad_path = std::env::temp_dir()
        .join(format!("test-rl-waiting-reset-fail-{}", std::process::id()))
        .join("nonexistent-dir")
        .join("state.json");
    let mut state = PrMonitorState::new(Some(42), Some("o/r".into()), "t".into());
    state.rate_limit = Some(RateLimitState {
        until_unix_secs: 9_999_999_999,
        comment_event_time: "2026-08-03T00:00:00Z".into(),
        wait_minutes: 47,
        wait_seconds: 10,
    });
    let rl = state.rate_limit.clone().unwrap();
    let pr_info = pr_info_without_shortcut();

    let outcome = finalize_waiting_reset(
        &mut state,
        &rl,
        &pr_info,
        &serde_json::Value::Null,
        &bad_path,
    );

    assert_eq!(
        outcome.action, "rate_limited",
        "state 書き込み失敗時も action_required に倒れず rate_limited を維持すること"
    );
}

/// state-continuity-drop 回帰 (SIM-NEW-rate_limit-L154): `finalize_posted_retrigger` も
/// 同様に head_commit を persist する (retrigger 投稿後の継続判定も同じ invariant に従う)。
#[test]
fn finalize_posted_retrigger_persists_head_commit() {
    let tmp = tempfile::tempdir().unwrap();
    let state_path = tmp.path().join("state.json");
    let mut state = PrMonitorState::new(Some(42), Some("o/r".into()), "t".into());
    let rl = RateLimitState {
        until_unix_secs: 0,
        comment_event_time: "2026-08-03T00:00:00Z".into(),
        wait_minutes: 0,
        wait_seconds: 0,
    };
    let pr_info = crate::util::PrInfo {
        pr_number: Some(42),
        repo: Some("o/r".into()),
        push_time: None,
        head_commit: Some("cafef00d".into()),
        fix_push_time: None,
    };

    let outcome =
        finalize_posted_retrigger(&mut state, &rl, &pr_info, &serde_json::Value::Null, &state_path);

    assert_eq!(outcome.action, "pending_review");
    assert_eq!(state.head_commit.as_deref(), Some("cafef00d"));
    let persisted = crate::state::read_state_from(&state_path).unwrap();
    assert_eq!(
        persisted.head_commit.as_deref(),
        Some("cafef00d"),
        "head_commit が persist されないと should_continue_state が次回 fresh 初期化に倒れる"
    );
}

/// dedup 済み (同一 comment 処理済み) の rate-limit も terminal な rate_limited に
/// 落ちること — 旧実装はここで None を返し park に流れていた。
#[test]
fn handle_rate_limit_branch_terminalizes_already_handled_comment() {
    let tmp = tempfile::tempdir().unwrap();
    let state_path = tmp.path().join("state.json");
    let mut state = PrMonitorState::new(Some(42), Some("o/r".into()), "t".into());
    state.rate_limit = Some(RateLimitState {
        until_unix_secs: 9_999_999_999,
        comment_event_time: "2026-08-03T00:00:00Z".into(),
        wait_minutes: 5,
        wait_seconds: 0,
    });
    state.rate_limit_last_retriggered_at = Some("2026-08-03T00:00:00Z".into());
    let pr_info = pr_info_without_shortcut();

    let outcome = handle_rate_limit_branch(
        &mut state,
        &RateLimitConfig::default(),
        &pr_info,
        &serde_json::Value::Null,
        &state_path,
    );

    let terminal = outcome.expect("rate-limit active なら必ず terminal を返す (park しない)");
    assert_eq!(terminal.action, "rate_limited");
}

/// 順位 141: shortcut signal の trigger 条件 (mergeable CLEAN + unresolved 0) で true。
#[test]
fn evaluate_rate_limit_shortcut_when_all_conditions_met() {
    let m = MergeableStatus {
        mergeable: "MERGEABLE".into(),
        merge_state: "CLEAN".into(),
    };
    let cr = crate::state::CodeRabbitState {
        review_state: "approved".into(),
        new_comments: 0,
        actionable_comments: Some(0),
        unresolved_threads: Some(0),
    };
    assert!(evaluate_rate_limit_shortcut(Some(&cr), &m));
}

/// 順位 141: unresolved thread が残っていれば shortcut を抑止 (CR の指摘が未対応)。
#[test]
fn evaluate_rate_limit_shortcut_blocks_when_unresolved_threads_exist() {
    let m = MergeableStatus {
        mergeable: "MERGEABLE".into(),
        merge_state: "CLEAN".into(),
    };
    let cr = crate::state::CodeRabbitState {
        review_state: "commented".into(),
        new_comments: 1,
        actionable_comments: Some(1),
        unresolved_threads: Some(1),
    };
    assert!(!evaluate_rate_limit_shortcut(Some(&cr), &m));
}

/// 順位 141: new_comments > 0 のとき unresolved_threads が 0 でも shortcut を抑止。
/// CR がまだコメントを処理中の状態で merge 判定を通過させない。
#[test]
fn evaluate_rate_limit_shortcut_blocks_when_new_comments_exist() {
    let m = MergeableStatus {
        mergeable: "MERGEABLE".into(),
        merge_state: "CLEAN".into(),
    };
    let cr = crate::state::CodeRabbitState {
        review_state: "commented".into(),
        new_comments: 1,
        actionable_comments: Some(0),
        unresolved_threads: Some(0),
    };
    assert!(!evaluate_rate_limit_shortcut(Some(&cr), &m));
}

/// 順位 141: mergeable が BLOCKED なら shortcut を抑止 (GitHub 側で merge 不可)。
#[test]
fn evaluate_rate_limit_shortcut_blocks_when_not_mergeable() {
    let m = MergeableStatus {
        mergeable: "BLOCKED".into(),
        merge_state: "BLOCKED".into(),
    };
    assert!(!evaluate_rate_limit_shortcut(None, &m));
}

/// 順位 141: CR state が None (初回 review なし) でも mergeable CLEAN なら shortcut 可。
#[test]
fn evaluate_rate_limit_shortcut_passes_when_coderabbit_none() {
    let m = MergeableStatus {
        mergeable: "MERGEABLE".into(),
        merge_state: "CLEAN".into(),
    };
    assert!(evaluate_rate_limit_shortcut(None, &m));
}

/// 順位 141: signal format に必須 field が全て含まれ、Claude が AskUserQuestion 化できる。
#[test]
fn format_shortcut_signal_includes_required_fields() {
    let rl = RateLimitState {
        until_unix_secs: 1_779_432_672,
        comment_event_time: "2026-05-22T06:08:02Z".into(),
        wait_minutes: 38,
        wait_seconds: 30,
    };
    let pr_info = crate::util::PrInfo {
        pr_number: Some(169),
        repo: Some("aloekun/claude-code-hook-test".into()),
        push_time: None,
        head_commit: None,
        fix_push_time: None,
    };
    let m = MergeableStatus {
        mergeable: "MERGEABLE".into(),
        merge_state: "CLEAN".into(),
    };
    let sig = format_shortcut_signal(&rl, &pr_info, &m);
    assert!(sig.starts_with("[RATE_LIMIT_BUT_MERGEABLE]"));
    assert!(sig.contains("[/RATE_LIMIT_BUT_MERGEABLE]"));
    assert!(sig.contains("pr: 169"));
    assert!(sig.contains("repo: aloekun/claude-code-hook-test"));
    assert!(sig.contains("rate_limit_wait_seconds: 2310"));
    assert!(sig.contains("mergeable: MERGEABLE"));
    assert!(sig.contains("merge_state: CLEAN"));
    assert!(sig.contains("AskUserQuestion"));
}
